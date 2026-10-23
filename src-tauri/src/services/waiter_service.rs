//! Waiter management and receivables.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};
use crate::models::{Sale, SaleItem, Waiter};

// Share a complete waiter snapshot across create and activation updates.
fn enqueue_waiter_snapshot(tx: &rusqlite::Transaction<'_>, waiter_id: i64) -> AppResult<Waiter> {
    let waiter = tx.query_row("SELECT * FROM waiters WHERE id=?1", [waiter_id], Waiter::from_row)?;
    let installation_id = crate::database::connection::ensure_installation_id(tx)?;
    let mapped: Option<String> = tx.query_row(
        "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='waiters' AND local_id=?1",
        [waiter_id], |row| row.get(0),
    ).optional()?;
    let entity_id = match mapped {
        Some(id) => id,
        None => {
            let id = format!("{installation_id}:{waiter_id}");
            crate::database::entity_ids::register(tx, "waiters", &id, waiter_id)?;
            id
        }
    };
    crate::sync::queue::enqueue_cloud_event(
        tx, "waiters", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": waiter_id,
            "waiter": waiter,
        }),
    )?;
    Ok(waiter)
}

pub fn create_waiter(conn: &Connection, full_name: &str, phone: Option<&str>, profile_photo: Option<&str>) -> AppResult<Waiter> {
    if full_name.trim().is_empty() { return Err(AppError::Validation("waiter name is required".into())); }
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT INTO waiters (full_name, phone, profile_photo) VALUES (?1, ?2, ?3)", rusqlite::params![full_name, phone, profile_photo])?;
    let id = tx.last_insert_rowid();
    let waiter = enqueue_waiter_snapshot(&tx, id)?;
    tx.commit()?;
    Ok(waiter)
}

pub fn list_waiters(conn: &Connection, only_active: bool) -> AppResult<Vec<Waiter>> {
    let sql = if only_active { "SELECT * FROM waiters WHERE is_active = 1 ORDER BY full_name" } else { "SELECT * FROM waiters ORDER BY full_name" };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], Waiter::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_waiter_active(conn: &Connection, waiter_id: i64, is_active: bool) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute("UPDATE waiters SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2", rusqlite::params![is_active, waiter_id])?;
    if affected == 0 { return Err(AppError::NotFound(format!("waiter {waiter_id}"))); }
    enqueue_waiter_snapshot(&tx, waiter_id)?;
    tx.commit()?;
    Ok(())
}

pub fn get_receivable(conn: &Connection, waiter_id: i64) -> AppResult<f64> {
    // Reversed sales never counted as owed by the waiter in the first place.
    let total: Option<f64> = conn.query_row("SELECT SUM(total_amount) FROM sales WHERE waiter_id = ?1 AND is_settled = 0 AND is_reversed = 0", [waiter_id], |row| row.get(0))?;
    Ok(total.unwrap_or(0.0))
}

