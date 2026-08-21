//! Dashboard aggregation.
//!
//! The dashboard page needs several figures at once (today's numbers,
//! this month's numbers, who owes what, what's selling). Rather than
//! have the frontend fire off half a dozen separate commands, this
//! service composes the existing `financial_service`, `report_service`,
//! and `waiter_service` building blocks into one payload.

use chrono::{Datelike, Local, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::services::financial_service::{self, ProfitSummary};
use crate::services::report_service::{self, DailyTrendEntry, MonthlyCostRevenueEntry, ProductPerformance, SalesMixEntry};
use crate::services::waiter_service::{self, WaiterReceivable};

/// Inclusive ['YYYY-MM-DD 00:00:00', 'YYYY-MM-DD 23:59:59'] range for today,
/// in the local system timezone.
fn today_range() -> (String, String) {
    let today = Local::now().date_naive();
    (format!("{today} 00:00:00"), format!("{today} 23:59:59"))
}

/// Inclusive range for the current calendar month.
fn this_month_range() -> AppResult<(String, String)> {
    let now = Local::now().date_naive();
    let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .ok_or_else(|| AppError::Internal("failed to compute month start".into()))?;
    Ok((format!("{start} 00:00:00"), format!("{now} 23:59:59")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub today: ProfitSummary,
    pub this_month: ProfitSummary,
    pub waiter_receivables: Vec<WaiterReceivable>,
    pub total_receivable: f64,
    pub top_products_today: Vec<ProductPerformance>,
    pub sales_mix_this_month: Vec<SalesMixEntry>,
    /// Day-by-day revenue vs. profit for the trailing 14 days.
    pub revenue_profit_trend: Vec<DailyTrendEntry>,
    /// Revenue vs. total cost for the trailing 6 calendar months.
    pub cost_revenue_by_month: Vec<MonthlyCostRevenueEntry>,
}

/// How many trailing days the dashboard's revenue/profit trend covers.
const TREND_DAYS: i64 = 14;
/// How many trailing months the dashboard's cost-vs-revenue chart covers.
const COST_REVENUE_MONTHS: u32 = 6;

/// Everything the dashboard page renders in one call.
pub fn summary(conn: &Connection) -> AppResult<DashboardSummary> {
    let (today_start, today_end) = today_range();
    let (month_start, month_end) = this_month_range()?;

    let today = financial_service::calculate_profit(conn, Some(&today_start), Some(&today_end))?;
    let this_month = financial_service::calculate_profit(conn, Some(&month_start), Some(&month_end))?;

    let waiter_receivables = waiter_service::list_receivables(conn)?;
    let total_receivable = waiter_receivables.iter().map(|r| r.receivable).sum();

    let mut top_products_today =
        report_service::product_performance(conn, Some(&today_start), Some(&today_end))?;
    top_products_today.truncate(5);

    let mut sales_mix_this_month =
        report_service::sales_mix(conn, Some(&month_start), Some(&month_end))?;
    sales_mix_this_month.truncate(5);

    let revenue_profit_trend = report_service::daily_profit_trend(conn, TREND_DAYS)?;
    let cost_revenue_by_month = report_service::monthly_cost_revenue_trend(conn, COST_REVENUE_MONTHS)?;

    Ok(DashboardSummary {
        today,
        this_month,
        waiter_receivables,
        total_receivable,
        top_products_today,
        sales_mix_this_month,
        revenue_profit_trend,
        cost_revenue_by_month,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use crate::models::item::ItemType;
    use crate::models::PaymentMethod;
    use crate::services::inventory_service::{self, NewItem};
    use crate::services::sales_service::{checkout, CartLine, CheckoutRequest};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn summary_reflects_a_sale_made_today() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None).unwrap();
        let cola = inventory_service::create_item(
            &conn,
            NewItem {
                category_id: None,
                name: "Cola".into(),
                item_type: ItemType::ReadyMade,
                purchase_cost: Some(0.5),
                selling_price: 2.0,
            },
        )
        .unwrap();

        checkout(
            &mut conn,
            CheckoutRequest {
                waiter_id: waiter.id,
                user_id: None,
                payment_method: PaymentMethod::Cash,
                lines: vec![CartLine { item_id: cola.id, quantity: 4 }],
                note: None,
            },
        )
        .unwrap();

        let summary = summary(&conn).unwrap();
        assert_eq!(summary.today.sales, 8.0);
        assert_eq!(summary.this_month.sales, 8.0);
        assert_eq!(summary.total_receivable, 8.0);
        assert_eq!(summary.waiter_receivables.len(), 1);
        assert_eq!(summary.top_products_today.len(), 1);
        assert_eq!(summary.sales_mix_this_month.len(), 1);
    }
}
