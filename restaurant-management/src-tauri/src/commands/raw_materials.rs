//! Tauri commands for raw materials & purchase (cost) recording.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::{RawMaterial, RawMaterialPurchase};
use crate::services::raw_material_service;

#[tauri::command]
pub fn create_raw_material(db: State<Db>, name: String, unit: String) -> AppResult<RawMaterial> {
    let conn = db.lock();
    raw_material_service::create_raw_material(&conn, &name, &unit)
}

#[tauri::command]
pub fn list_raw_materials(db: State<Db>, only_active: bool) -> AppResult<Vec<RawMaterial>> {
    let conn = db.lock();
    raw_material_service::list_raw_materials(&conn, only_active)
}

/// Record a purchase. `total_cost` is computed server-side from
/// quantity * unit_cost — never trusted from the frontend.
#[tauri::command]
pub fn record_purchase(
    db: State<Db>,
    raw_material_id: i64,
    quantity: f64,
    unit_cost: f64,
    supplier: Option<String>,
    note: Option<String>,
) -> AppResult<RawMaterialPurchase> {
    let conn = db.lock();
    raw_material_service::record_purchase(
        &conn,
        raw_material_id,
        quantity,
        unit_cost,
        supplier.as_deref(),
        note.as_deref(),
    )
}

#[tauri::command]
pub fn list_purchases(
    db: State<Db>,
    raw_material_id: Option<i64>,
) -> AppResult<Vec<RawMaterialPurchase>> {
    let conn = db.lock();
    raw_material_service::list_purchases(&conn, raw_material_id)
}

/// Total raw-material cost, optionally scoped to a date range
/// (inclusive, 'YYYY-MM-DD' or full datetime strings).
#[tauri::command]
pub fn raw_material_total_cost(
    db: State<Db>,
    from: Option<String>,
    to: Option<String>,
) -> AppResult<f64> {
    let conn = db.lock();
    raw_material_service::total_cost(&conn, from.as_deref(), to.as_deref())
}
