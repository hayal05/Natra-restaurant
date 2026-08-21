//! Tauri command for the dashboard page: a single aggregated summary.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::services::dashboard_service::{self, DashboardSummary};

#[tauri::command]
pub fn dashboard_summary(db: State<Db>) -> AppResult<DashboardSummary> {
    let conn = db.lock();
    dashboard_service::summary(&conn)
}
