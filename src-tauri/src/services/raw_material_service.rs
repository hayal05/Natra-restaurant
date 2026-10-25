//! Raw materials catalog and purchase (cost) recording.
//!
//! Purchases are independent cost events — not linked to specific
//! cookable dishes — matching 003_raw_materials.sql.

use rusqlite::{Connection, OptionalExtension};

use crate::errors::{AppError, AppResult};
use crate::models::{RawMaterial, RawMaterialPurchase};

pub fn create_raw_material(conn: &Connection, name: &str, unit: &str) -> AppResult<RawMaterial> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("raw material name is required".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO raw_materials (name, unit) VALUES (?1, ?2)",
        rusqlite::params![name, unit],
    )?;
    let id = tx.last_insert_rowid();
    let material = tx.query_row("SELECT * FROM raw_materials WHERE id = ?1", [id], RawMaterial::from_row)?;
    let installation_id = crate::database::connection::ensure_installation_id(&tx)?;
    let entity_id = format!("{installation_id}:{id}");
    crate::database::entity_ids::register(&tx, "raw_materials", &entity_id, id)?;
    crate::sync::queue::enqueue_cloud_event(
        &tx, "raw_materials", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "material": material,
        }),
    )?;
    tx.commit()?;
    Ok(material)
}

/// Import a catalog material by its immutable cloud identity.
/// Existing mapped rows are never overwritten by an unrelated event.
#[allow(dead_code)]
pub(crate) fn apply_remote_raw_material(
    tx: &rusqlite::Transaction<'_>, entity_id: &str, payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote raw material".into());
    let material: RawMaterial = serde_json::from_value(
        payload.get("material").cloned().ok_or_else(invalid)?
    ).map_err(|_| invalid())?;
    if entity_id.trim().is_empty() || material.name.trim().is_empty() || material.unit.trim().is_empty() {
        return Err(invalid());
    }
    if crate::database::entity_ids::lookup(tx, "raw_materials", entity_id)?.is_some() {
        return Err(AppError::Validation("raw material identity already imported".into()));
    }
    tx.execute(
        "INSERT INTO raw_materials(name,unit,is_active,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![material.name,material.unit,material.is_active,
            material.created_at,material.updated_at],
    )?;
    crate::database::entity_ids::register(tx, "raw_materials", entity_id, tx.last_insert_rowid())?;
    Ok(())
}

pub fn list_raw_materials(conn: &Connection, only_active: bool) -> AppResult<Vec<RawMaterial>> {
    let sql = if only_active {
        "SELECT * FROM raw_materials WHERE is_active = 1 ORDER BY name"
    } else {
        "SELECT * FROM raw_materials ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], RawMaterial::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Record a raw-material purchase. `total_cost` is computed here
/// (quantity * unit_cost) rather than trusted from the caller.
pub fn record_purchase(
    conn: &Connection,
    raw_material_id: i64,
    quantity: f64,
    unit_cost: f64,
    supplier: Option<&str>,
    note: Option<&str>,
) -> AppResult<RawMaterialPurchase> {
    if quantity <= 0.0 {
        return Err(AppError::Validation("quantity must be greater than zero".into()));
    }
    if unit_cost < 0.0 {
        return Err(AppError::Validation("unit cost cannot be negative".into()));
    }
    let total_cost = quantity * unit_cost;

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO raw_material_purchases (raw_material_id, quantity, unit_cost, total_cost, supplier, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![raw_material_id, quantity, unit_cost, total_cost, supplier, note],
    )?;
    let id = tx.last_insert_rowid();
    let purchase = tx.query_row(
        "SELECT * FROM raw_material_purchases WHERE id = ?1",
        [id],
        RawMaterialPurchase::from_row,
    )?;
    let installation_id = crate::database::connection::ensure_installation_id(&tx)?;
    let material_entity_id: String = tx.query_row(
        "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='raw_materials' AND local_id=?1",
        [raw_material_id], |row| row.get(0),
    ).optional()?.unwrap_or_else(|| format!("{installation_id}:{raw_material_id}"));
    // Legacy local material rows get a persistent identity before referencing them.
    crate::database::entity_ids::register(&tx, "raw_materials", &material_entity_id, raw_material_id)?;
    let entity_id = format!("{installation_id}:{id}");
    crate::database::entity_ids::register(&tx, "raw_material_purchases", &entity_id, id)?;
    crate::sync::queue::enqueue_cloud_event(
        &tx, "raw_material_purchases", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "raw_material_entity_id": material_entity_id,
            "purchase": purchase,
        }),
    )?;
    tx.commit()?;
    Ok(purchase)
}

