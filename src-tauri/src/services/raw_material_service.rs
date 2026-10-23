//! Raw materials catalog and purchase (cost) recording.
//!
//! Purchases are independent cost events — not linked to specific
//! cookable dishes — matching 003_raw_materials.sql.

use rusqlite::Connection;

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
    crate::sync::queue::enqueue_cloud_event(
        &tx, "raw_materials", &format!("{installation_id}:{id}"), "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "material": material,
        }),
    )?;
    tx.commit()?;
    Ok(material)
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
    crate::sync::queue::enqueue_cloud_event(
        &tx, "raw_material_purchases", &format!("{installation_id}:{id}"), "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "raw_material_entity_id": format!("{installation_id}:{raw_material_id}"),
            "purchase": purchase,
        }),
    )?;
    tx.commit()?;
    Ok(purchase)
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
    fn rejects_non_positive_quantity() {
        let conn = db();
        let chicken = create_raw_material(&conn, "Chicken", "kg").unwrap();
        let result = record_purchase(&conn, chicken.id, 0.0, 3.0, None, None);
        assert!(result.is_err());
    }
}
