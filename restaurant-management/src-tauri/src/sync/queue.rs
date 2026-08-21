//! The local outbox (`sync_queue`, see `migrations/005_sync.sql`).
//!
//! Every local write to a syncable table is meant to land a row here —
//! that's the hook point services will call into as they're wired up
//! for sync (see `sync::SYNCABLE_TABLES`). This module doesn't care who
//! writes to the queue, only how entries are read back out and marked
//! as they move through push: pending -> synced, or pending -> pending
//! (retried) -> failed once too many attempts have been burned.

use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::database::schema::sync_operation as op;
use crate::database::schema::sync_status as status;
use crate::errors::AppResult;

/// A single outbox entry, as read back from `sync_queue`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: i64,
    pub table_name: String,
    pub record_id: i64,
    pub operation: Operation,
    /// JSON snapshot of the row at write time. `None` for deletes.
    pub payload: Option<String>,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub synced_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Insert,
    Update,
    Delete,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Operation::Insert => op::INSERT,
            Operation::Update => op::UPDATE,
            Operation::Delete => op::DELETE,
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            v if v == op::INSERT => Operation::Insert,
            v if v == op::UPDATE => Operation::Update,
            v if v == op::DELETE => Operation::Delete,
            other => unreachable!("sync_queue.operation CHECK constraint allows only insert/update/delete, got {other}"),
        }
    }
}

/// A queue entry is retried this many times before it's parked as
/// `failed` and left for manual inspection (surfaced later via a
/// settings/sync-status screen) instead of retried forever.
const MAX_ATTEMPTS: i64 = 5;

impl QueueEntry {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let operation: String = row.get("operation")?;
        Ok(QueueEntry {
            id: row.get("id")?,
            table_name: row.get("table_name")?,
            record_id: row.get("record_id")?,
            operation: Operation::from_str(&operation),
            payload: row.get("payload")?,
            status: row.get("status")?,
            attempts: row.get("attempts")?,
            last_error: row.get("last_error")?,
            created_at: row.get("created_at")?,
            synced_at: row.get("synced_at")?,
        })
    }
}

/// Queue a local mutation for the next sync push. `payload` should be a
/// JSON object snapshot of the row (column name -> value) for insert/
/// update; pass `None` for deletes, since there's nothing left to push
/// but the id.
pub fn enqueue(
    conn: &Connection,
    table_name: &str,
    record_id: i64,
    operation: Operation,
    payload: Option<&serde_json::Value>,
) -> AppResult<()> {
    let payload_json = payload.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO sync_queue (table_name, record_id, operation, payload, status)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![table_name, record_id, operation.as_str(), payload_json, status::PENDING],
    )?;
    Ok(())
}

/// Oldest-first batch of entries still waiting to be pushed.
pub fn pending(conn: &Connection, limit: i64) -> AppResult<Vec<QueueEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, table_name, record_id, operation, payload, status, attempts, last_error, created_at, synced_at
         FROM sync_queue WHERE status = ?1 ORDER BY id ASC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![status::PENDING, limit], QueueEntry::from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark an entry as successfully pushed.
pub fn mark_synced(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_queue SET status = ?1, synced_at = datetime('now'), last_error = NULL WHERE id = ?2",
        rusqlite::params![status::SYNCED, id],
    )?;
    Ok(())
}

/// Record a failed push attempt. Stays `pending` (so the next sync
/// cycle retries it) until `MAX_ATTEMPTS` is reached, then flips to
/// `failed` so a broken row can't spin forever without anyone noticing.
pub fn mark_failed(conn: &Connection, id: i64, error: &str) -> AppResult<()> {
    let attempts: i64 = conn.query_row(
        "SELECT attempts FROM sync_queue WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    let attempts = attempts + 1;
    let next_status = if attempts >= MAX_ATTEMPTS { status::FAILED } else { status::PENDING };

    conn.execute(
        "UPDATE sync_queue SET attempts = ?1, last_error = ?2, status = ?3 WHERE id = ?4",
        rusqlite::params![attempts, error, next_status, id],
    )?;
    Ok(())
}

/// Counts by status, used for a sync-status readout in the UI
/// (pending backlog size, and whether anything needs manual attention).
pub fn counts(conn: &Connection) -> AppResult<QueueCounts> {
    let mut count_for = |s: &str| -> AppResult<i64> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sync_queue WHERE status = ?1",
            [s],
            |row| row.get(0),
        )?)
    };
    Ok(QueueCounts {
        pending: count_for(status::PENDING)?,
        synced: count_for(status::SYNCED)?,
        failed: count_for(status::FAILED)?,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueCounts {
    pub pending: i64,
    pub synced: i64,
    pub failed: i64,
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
    fn enqueue_then_read_pending() {
        let conn = db();
        let payload = serde_json::json!({"id": 1, "full_name": "Alex"});
        enqueue(&conn, "waiters", 1, Operation::Insert, Some(&payload)).unwrap();

        let entries = pending(&conn, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].table_name, "waiters");
        assert_eq!(entries[0].operation, Operation::Insert);
        assert!(entries[0].payload.as_deref().unwrap().contains("Alex"));
    }

    #[test]
    fn mark_synced_removes_entry_from_pending() {
        let conn = db();
        enqueue(&conn, "waiters", 1, Operation::Insert, None).unwrap();
        let id = pending(&conn, 10).unwrap()[0].id;

        mark_synced(&conn, id).unwrap();

        assert!(pending(&conn, 10).unwrap().is_empty());
        assert_eq!(counts(&conn).unwrap().synced, 1);
    }

    #[test]
    fn mark_failed_retries_until_max_attempts_then_parks() {
        let conn = db();
        enqueue(&conn, "waiters", 1, Operation::Update, None).unwrap();
        let id = pending(&conn, 10).unwrap()[0].id;

        for _ in 0..MAX_ATTEMPTS - 1 {
            mark_failed(&conn, id, "network blip").unwrap();
            // Still pending — eligible for the next retry.
            assert_eq!(pending(&conn, 10).unwrap().len(), 1);
        }

        mark_failed(&conn, id, "still failing").unwrap();
        assert!(pending(&conn, 10).unwrap().is_empty());
        assert_eq!(counts(&conn).unwrap().failed, 1);
    }

    #[test]
    fn delete_entries_carry_no_payload() {
        let conn = db();
        enqueue(&conn, "items", 7, Operation::Delete, None).unwrap();
        let entry = &pending(&conn, 10).unwrap()[0];
        assert_eq!(entry.operation, Operation::Delete);
        assert!(entry.payload.is_none());
    }
}
