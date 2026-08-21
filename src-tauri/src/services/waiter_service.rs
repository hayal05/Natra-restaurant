//! Waiter management and receivables.
//!
//! A waiter's receivable is the sum of `sales.total_amount` for every
//! sale they rang up that hasn't been settled yet — money they've
//! collected on behalf of the business but not yet handed over/reconciled.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::Waiter;

pub fn create_waiter(conn: &Connection, full_name: &str, phone: Option<&str>) -> AppResult<Waiter> {
    if full_name.trim().is_empty() {
        return Err(AppError::Validation("waiter name is required".into()));
    }
    conn.execute(
        "INSERT INTO waiters (full_name, phone) VALUES (?1, ?2)",
        rusqlite::params![full_name, phone],
    )?;
    let id = conn.last_insert_rowid();
    Ok(conn.query_row("SELECT * FROM waiters WHERE id = ?1", [id], Waiter::from_row)?)
}

pub fn list_waiters(conn: &Connection, only_active: bool) -> AppResult<Vec<Waiter>> {
    let sql = if only_active {
        "SELECT * FROM waiters WHERE is_active = 1 ORDER BY full_name"
    } else {
        "SELECT * FROM waiters ORDER BY full_name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], Waiter::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_waiter_active(conn: &Connection, waiter_id: i64, is_active: bool) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE waiters SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![is_active, waiter_id],
    )?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("waiter {waiter_id}")));
    }
    Ok(())
}

/// Outstanding receivable for a single waiter: sum of unsettled sales.
pub fn get_receivable(conn: &Connection, waiter_id: i64) -> AppResult<f64> {
    let total: Option<f64> = conn.query_row(
        "SELECT SUM(total_amount) FROM sales WHERE waiter_id = ?1 AND is_settled = 0",
        [waiter_id],
        |row| row.get(0),
    )?;
    Ok(total.unwrap_or(0.0))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaiterReceivable {
    pub waiter: Waiter,
    pub receivable: f64,
}

/// Receivables for every active waiter — the shape the dashboard wants.
pub fn list_receivables(conn: &Connection) -> AppResult<Vec<WaiterReceivable>> {
    let waiters = list_waiters(conn, true)?;
    waiters
        .into_iter()
        .map(|w| {
            let receivable = get_receivable(conn, w.id)?;
            Ok(WaiterReceivable { waiter: w, receivable })
        })
        .collect()
}

/// Mark every unsettled sale for a waiter as settled (reconciled).
/// Returns how many sales were settled.
pub fn settle_waiter(conn: &Connection, waiter_id: i64) -> AppResult<usize> {
    let affected = conn.execute(
        "UPDATE sales SET is_settled = 1, settled_at = datetime('now')
         WHERE waiter_id = ?1 AND is_settled = 0",
        [waiter_id],
    )?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn receivable_reflects_only_unsettled_sales() {
        let conn = db();
        let waiter = create_waiter(&conn, "Alex", None).unwrap();

        conn.execute(
            "INSERT INTO sales (waiter_id, total_quantity, total_amount, total_cost, total_profit)
             VALUES (?1, 1, 10.0, 0, 10.0)",
            [waiter.id],
        ).unwrap();
        conn.execute(
            "INSERT INTO sales (waiter_id, total_quantity, total_amount, total_cost, total_profit, is_settled)
             VALUES (?1, 1, 25.0, 0, 25.0, 1)",
            [waiter.id],
        ).unwrap();

        // Only the unsettled 10.0 sale should count.
        assert_eq!(get_receivable(&conn, waiter.id).unwrap(), 10.0);
    }

    #[test]
    fn settling_zeroes_out_receivable() {
        let conn = db();
        let waiter = create_waiter(&conn, "Alex", None).unwrap();
        conn.execute(
            "INSERT INTO sales (waiter_id, total_quantity, total_amount, total_cost, total_profit)
             VALUES (?1, 1, 10.0, 0, 10.0)",
            [waiter.id],
        ).unwrap();

        assert_eq!(get_receivable(&conn, waiter.id).unwrap(), 10.0);
        let settled_count = settle_waiter(&conn, waiter.id).unwrap();
        assert_eq!(settled_count, 1);
        assert_eq!(get_receivable(&conn, waiter.id).unwrap(), 0.0);
    }
}
