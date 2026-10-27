use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::State;
use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::services::month_closing_service::{self, ClosingState, MaterialBalance};

#[derive(Serialize)]
pub struct ClosingView {
    pub month: String,
    pub status: String,
    pub materials: Vec<MaterialBalance>,
    pub remaining: Vec<(i64, Option<f64>)>,
}

#[tauri::command]
pub fn get_material_month_closing(db: State<Db>, bridge: State<CloudBridge>, month: String) -> AppResult<ClosingView> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    let materials = month_closing_service::preview(&conn, &month)?;
    let record: Option<(i64,String)> = conn.query_row(
        "SELECT id,status FROM raw_material_month_closings WHERE period_month=?1",
        [&month], |r| Ok((r.get(0)?,r.get(1)?))
    ).optional()?;
    let mut remaining = Vec::new();
    if let Some((id,_)) = record.as_ref() {
        let mut stmt = conn.prepare("SELECT raw_material_id,remaining_quantity FROM raw_material_month_closing_lines WHERE closing_id=?1")?;
        remaining = stmt.query_map([id], |r| Ok((r.get(0)?,r.get(1)?)))?
            .collect::<Result<Vec<_>,_>>()?;
    }
    Ok(ClosingView {
        month,
        status: record.map(|(_,status)|status).unwrap_or_else(||"new".into()),
        materials, remaining,
    })
}

#[tauri::command]
pub fn save_material_month_draft(db: State<Db>, bridge: State<CloudBridge>, month: String, remaining: Vec<(i64,f64)>) -> AppResult<ClosingState> {
    let mut conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    month_closing_service::save_draft(&mut conn, &month, &remaining)
}

#[tauri::command]
pub fn close_material_month(db: State<Db>, bridge: State<CloudBridge>, month: String) -> AppResult<ClosingState> {
    let mut conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    month_closing_service::close_month(&mut conn, &month)
}

#[tauri::command]
pub fn reopen_material_month(db: State<Db>, bridge: State<CloudBridge>, month: String, reason: String) -> AppResult<()> {
    let mut conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    month_closing_service::reopen_month(&mut conn, &month, &reason)
}
