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

/// Import a waiter from a remote installation. Never reuse the sender's
/// local row ID, and never overwrite an existing mapped waiter here.
#[allow(dead_code)]
pub(crate) fn apply_remote_waiter(
    tx: &rusqlite::Transaction<'_>, entity_id: &str, payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote waiter".into());
    let waiter: Waiter = serde_json::from_value(
        payload.get("waiter").cloned().ok_or_else(invalid)?
    ).map_err(|_| invalid())?;
    if entity_id.trim().is_empty() || waiter.full_name.trim().is_empty() {
        return Err(invalid());
    }
    if let Some(local_id) = crate::database::entity_ids::lookup(tx, "waiters", entity_id)? {
        // Keep the local waiter ID stable for sale and settlement references.
        let updated = tx.execute(
            "UPDATE waiters SET full_name=?1,phone=?2,profile_photo=?3,
             is_active=?4,updated_at=?5 WHERE id=?6",
            rusqlite::params![waiter.full_name,waiter.phone,waiter.profile_photo,
                waiter.is_active,waiter.updated_at,local_id],
        )?;
        if updated != 1 { return Err(invalid()); }
    } else {
        tx.execute(
            "INSERT INTO waiters(full_name,phone,profile_photo,is_active,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![waiter.full_name,waiter.phone,waiter.profile_photo,
                waiter.is_active,waiter.created_at,waiter.updated_at],
        )?;
        crate::database::entity_ids::register(tx, "waiters", entity_id, tx.last_insert_rowid())?;
    }
    Ok(())
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
    let waiter_cloud_id: Option<String> = tx.query_row(
        "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='waiters' AND local_id=?1",
        [waiter_id], |r| r.get(0),
    ).optional()?;
    let waiter_cloud_id = waiter_cloud_id.unwrap_or_else(|| format!("{installation_id}:{waiter_id}"));
    crate::database::entity_ids::register(&tx, "waiters", &waiter_cloud_id, waiter_id)?;
    let mut settled_sales = Vec::with_capacity(sale_ids.len());
    let mut sale_entity_ids = Vec::with_capacity(sale_ids.len());
    for id in sale_ids {
        let sale_cloud_id: Option<String> = tx.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='sales' AND local_id=?1",
            [id], |r| r.get(0),
        ).optional()?;
        let sale_cloud_id = sale_cloud_id.unwrap_or_else(|| format!("{installation_id}:{id}"));
        crate::database::entity_ids::register(&tx, "sales", &sale_cloud_id, id)?;
        sale_entity_ids.push(sale_cloud_id);
        settled_sales.push(tx.query_row(
            "SELECT * FROM sales WHERE id = ?1", [id], Sale::from_row
        )?);
    }
    crate::sync::queue::enqueue_cloud_event(
        &tx, "waiter_settlements", &waiter_cloud_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "waiter_entity_id": waiter_cloud_id,
            "sale_entity_ids": sale_entity_ids,
            "settled_sales": settled_sales,
        }),
    )?;
    tx.commit()?;
    Ok(affected)
}