/// Insert a downloaded cost event in the caller's receipt/cursor transaction.
/// Never trust the sender's SQLite ID or overwrite a mapped financial purchase.
#[allow(dead_code)]
pub(crate) fn apply_remote_purchase(
    tx: &rusqlite::Transaction<'_>,
    entity_id: &str,
    payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote material purchase".into());
    let purchase: RawMaterialPurchase = serde_json::from_value(
        payload.get("purchase").cloned().ok_or_else(invalid)?
    ).map_err(|_| invalid())?;
    let material_entity_id = payload.get("raw_material_entity_id")
        .and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty())
        .ok_or_else(invalid)?;
    if entity_id.trim().is_empty() || !purchase.quantity.is_finite()
        || !purchase.unit_cost.is_finite() || !purchase.total_cost.is_finite() {
        return Err(invalid());
    }
    purchase.validate()?;
    if crate::database::entity_ids::lookup(tx, "raw_material_purchases", entity_id)?.is_some() {
        return Err(AppError::Validation("purchase already exists; refusing cost duplication".into()));
    }
    let local_material_id = crate::database::entity_ids::lookup(tx, "raw_materials", material_entity_id)?
        .ok_or_else(|| AppError::Validation("unknown raw material identity in purchase".into()))?;
    tx.execute(
        "INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,
         supplier,purchase_date,note,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![local_material_id,purchase.quantity,purchase.unit_cost,
            purchase.total_cost,purchase.supplier,purchase.purchase_date,purchase.note,
            purchase.created_at,purchase.updated_at],
    )?;
    let local_id = tx.last_insert_rowid();
    crate::database::entity_ids::register(tx, "raw_material_purchases", entity_id, local_id)?;
    Ok(())
}

