//! Tauri commands for waiter management & receivables.
use tauri::State;
use crate::database::Db;
use crate::errors::AppResult;
use crate::models::Waiter;
use crate::services::waiter_service::{self, WaiterReceivable};

#[tauri::command]
pub fn create_waiter(db: State<Db>, full_name: String, phone: Option<String>, profile_photo: Option<String>) -> AppResult<Waiter> {
    let conn = db.lock();
    waiter_service::create_waiter(&conn, &full_name, phone.as_deref(), profile_photo.as_deref())
}

#[tauri::command]
pub fn list_waiters(db: State<Db>, only_active: bool) -> AppResult<Vec<Waiter>> {
    let conn = db.lock();
    waiter_service::list_waiters(&conn, only_active)
}

#[tauri::command]
pub fn set_waiter_active(db: State<Db>, waiter_id: i64, is_active: bool) -> AppResult<()> {
    let conn = db.lock();
    waiter_service::set_waiter_active(&conn, waiter_id, is_active)
}

#[tauri::command]
pub fn list_waiter_receivables(db: State<Db>) -> AppResult<Vec<WaiterReceivable>> {
    let conn = db.lock();
    waiter_service::list_receivables(&conn)
}

#[tauri::command]
pub fn settle_waiter(db: State<Db>, waiter_id: i64) -> AppResult<usize> {
    let conn = db.lock();
    waiter_service::settle_waiter(&conn, waiter_id)
}
