use serde::Serialize;
use tauri::State;
use crate::database::Db;
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
pub fn get_material_month_closing(db: State<Db>, month: String) -> AppResult<ClosingView> {
    use rusqlite::OptionalExtension;
    let conn = db.lock();
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
pub fn save_material_month_draft(db: State<Db>, month: String, remaining: Vec<(i64,f64)>) -> AppResult<ClosingState> {
    month_closing_service::save_draft(&mut db.lock(), &month, &remaining)
}

#[tauri::command]
pub fn close_material_month(db: State<Db>, month: String) -> AppResult<ClosingState> {
    month_closing_service::close_month(&mut db.lock(), &month)
}

#[tauri::command]
pub fn reopen_material_month(db: State<Db>, month: String, reason: String) -> AppResult<()> {
    month_closing_service::reopen_month(&mut db.lock(), &month, &reason)
}
