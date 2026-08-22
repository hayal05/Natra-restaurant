//! Waiter management and receivables.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};
use crate::models::Waiter;

pub fn create_waiter(conn: &Connection, full_name: &str, phone: Option<&str>, profile_photo: Option<&str>) -> AppResult<Waiter> {
    if full_name.trim().is_empty() { return Err(AppError::Validation("waiter name is required".into())); }
    conn.execute("INSERT INTO waiters (full_name, phone, profile_photo) VALUES (?1, ?2, ?3)", rusqlite::params![full_name, phone, profile_photo])?;
    let id = conn.last_insert_rowid();
    Ok(conn.query_row("SELECT * FROM waiters WHERE id = ?1", [id], Waiter::from_row)?)
}

pub fn list_waiters(conn: &Connection, only_active: bool) -> AppResult<Vec<Waiter>> {
    let sql = if only_active { "SELECT * FROM waiters WHERE is_active = 1 ORDER BY full_name" } else { "SELECT * FROM waiters ORDER BY full_name" };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], Waiter::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_waiter_active(conn: &Connection, waiter_id: i64, is_active: bool) -> AppResult<()> {
    let affected = conn.execute("UPDATE waiters SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![is_active, waiter_id])?;
    if affected == 0 { return Err(AppError::NotFound(format!("waiter {waiter_id}"))); }
    Ok(())
}

pub fn get_receivable(conn: &Connection, waiter_id: i64) -> AppResult<f64> {
    let total: Option<f64> = conn.query_row("SELECT SUM(total_amount) FROM sales WHERE waiter_id = ?1 AND is_settled = 0", [waiter_id], |row| row.get(0))?;
    Ok(total.unwrap_or(0.0))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaiterReceivable { pub waiter: Waiter, pub receivable: f64 }

pub fn list_receivables(conn: &Connection) -> AppResult<Vec<WaiterReceivable>> {
    let waiters = list_waiters(conn, true)?;
    waiters.into_iter().map(|w| { let receivable = get_receivable(conn, w.id)?; Ok(WaiterReceivable { waiter: w, receivable }) }).collect()
}

pub fn settle_waiter(conn: &Connection, waiter_id: i64) -> AppResult<usize> {
    let affected = conn.execute("UPDATE sales SET is_settled = 1, settled_at = datetime('now') WHERE waiter_id = ?1 AND is_settled = 0", [waiter_id])?;
    Ok(affected)
}