pub fn list_purchases(
    conn: &Connection,
    raw_material_id: Option<i64>,
) -> AppResult<Vec<RawMaterialPurchase>> {
    let sql = if raw_material_id.is_some() {
        "SELECT * FROM raw_material_purchases WHERE raw_material_id = ?1 ORDER BY purchase_date DESC"
    } else {
        "SELECT * FROM raw_material_purchases ORDER BY purchase_date DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = match raw_material_id {
        Some(id) => stmt.query_map([id], RawMaterialPurchase::from_row)?,
        None => stmt.query_map([], RawMaterialPurchase::from_row)?,
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Total raw-material cost, optionally scoped to a date range
/// (inclusive, formatted 'YYYY-MM-DD' or full datetime strings).
pub fn total_cost(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<f64> {
    let total: Option<f64> = match (from, to) {
        (Some(f), Some(t)) => conn.query_row(
            "SELECT SUM(total_cost) FROM raw_material_purchases WHERE purchase_date BETWEEN ?1 AND ?2",
            [f, t],
            |row| row.get(0),
        )?,
        _ => conn.query_row("SELECT SUM(total_cost) FROM raw_material_purchases", [], |row| row.get(0))?,
    };
    Ok(total.unwrap_or(0.0))
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
    fn total_cost_is_computed_not_trusted() {
        let conn = db();
        let chicken = create_raw_material(&conn, "Chicken", "kg").unwrap();
        let purchase = record_purchase(&conn, chicken.id, 2.0, 3.0, Some("Local Farm"), None).unwrap();
        assert_eq!(purchase.total_cost, 6.0);

        record_purchase(&conn, chicken.id, 5.0, 3.0, None, None).unwrap();
        assert_eq!(total_cost(&conn, None, None).unwrap(), 21.0);
    }

    #[test]
    fn remote_material_import_maps_identity_and_rejects_duplicates() {
        let mut conn = db();
        let payload = serde_json::json!({"material":{
            "id":99,"name":"Imported Rice","unit":"kg","is_active":true,
            "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"
        }});
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-material-event",1)],
            |tx,_,_|apply_remote_raw_material(tx,"other:99",&payload),
        ).unwrap();
        let local_id = crate::database::entity_ids::lookup(&conn,"raw_materials","other:99")
            .unwrap().unwrap();
        assert_ne!(local_id,99);
        assert!(crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("duplicate-material-event",2)],
            |tx,_,_|apply_remote_raw_material(tx,"other:99",&payload),
        ).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),1);
    }

    #[test]
    fn material_and_purchase_creation_generate_outbox_events() {
        let conn = db();
        let material = create_raw_material(&conn, "Sugar", "kg").unwrap();
        let purchase = record_purchase(&conn, material.id, 2.0, 10.0, None, None).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cloud_outbox", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 2);
        let payload: String = conn.query_row(
            "SELECT payload FROM cloud_outbox WHERE entity_type='raw_material_purchases'",
            [], |r| r.get(0)
        ).unwrap();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["purchase"]["id"], purchase.id);
        assert_eq!(json["purchase"]["total_cost"], 20.0);
    }

    #[test]
    fn downloaded_purchase_preserves_cost_and_prevents_duplicate_replay() {
        let mut conn = db();
        let material = create_raw_material(&conn, "Beans", "kg").unwrap();
        let purchase = record_purchase(&conn, material.id, 2.0, 4.0, None, None).unwrap();
        let material_cloud_id: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='raw_materials'",
            [], |r| r.get(0),
        ).unwrap();
        let payload = serde_json::json!({
            "raw_material_entity_id":material_cloud_id,"purchase":purchase
        });
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("external-purchase-event",1)],
            |tx,_,_| apply_remote_purchase(tx,"remote:purchase",&payload)
        ).unwrap();
        assert_eq!(total_cost(&conn,None,None).unwrap(),16.0);
        assert!(crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("duplicate-purchase-event",2)],
            |tx,_,_| apply_remote_purchase(tx,"remote:purchase",&payload)
        ).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),1);
        assert_eq!(total_cost(&conn,None,None).unwrap(),16.0);
    }

    #[test]
    fn material_and_purchase_roll_back_on_outbox_failure() {
        let conn = db();
        let material = create_raw_material(&conn, "Flour", "kg").unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_raw_outbox BEFORE INSERT ON cloud_outbox
             BEGIN SELECT RAISE(FAIL, 'outbox unavailable'); END;"
        ).unwrap();
        assert!(create_raw_material(&conn, "Failed Material", "kg").is_err());
        assert!(record_purchase(&conn, material.id, 1.0, 8.0, None, None).is_err());
        let material_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM raw_materials", [], |r| r.get(0)
        ).unwrap();
        let purchase_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM raw_material_purchases", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(material_count, 1);
        assert_eq!(purchase_count, 0);
    }

    #[test]
    fn purchase_keeps_original_material_cloud_identity() {
        let conn = db();
        let material = create_raw_material(&conn, "Rice", "kg").unwrap();
        conn.execute(
            "UPDATE cloud_entity_ids SET entity_id='other-device:72' WHERE entity_type='raw_materials' AND local_id=?1",
            [material.id],
        ).unwrap();
        let purchase = record_purchase(&conn, material.id, 2.0, 7.0, None, None).unwrap();
        let (entity_id, payload): (String, String) = conn.query_row(
            "SELECT entity_id,payload FROM cloud_outbox WHERE entity_type='raw_material_purchases'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(
            crate::database::entity_ids::lookup(&conn, "raw_material_purchases", &entity_id).unwrap(),
            Some(purchase.id),
        );
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["raw_material_entity_id"], "other-device:72");
    }

    #[test]
    fn rejects_non_positive_quantity() {
        let conn = db();
        let chicken = create_raw_material(&conn, "Chicken", "kg").unwrap();
        let result = record_purchase(&conn, chicken.id, 0.0, 3.0, None, None);
        assert!(result.is_err());
    }
}
