//! Tauri commands for reporting: monthly summary, product performance,
//! sales mix, and yearly cash flow.

use tauri::State;

use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::services::financial_service::ProfitSummary;
use crate::services::report_service::{self, CashFlowMonth, ProductPerformance, SalesMixEntry, MonthlyEstimatedProfit};

/// Revenue/costs/expenses/profit for one calendar month.
#[tauri::command]
pub fn monthly_report(db: State<Db>, bridge: State<CloudBridge>, year: i32, month: u32) -> AppResult<ProfitSummary> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    report_service::monthly_report(&conn, year, month)
}

#[tauri::command]
pub fn monthly_estimated_profit(db: State<Db>, bridge: State<CloudBridge>, year: i32, month: u32) -> AppResult<MonthlyEstimatedProfit> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    report_service::monthly_estimated_profit(&conn, year, month)
}

/// Per-item quantity sold / revenue / cost, optionally scoped to a date range.
#[tauri::command]
pub fn product_performance(
    db: State<Db>, bridge: State<CloudBridge>,
    from: Option<String>,
    to: Option<String>,
) -> AppResult<Vec<ProductPerformance>> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    report_service::product_performance(&conn, from.as_deref(), to.as_deref())
}

/// Each product's share of total revenue over the period.
#[tauri::command]
pub fn sales_mix(db: State<Db>, bridge: State<CloudBridge>, from: Option<String>, to: Option<String>) -> AppResult<Vec<SalesMixEntry>> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    report_service::sales_mix(&conn, from.as_deref(), to.as_deref())
}

/// Inflow/outflow/net for every month of the given year.
#[tauri::command]
pub fn cash_flow_by_year(db: State<Db>, bridge: State<CloudBridge>, year: i32) -> AppResult<Vec<CashFlowMonth>> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    report_service::cash_flow_by_year(&conn, year)
}