/// Replay a downloaded settlement in the same transaction as its cloud receipt.
/// Local sale IDs are never accepted from the sender; each is resolved by cloud ID.
#[allow(dead_code)]
pub(crate) fn apply_remote_waiter_settlement(
    tx: &rusqlite::Transaction<'_>,
    payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote waiter settlement".into());
    let waiter_cloud_id = payload.get("waiter_entity_id").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty()).ok_or_else(invalid)?;
    let waiter_id = crate::database::entity_ids::lookup(tx, "waiters", waiter_cloud_id)?
        .ok_or_else(|| AppError::Validation("unknown waiter identity in settlement".into()))?;
    let ids = payload.get("sale_entity_ids").and_then(|v| v.as_array()).ok_or_else(invalid)?;
    let snapshots: Vec<Sale> = serde_json::from_value(
        payload.get("settled_sales").cloned().ok_or_else(invalid)?
    ).map_err(|_| invalid())?;
    if ids.is_empty() || ids.len() != snapshots.len() { return Err(invalid()); }
    let mut seen = std::collections::HashSet::new();
    let mut resolved = Vec::with_capacity(ids.len());
    for (cloud_id, snapshot) in ids.iter().zip(&snapshots) {
        let cloud_id = cloud_id.as_str().filter(|v| !v.trim().is_empty()).ok_or_else(invalid)?;
        if !seen.insert(cloud_id) || !snapshot.is_settled
            || snapshot.settled_at.as_deref().unwrap_or("").is_empty() {
            return Err(invalid());
        }
        let local_id = crate::database::entity_ids::lookup(tx, "sales", cloud_id)?
            .ok_or_else(|| AppError::Validation("unknown sale identity in settlement".into()))?;
        let current = tx.query_row("SELECT * FROM sales WHERE id=?1", [local_id], Sale::from_row)?;
        if current.waiter_id != waiter_id || current.is_settled
            || current.total_quantity != snapshot.total_quantity
            || !snapshot.total_amount.is_finite() || !snapshot.total_cost.is_finite()
            || (current.total_amount - snapshot.total_amount).abs() > 0.01
            || (current.total_cost - snapshot.total_cost).abs() > 0.01 {
            return Err(invalid());
        }
        resolved.push((local_id, snapshot.settled_at.clone().ok_or_else(invalid)?));
    }
    for (id, settled_at) in resolved {
        let changed = tx.execute(
            "UPDATE sales SET is_settled=1, settled_at=?1 WHERE id=?2 AND is_settled=0",
            rusqlite::params![settled_at,id],
        )?;
        if changed != 1 { return Err(invalid()); }
    }
    Ok(())
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
    fn remote_waiter_update_retains_identity_and_changes_active_status() {
        let mut conn = db();
        let waiter = create_waiter(&conn, "Old Name", None, None).unwrap();
        let cloud_id: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='waiters' AND local_id=?1",
            [waiter.id], |r| r.get(0),
        ).unwrap();
        let mut updated = serde_json::to_value(waiter.clone()).unwrap();
        updated["full_name"] = serde_json::json!("New Name");
        updated["is_active"] = serde_json::json!(false);
        let payload = serde_json::json!({"waiter":updated});
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("updated-waiter",1)],
            |tx,_,_|apply_remote_waiter(tx,&cloud_id,&payload)
        ).unwrap();
        let (name,active):(String,bool) = conn.query_row(
            "SELECT full_name,is_active FROM waiters WHERE id=?1",
            [waiter.id],|r|Ok((r.get(0)?,r.get(1)?)),
        ).unwrap();
        assert_eq!(name,"New Name");
        assert!(!active);
        assert_eq!(crate::database::entity_ids::lookup(&conn,"waiters",&cloud_id).unwrap(),Some(waiter.id));
    }

    #[test]
    fn remote_waiter_import_uses_local_identity_and_replays_updates() {
        let mut conn = db();
        let payload = serde_json::json!({"waiter":{
            "id":999,"full_name":"Remote Waiter","phone":null,"profile_photo":null,
            "is_active":true,"created_at":"2025-01-01 00:00:00",
            "updated_at":"2025-01-01 00:00:00"
        }});
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-waiter-1",1)],
            |tx,_,_|apply_remote_waiter(tx,"remote:999",&payload)
        ).unwrap();
        let local = crate::database::entity_ids::lookup(&conn,"waiters","remote:999")
            .unwrap().unwrap();
        assert_ne!(local,999);
        let name: String = conn.query_row(
            "SELECT full_name FROM waiters WHERE id=?1",[local],|r|r.get(0),
        ).unwrap();
        assert_eq!(name,"Remote Waiter");
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-waiter-2",2)],
            |tx,_,_|apply_remote_waiter(tx,"remote:999",&payload)
        ).unwrap();
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),2);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM waiters",[],|r|r.get(0)).unwrap();
        assert_eq!(count,1);
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
    fn remote_settlement_is_atomic_and_rejects_duplicate_effects() {
        let mut conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        pending_sale(&conn, waiter.id);
        let sale_id = conn.last_insert_rowid();
        let waiter_cloud: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='waiters' AND local_id=?1",
            [waiter.id], |r| r.get(0),
        ).unwrap();
        crate::database::entity_ids::register(&conn,"sales","remote:sale",sale_id).unwrap();
        let mut sale = conn.query_row("SELECT * FROM sales WHERE id=?1",[sale_id],Sale::from_row).unwrap();
        sale.is_settled=true;
        sale.settled_at=Some("2026-02-01 10:00:00".into());
        let payload=serde_json::json!({
            "waiter_entity_id":waiter_cloud,
            "sale_entity_ids":["remote:sale"],
            "settled_sales":[sale]
        });
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-settlement",1)],
            |tx,_,_|apply_remote_waiter_settlement(tx,&payload),
        ).unwrap();
        assert_eq!(conn.query_row("SELECT is_settled FROM sales WHERE id=?1",
            [sale_id],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("second-settlement",2)],
            |tx,_,_|apply_remote_waiter_settlement(tx,&payload),
        ).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),1);
    }

    #[test]
    fn settlement_preserves_original_cloud_id_references() {
        let conn = db();
        let waiter = create_waiter(&conn, "Mimi", None, None).unwrap();
        pending_sale(&conn, waiter.id);
        let sale_id = conn.last_insert_rowid();
        conn.execute("UPDATE cloud_entity_ids SET entity_id='remote:waiter'
                      WHERE entity_type='waiters' AND local_id=?1", [waiter.id]).unwrap();
        crate::database::entity_ids::register(&conn, "sales", "remote:sale", sale_id).unwrap();
        assert_eq!(settle_waiter(&conn, waiter.id).unwrap(), 1);
        let (entity_id, payload): (String, String) = conn.query_row(
            "SELECT entity_id,payload FROM cloud_outbox WHERE entity_type='waiter_settlements'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(entity_id, "remote:waiter");
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["waiter_entity_id"], "remote:waiter");
        assert_eq!(event["sale_entity_ids"][0], "remote:sale");
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
