//! Persistent global-to-local IDs for cloud event application.
//! Call register inside the same transaction as inserting the business row.
use rusqlite::{Connection, OptionalExtension};
use crate::errors::{AppError, AppResult};

pub fn lookup(conn: &Connection, entity_type: &str, entity_id: &str) -> AppResult<Option<i64>> {
    let id = conn.query_row(
        "SELECT local_id FROM cloud_entity_ids WHERE entity_type=?1 AND entity_id=?2",
        rusqlite::params![entity_type, entity_id], |row| row.get(0),
    ).optional()?;
    Ok(id)
}

/// An existing mapping is immutable. SQLite's UNIQUE(type,local_id)
/// also prevents assigning a local row to a second global identity.
pub fn register(conn: &Connection, entity_type: &str, entity_id: &str, local_id: i64) -> AppResult<()> {
    if entity_type.trim().is_empty() || entity_id.trim().is_empty() || local_id <= 0 {
        return Err(AppError::Validation("invalid cloud entity mapping".into()));
    }
    if let Some(existing) = lookup(conn, entity_type, entity_id)? {
        if existing == local_id { return Ok(()); }
        return Err(AppError::Validation("cloud entity identity cannot be remapped".into()));
    }
    conn.execute(
        "INSERT INTO cloud_entity_ids(entity_type,entity_id,local_id) VALUES(?1,?2,?3)",
        rusqlite::params![entity_type,entity_id,local_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn mapping_is_stable_and_scoped_by_entity_type() {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        assert_eq!(lookup(&conn, "items", "device-a:1").unwrap(), None);
        register(&conn, "items", "device-a:1", 7).unwrap();
        register(&conn, "items", "device-a:1", 7).unwrap();
        assert_eq!(lookup(&conn, "items", "device-a:1").unwrap(), Some(7));
        assert!(register(&conn, "items", "device-a:1", 8).is_err());
        assert!(register(&conn, "items", "device-b:1", 7).is_err());
        register(&conn, "waiters", "device-a:1", 7).unwrap();
        assert_eq!(lookup(&conn, "waiters", "device-a:1").unwrap(), Some(7));
    }

    #[test]
    fn mapping_rolls_back_with_business_transaction() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        register(&tx, "items", "device-a:1", 1).unwrap();
        tx.rollback().unwrap();
        assert_eq!(lookup(&conn, "items", "device-a:1").unwrap(), None);
    }
}
