//! Tauri commands for reporting: monthly summary, product performance,
//! sales mix, and yearly cash flow.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::services::financial_service::ProfitSummary;
use crate::services::report_service::{self, CashFlowMonth, ProductPerformance, SalesMixEntry};

/// Revenue/costs/expenses/profit for one calendar month.
#[tauri::command]
pub fn monthly_report(db: State<Db>, year: i32, month: u32) -> AppResult<ProfitSummary> {
    let conn = db.lock();
    report_service::monthly_report(&conn, year, month)
}

/// Per-item quantity sold / revenue / cost, optionally scoped to a date range.
#[tauri::command]
pub fn product_performance(
    db: State<Db>,
    from: Option<String>,
    to: Option<String>,
) -> AppResult<Vec<ProductPerformance>> {
    let conn = db.lock();
    report_service::product_performance(&conn, from.as_deref(), to.as_deref())
}

/// Each product's share of total revenue over the period.
#[tauri::command]
pub fn sales_mix(db: State<Db>, from: Option<String>, to: Option<String>) -> AppResult<Vec<SalesMixEntry>> {
    let conn = db.lock();
    report_service::sales_mix(&conn, from.as_deref(), to.as_deref())
}

/// Inflow/outflow/net for every month of the given year.
#[tauri::command]
pub fn cash_flow_by_year(db: State<Db>, year: i32) -> AppResult<Vec<CashFlowMonth>> {
    let conn = db.lock();
    report_service::cash_flow_by_year(&conn, year)
}
