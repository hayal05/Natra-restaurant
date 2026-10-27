//! Tauri commands for expenses and the profit summary.

use tauri::State;

use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::models::Expense;
use crate::services::financial_service::{self, ProfitSummary};

#[tauri::command]
pub fn create_expense(
    db: State<Db>, bridge: State<CloudBridge>,
    category: String,
    description: Option<String>,
    amount: f64,
    expense_date: Option<String>,
    note: Option<String>,
) -> AppResult<Expense> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    financial_service::create_expense(
        &conn,
        &category,
        description.as_deref(),
        amount,
        expense_date.as_deref(),
        note.as_deref(),
    )
}

#[tauri::command]
pub fn list_expenses(db: State<Db>, bridge: State<CloudBridge>, from: Option<String>, to: Option<String>) -> AppResult<Vec<Expense>> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    financial_service::list_expenses(&conn, from.as_deref(), to.as_deref())
}

/// Full profit formula (sales - ready-made costs - raw-material costs
/// - other expenses), optionally scoped to a date range.
#[tauri::command]
pub fn calculate_profit(db: State<Db>, bridge: State<CloudBridge>, from: Option<String>, to: Option<String>) -> AppResult<ProfitSummary> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    financial_service::calculate_profit(&conn, from.as_deref(), to.as_deref())
}
