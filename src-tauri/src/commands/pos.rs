//! Tauri commands for the point-of-sale: checkout and sales history.

use tauri::State;

use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission, BusinessAuthorization};
use crate::errors::AppResult;
use crate::models::Sale;
use crate::services::sales_service::{self, CheckoutRequest, CheckoutResult};

/// Ring up a sale. Atomic: if any cart line is invalid, nothing is
/// written — see `sales_service::checkout`.
#[tauri::command]
pub fn checkout(db: State<Db>, bridge: State<CloudBridge>, req: CheckoutRequest) -> AppResult<CheckoutResult> {
    let mut conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Pos)?;
    checkout_authorized(&mut conn, &_authorization, req)
}

/// A verified native guard is required even by this internal checkout adapter.
pub(crate) fn checkout_authorized(conn: &mut rusqlite::Connection,
    authorization: &BusinessAuthorization<'_>, mut req: CheckoutRequest) -> AppResult<CheckoutResult> {
    req.user_id = Some(authorization.user_id);
    sales_service::checkout(conn, req)
}

/// `from`/`to` bound the search to a `created_at` window (inclusive), so
/// the frontend can ask for a single day or an arbitrary date range; both
/// are optional and `limit` is too — see `sales_service::list_sales`.
#[tauri::command]
pub fn list_sales(
    db: State<Db>, bridge: State<CloudBridge>,
    waiter_id: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> AppResult<Vec<Sale>> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Pos)?;
    sales_service::list_sales(&conn, waiter_id, from.as_deref(), to.as_deref(), limit)
}

/// Void a sale, but only within `sales_service::REVERSAL_WINDOW_HOURS` of
/// when it was made. The sale row stays in history (for audit) but is
/// excluded from all revenue/cost/profit figures and waiter receivables.
#[tauri::command]
pub fn reverse_sale(db: State<Db>, bridge: State<CloudBridge>, sale_id: i64) -> AppResult<Sale> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    sales_service::reverse_sale(&conn, sale_id)
}
