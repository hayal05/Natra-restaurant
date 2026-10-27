//! Tauri command for the dashboard page: a single aggregated summary.

use tauri::State;

use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::services::dashboard_service::{self, DashboardSummary};

#[tauri::command]
pub fn dashboard_summary(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<DashboardSummary> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    dashboard_service::summary(&conn)
}