/// The individual sales that make up a waiter's current receivable — same
/// `is_settled = 0 AND is_reversed = 0` filter as `get_receivable`, just
/// returning the rows instead of the sum, so the UI can show *which*
/// transactions the outstanding amount comes from.
pub fn list_receivable_sales(conn: &Connection, waiter_id: i64) -> AppResult<Vec<Sale>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM sales WHERE waiter_id = ?1 AND is_settled = 0 AND is_reversed = 0 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([waiter_id], Sale::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The individual line items behind a waiter's current receivable — same
/// `is_settled = 0 AND is_reversed = 0` filter as `get_receivable`, joined
/// down to item level so the UI can show what was actually sold, its unit
/// price, and when, instead of just a per-sale total.
pub fn list_receivable_sale_items(conn: &Connection, waiter_id: i64) -> AppResult<Vec<SaleItem>> {
    let mut stmt = conn.prepare(
        "SELECT sale_items.* FROM sale_items \
         JOIN sales ON sales.id = sale_items.sale_id \
         WHERE sales.waiter_id = ?1 AND sales.is_settled = 0 AND sales.is_reversed = 0 \
         ORDER BY sales.created_at DESC, sale_items.id ASC",
    )?;
    let rows = stmt.query_map([waiter_id], SaleItem::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaiterReceivable { pub waiter: Waiter, pub receivable: f64 }

pub fn list_receivables(conn: &Connection) -> AppResult<Vec<WaiterReceivable>> {
    let waiters = list_waiters(conn, true)?;
    waiters.into_iter().map(|w| { let receivable = get_receivable(conn, w.id)?; Ok(WaiterReceivable { waiter: w, receivable }) }).collect()
}

pub fn settle_waiter(conn: &Connection, waiter_id: i64) -> AppResult<usize> {
    let tx = conn.unchecked_transaction()?;
    // Capture the exact affected sales before updating; retries cannot
    // create a second settlement for already-settled sales.
    let sale_ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM sales WHERE waiter_id = ?1 AND is_settled = 0 ORDER BY id"
        )?;
        let rows = stmt.query_map([waiter_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if sale_ids.is_empty() {
        tx.commit()?;
        return Ok(0);
    }
    let affected = tx.execute(
        "UPDATE sales SET is_settled = 1, settled_at = datetime('now')
         WHERE waiter_id = ?1 AND is_settled = 0", [waiter_id]
    )?;
    let installation_id = crate::database::connection::ensure_installation_id(&tx)?;
    let mut settled_sales = Vec::with_capacity(sale_ids.len());
    for id in sale_ids {
        settled_sales.push(tx.query_row(
            "SELECT * FROM sales WHERE id = ?1", [id], Sale::from_row
        )?);
    }
    crate::sync::queue::enqueue_cloud_event(
        &tx, "waiter_settlements", &format!("{installation_id}:{waiter_id}"), "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "waiter_entity_id": format!("{installation_id}:{waiter_id}"),
            "settled_sales": settled_sales,
        }),
    )?;
    tx.commit()?;
    Ok(affected)
}

#[cfg(test)]
mod settlement_outbox_tests {
    use super::*;
    use crate::database::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn pending_sale(conn: &Connection, waiter_id: i64) {
        conn.execute(
            "INSERT INTO sales (waiter_id, total_quantity, total_amount, total_cost, total_profit, payment_method)
             VALUES (?1, 1, 20, 0, 20, 'cash')", [waiter_id]
        ).unwrap();
    }

    #[test]
    fn settlement_queues_exact_sales_once() {
        let conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        pending_sale(&conn, waiter.id);
        pending_sale(&conn, waiter.id);
        assert_eq!(settle_waiter(&conn, waiter.id).unwrap(), 2);
        assert_eq!(settle_waiter(&conn, waiter.id).unwrap(), 0);
        let (count, payload): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(payload) FROM cloud_outbox WHERE entity_type='waiter_settlements'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(count, 1);
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["settled_sales"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn settlement_rolls_back_on_outbox_failure() {
        let conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        pending_sale(&conn, waiter.id);
        conn.execute_batch(
            "CREATE TRIGGER stop_settlement BEFORE INSERT ON cloud_outbox
             WHEN NEW.entity_type='waiter_settlements'
             BEGIN SELECT RAISE(FAIL, 'outbox failure'); END;"
        ).unwrap();
        assert!(settle_waiter(&conn, waiter.id).is_err());
        let unsettled: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sales WHERE waiter_id=?1 AND is_settled=0",
            [waiter.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(unsettled, 1);
    }
}

#[cfg(test)]
mod sync_outbox_tests {
    use super::*;
    use crate::database::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn waiter_creation_and_activation_enqueue_snapshots() {
        let conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        set_waiter_active(&conn, waiter.id, false).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM cloud_outbox WHERE entity_type='waiters'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
        let last: String = conn.query_row("SELECT payload FROM cloud_outbox WHERE entity_type='waiters' ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&last).unwrap();
        assert_eq!(json["waiter"]["is_active"], false);
    }

    #[test]
    fn waiter_identity_persists_across_activation() {
        let conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        let original: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='waiters' AND local_id=?1",
            [waiter.id], |r| r.get(0),
        ).unwrap();
        conn.execute(
            "UPDATE cloud_entity_ids SET entity_id='remote-device:55'
             WHERE entity_type='waiters' AND local_id=?1", [waiter.id],
        ).unwrap();
        set_waiter_active(&conn, waiter.id, false).unwrap();
        let latest: String = conn.query_row(
            "SELECT entity_id FROM cloud_outbox WHERE entity_type='waiters' ORDER BY rowid DESC LIMIT 1",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(latest, "remote-device:55");
        assert_ne!(latest, original);
    }

    #[test]
    fn waiter_changes_rollback_on_outbox_failure() {
        let conn = db();
        conn.execute_batch("CREATE TRIGGER fail_waiter_event BEFORE INSERT ON cloud_outbox BEGIN SELECT RAISE(FAIL, 'outbox failure'); END;").unwrap();
        assert!(create_waiter(&conn, "Unwritten", None, None).is_err());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM waiters WHERE full_name='Unwritten'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
