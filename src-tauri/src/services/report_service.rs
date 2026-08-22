//! Reporting: monthly revenue/expenses/costs, product performance,
//! sales mix, and cash flow.

use chrono::{Datelike, NaiveDate};
use rusqlite::Connection;
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
pub struct ProductPerformance { pub item_id: i64, pub item_name: String, pub item_type: String, pub quantity_sold: i64, pub total_sales: f64, pub total_cost: f64 }

pub fn product_performance(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ProductPerformance>> {
    let base_sql = "SELECT si.item_id, si.item_name, si.item_type, SUM(si.quantity) AS quantity_sold, SUM(si.line_total) AS total_sales, SUM(si.line_cost) AS total_cost FROM sale_items si JOIN sales s ON s.id = si.sale_id {WHERE} GROUP BY si.item_id, si.item_name, si.item_type ORDER BY total_sales DESC";

    // Keep the prepared statement alive for the entire query_map/collect operation.
    // This explicit scope also avoids rusqlite borrow/lifetime errors on newer Rust.
    match (from, to) {
        (Some(f), Some(t)) => {
            let sql = base_sql.replace("{WHERE}", "WHERE s.created_at BETWEEN ?1 AND ?2");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![f, t], map_product_performance)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        _ => {
            let sql = base_sql.replace("{WHERE}", "");
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
    let today = chrono::Local::now().date_naive();
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
    let today = chrono::Local::now().date_naive();
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
