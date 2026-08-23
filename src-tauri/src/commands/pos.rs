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

/// `from`/`to` bound the search to a `created_at` window (inclusive), so
/// the frontend can ask for a single day or an arbitrary date range; both
/// are optional and `limit` is too — see `sales_service::list_sales`.
#[tauri::command]
pub fn list_sales(
    db: State<Db>,
    waiter_id: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> AppResult<Vec<Sale>> {
    let conn = db.lock();
    sales_service::list_sales(&conn, waiter_id, from.as_deref(), to.as_deref(), limit)
}

/// Void a sale, but only within `sales_service::REVERSAL_WINDOW_HOURS` of
/// when it was made. The sale row stays in history (for audit) but is
/// excluded from all revenue/cost/profit figures and waiter receivables.
#[tauri::command]
pub fn reverse_sale(db: State<Db>, sale_id: i64) -> AppResult<Sale> {
    let conn = db.lock();
    sales_service::reverse_sale(&conn, sale_id)
}
