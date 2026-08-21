//! Tauri commands for the point-of-sale: checkout and sales history.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::Sale;
use crate::services::sales_service::{self, CheckoutRequest, CheckoutResult};

/// Ring up a sale. Atomic: if any cart line is invalid, nothing is
/// written — see `sales_service::checkout`.
#[tauri::command]
pub fn checkout(db: State<Db>, req: CheckoutRequest) -> AppResult<CheckoutResult> {
    let mut conn = db.lock();
    sales_service::checkout(&mut conn, req)
}

#[tauri::command]
pub fn list_sales(db: State<Db>, waiter_id: Option<i64>, limit: i64) -> AppResult<Vec<Sale>> {
    let conn = db.lock();
    sales_service::list_sales(&conn, waiter_id, limit)
}
