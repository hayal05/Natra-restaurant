//! Reporting: monthly revenue/expenses/costs, product performance,
//! sales mix, and cash flow.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::services::financial_service::{self, ProfitSummary};

fn month_range(year: i32, month: u32) -> AppResult<(String, String)> {
    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| AppError::Validation("invalid year/month".into()))?;
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let next_start = NaiveDate::from_ymd_opt(next_year, next_month, 1).ok_or_else(|| AppError::Validation("invalid year/month".into()))?;
    let end = next_start.pred_opt().ok_or_else(|| AppError::Internal("date overflow".into()))?;
    Ok((format!("{start} 00:00:00"), format!("{end} 23:59:59")))
}

pub fn monthly_report(conn: &Connection, year: i32, month: u32) -> AppResult<ProfitSummary> {
    let (start, end) = month_range(year, month)?;
    financial_service::calculate_profit(conn, Some(&start), Some(&end))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyEstimatedProfit {
    pub status: String,
    pub sales: f64,
    pub ready_made_costs: f64,
    pub raw_material_purchases: f64,
    pub opening_stock_value: Option<f64>,
    pub closing_stock_value: Option<f64>,
    pub raw_material_cost_used: Option<f64>,
    pub other_expenses: f64,
    pub estimated_net_profit: Option<f64>,
}

/// Only finalized material closings can yield an estimated net profit.
/// The existing purchase-based financial report remains available for
/// operational trends; this method never invents a closing stock value.
pub fn monthly_estimated_profit(conn: &Connection, year: i32, month: u32) -> AppResult<MonthlyEstimatedProfit> {
    // Convert Ethiopia-local month boundaries to the UTC timestamps stored by SQLite.
    // Existing general-purpose reports retain their original date semantics.
    use chrono::{FixedOffset, TimeZone};
    let local_start = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| AppError::Validation("invalid year/month".into()))?;
    let local_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }.ok_or_else(|| AppError::Validation("invalid year/month".into()))?;
    let offset = FixedOffset::east_opt(3 * 3600).unwrap();
    let utc_boundary = |day: NaiveDate| offset
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .single().unwrap().with_timezone(&chrono::Utc)
        .format("%Y-%m-%d %H:%M:%S").to_string();
    let start = utc_boundary(local_start);
    let end = (local_next.and_hms_opt(0,0,0).unwrap() - chrono::Duration::hours(3)
        - chrono::Duration::seconds(1)).format("%Y-%m-%d %H:%M:%S").to_string();
    let purchases = financial_service::calculate_profit(conn, Some(&start), Some(&end))?;
    let month_key = format!("{year:04}-{month:02}-01");
    let status: Option<String> = conn.query_row(
        "SELECT status FROM raw_material_month_closings WHERE period_month = ?1",
        [&month_key], |r| r.get(0)
    ).optional()?;
    let finalized = status.as_deref() == Some("closed");
    let (opening, closing, used, estimated) = if finalized {
        let (opening, bought, closing): (f64, f64, f64) = conn.query_row(
            "SELECT COALESCE(SUM(l.opening_value),0),COALESCE(SUM(l.purchased_value),0),COALESCE(SUM(l.closing_value),0)
             FROM raw_material_month_closing_lines l
             JOIN raw_material_month_closings c ON c.id=l.closing_id
             WHERE c.period_month=?1 AND c.status='closed'",
            [&month_key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        )?;
        let consumed = opening + bought - closing;
        let profit = purchases.sales - purchases.ready_made_costs - consumed - purchases.other_expenses;
        (Some(opening),Some(closing),Some(consumed),Some(profit))
    } else { (None,None,None,None) };
    Ok(MonthlyEstimatedProfit {
        status: if finalized { "finalized".into() } else { "pending_closing".into() },
        sales: purchases.sales, ready_made_costs: purchases.ready_made_costs,
        raw_material_purchases: purchases.raw_material_costs,
        opening_stock_value: opening, closing_stock_value: closing,
        raw_material_cost_used: used, other_expenses: purchases.other_expenses,
        estimated_net_profit: estimated,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductPerformance { pub item_id: i64, pub item_name: String, pub item_type: String, pub quantity_sold: i64, pub total_sales: f64, pub total_cost: f64 }

pub fn product_performance(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ProductPerformance>> {
    let base_sql = "SELECT si.item_id, si.item_name, si.item_type, SUM(si.quantity) AS quantity_sold, SUM(si.line_total) AS total_sales, SUM(si.line_cost) AS total_cost FROM sale_items si JOIN sales s ON s.id = si.sale_id {WHERE} GROUP BY si.item_id, si.item_name, si.item_type ORDER BY total_sales DESC";

    // s.is_reversed = 0 excludes voided sales (see sales_service::reverse_sale)
    // from product performance / sales mix, same as every other revenue figure.
    // Keep the prepared statement alive for the entire query_map/collect operation.
    // This explicit scope also avoids rusqlite borrow/lifetime errors on newer Rust.
    match (from, to) {
        (Some(f), Some(t)) => {
            let sql = base_sql.replace("{WHERE}", "WHERE s.is_reversed = 0 AND s.created_at BETWEEN ?1 AND ?2");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![f, t], map_product_performance)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        _ => {
            let sql = base_sql.replace("{WHERE}", "WHERE s.is_reversed = 0");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([], map_product_performance)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }
}

fn map_product_performance(row: &rusqlite::Row) -> rusqlite::Result<ProductPerformance> {
    Ok(ProductPerformance { item_id: row.get("item_id")?, item_name: row.get("item_name")?, item_type: row.get("item_type")?, quantity_sold: row.get("quantity_sold")?, total_sales: row.get("total_sales")?, total_cost: row.get("total_cost")? })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesMixEntry { pub item_name: String, pub quantity_sold: i64, pub total_sales: f64, pub percentage_of_sales: f64 }

pub fn sales_mix(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<SalesMixEntry>> {
    let performance = product_performance(conn, from, to)?;
    let total_sales: f64 = performance.iter().map(|p| p.total_sales).sum();
    Ok(performance.into_iter().map(|p| SalesMixEntry { item_name: p.item_name, quantity_sold: p.quantity_sold, total_sales: p.total_sales, percentage_of_sales: if total_sales > 0.0 { (p.total_sales / total_sales) * 100.0 } else { 0.0 } }).collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTrendEntry { pub date: String, pub revenue: f64, pub profit: f64 }

pub fn daily_profit_trend(conn: &Connection, days: i64) -> AppResult<Vec<DailyTrendEntry>> {
    // sales.created_at is stamped in UTC by SQLite's datetime('now'); the
    // day boundaries compared against it must be computed in UTC too.
    let today = chrono::Utc::now().date_naive();
    let start = today - chrono::Duration::days(days - 1);
    let mut out = Vec::with_capacity(days as usize);
    let mut day = start;
    while day <= today {
        let day_start = format!("{day} 00:00:00");
        let day_end = format!("{day} 23:59:59");
        let summary = financial_service::calculate_profit(conn, Some(&day_start), Some(&day_end))?;
        out.push(DailyTrendEntry { date: day.to_string(), revenue: summary.sales, profit: summary.profit });
        day += chrono::Duration::days(1);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyCostRevenueEntry { pub year: i32, pub month: u32, pub revenue: f64, pub cost: f64 }

pub fn monthly_cost_revenue_trend(conn: &Connection, months: u32) -> AppResult<Vec<MonthlyCostRevenueEntry>> {
    let today = chrono::Utc::now().date_naive();
    let (mut year, mut month) = (today.year(), today.month());
    let mut entries = Vec::with_capacity(months as usize);
    for _ in 0..months {
        let summary = monthly_report(conn, year, month)?;
        let cost = summary.ready_made_costs + summary.raw_material_costs + summary.other_expenses;
        entries.push(MonthlyCostRevenueEntry { year, month, revenue: summary.sales, cost });
        if month == 1 { month = 12; year -= 1; } else { month -= 1; }
    }
    entries.reverse();
    Ok(entries)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashFlowMonth { pub year: i32, pub month: u32, pub inflow: f64, pub outflow: f64, pub net: f64 }

pub fn cash_flow_by_year(conn: &Connection, year: i32) -> AppResult<Vec<CashFlowMonth>> {
    (1..=12u32).map(|month| {
        let summary = monthly_report(conn, year, month)?;
        let outflow = summary.ready_made_costs + summary.raw_material_costs + summary.other_expenses;
        Ok(CashFlowMonth { year, month, inflow: summary.sales, outflow, net: summary.profit })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use crate::models::item::ItemType;
    use crate::models::PaymentMethod;
    use crate::services::inventory_service::{self, NewItem};
    use crate::services::sales_service::{checkout, CartLine, CheckoutRequest};
    use crate::services::waiter_service;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn estimated_profit_only_after_month_closing() {
        use crate::services::{month_closing_service, raw_material_service};
        let mut conn = db();
        let meat = raw_material_service::create_raw_material(&conn, "Meat", "kg").unwrap();
        conn.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES(?1,20,300,6000,'2025-10-05 00:00:00')", [meat.id]).unwrap();
        conn.execute("INSERT INTO expenses(category,amount,expense_date) VALUES('Utilities',100,'2025-10-10 00:00:00')", []).unwrap();
        let pending = monthly_estimated_profit(&conn, 2025,10).unwrap();
        assert_eq!(pending.status,"pending_closing");
        assert!(pending.estimated_net_profit.is_none());
        month_closing_service::save_draft(&mut conn,"2025-10-01",&[(meat.id,5.0)]).unwrap();
        assert!(monthly_estimated_profit(&conn,2025,10).unwrap().estimated_net_profit.is_none());
        month_closing_service::close_month(&mut conn,"2025-10-01").unwrap();
        let finished = monthly_estimated_profit(&conn,2025,10).unwrap();
        assert_eq!(finished.status,"finalized");
        assert_eq!(finished.raw_material_cost_used,Some(4500.0));
        assert_eq!(finished.closing_stock_value,Some(1500.0));
        assert_eq!(finished.estimated_net_profit,Some(-4600.0));
        month_closing_service::reopen_month(&mut conn,"2025-10-01","Correct material count").unwrap();
        assert!(monthly_estimated_profit(&conn,2025,10).unwrap().estimated_net_profit.is_none());
    }

    #[test]
    fn monthly_estimate_uses_eat_expense_boundaries() {
        let c = db();
        c.execute("INSERT INTO expenses(category,amount,expense_date) VALUES('Rent',15,'2025-09-30 21:30:00')",[]).unwrap();
        c.execute("INSERT INTO expenses(category,amount,expense_date) VALUES('Rent',25,'2025-10-31 20:30:00')",[]).unwrap();
        c.execute("INSERT INTO expenses(category,amount,expense_date) VALUES('Rent',35,'2025-10-31 21:30:00')",[]).unwrap();
        let october = monthly_estimated_profit(&c,2025,10).unwrap();
        assert_eq!(october.other_expenses,40.0);
        let november = monthly_estimated_profit(&c,2025,11).unwrap();
        assert_eq!(november.other_expenses,35.0);
    }

    #[test]
    fn month_range_covers_full_calendar_month() {
        let (start, end) = month_range(2026, 2).unwrap();
        assert_eq!(start, "2026-02-01 00:00:00");
        assert_eq!(end, "2026-02-28 23:59:59");
    }

    #[test]
    fn month_range_handles_december_rollover() {
        let (start, end) = month_range(2026, 12).unwrap();
        assert_eq!(start, "2026-12-01 00:00:00");
        assert_eq!(end, "2026-12-31 23:59:59");
    }

    #[test]
    fn product_performance_and_sales_mix_add_up() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let cola = inventory_service::create_item(&conn, NewItem { category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade, purchase_cost: Some(0.5), selling_price: 2.0 }).unwrap();
        let chicken = inventory_service::create_item(&conn, NewItem { category_id: None, name: "Grilled Chicken".into(), item_type: ItemType::Cookable, purchase_cost: None, selling_price: 8.0 }).unwrap();
        checkout(&mut conn, CheckoutRequest { waiter_id: waiter.id, user_id: None, payment_method: PaymentMethod::Cash, lines: vec![CartLine { item_id: cola.id, quantity: 3 }, CartLine { item_id: chicken.id, quantity: 2 }], note: None }).unwrap();
        let performance = product_performance(&conn, None, None).unwrap();
        assert_eq!(performance.len(), 2);
        let chicken_perf = performance.iter().find(|p| p.item_name == "Grilled Chicken").unwrap();
        assert_eq!(chicken_perf.quantity_sold, 2);
        assert_eq!(chicken_perf.total_sales, 16.0);
        assert_eq!(chicken_perf.total_cost, 0.0);
        let mix = sales_mix(&conn, None, None).unwrap();
        let total_pct: f64 = mix.iter().map(|m| m.percentage_of_sales).sum();
        assert!((total_pct - 100.0).abs() < 0.001, "percentages should sum to ~100%");
    }
}
