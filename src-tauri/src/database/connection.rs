//! Database connection management.
//!
//! The app uses a single local SQLite file as the source of truth.
//! `Db` wraps a `rusqlite::Connection` behind a `Mutex` so it can be
//! shared as Tauri managed state across command handlers.

use crate::errors::AppResult;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub struct Db(pub Mutex<Connection>);

impl Db {
    /// Open (or create) the local SQLite database at `path`, apply
    /// pragmas, and run any pending migrations.
    pub fn init(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;

        // Enforce foreign keys — SQLite has them off by default per-connection.
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;

        crate::database::migrations::run(&conn)?;
        ensure_installation_id(&conn)?;

        Ok(Db(Mutex::new(conn)))
    }

    /// Open an in-memory database (used in tests) with migrations applied.
    #[allow(dead_code)]
    pub fn init_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        crate::database::migrations::run(&conn)?;
        ensure_installation_id(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }

    /// Acquire the connection lock. Every command handler goes through
    /// this instead of `self.0.lock().unwrap()` directly, because a
    /// bare `.unwrap()` there is a correctness bug, not just a style
    /// nit: `rusqlite::Connection` operations never panic (they return
    /// `Result`), so the only way this mutex can be poisoned is a
    /// panic somewhere else entirely unrelated to the data underneath
    /// it — a bug in one command's logic, an out-of-bounds index, a
    /// bad `expect()`. `.unwrap()`-ing a poisoned mutex there would
    /// turn that one bug into a permanently bricked app: every command
    /// after it panics too, forever, until the process restarts.
    ///
    /// The connection itself is fine even after a poisoning panic —
    /// nothing about *using* it can corrupt its internal state — so we
    /// recover the guard and keep going, logging so the underlying
    /// panic doesn't go unnoticed.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!(
                    "[db] mutex was poisoned by a panic elsewhere in the app; \
                     recovering the connection so the app keeps working"
                );
                poisoned.into_inner()
            }
        }
    }
}

/// Read the last cloud event sequence durably applied to this installation.
/// A missing, negative or malformed cursor is an error, never an implicit reset:
/// resetting to zero silently could replay old financial events.
#[allow(dead_code)]
pub(crate) fn read_cloud_pull_cursor(conn: &Connection) -> AppResult<i64> {
    let value: String = conn.query_row(
        "SELECT value FROM sync_meta WHERE key = 'cloud_last_applied_sequence'",
        [],
        |row| row.get(0),
    )?;
    let cursor = value.parse::<i64>().map_err(|_| {
        rusqlite::Error::InvalidParameterName("invalid cloud pull cursor".into())
    })?;
    if cursor < 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "negative cloud pull cursor".into(),
        ).into());
    }
    Ok(cursor)
}

/// Advance the cloud pull cursor inside the caller's SQLite transaction.
/// Call only after successfully applying every event through this sequence.
/// If the transaction rolls back, the cursor update rolls back with it.
#[allow(dead_code)]
pub(crate) fn advance_cloud_pull_cursor(
    tx: &rusqlite::Transaction<'_>,
    new_cursor: i64,
) -> AppResult<()> {
    if new_cursor < 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "negative cloud pull cursor".into(),
        ).into());
    }
    let current = read_cloud_pull_cursor(tx)?;
    if new_cursor < current {
        return Err(rusqlite::Error::InvalidParameterName(
            "cloud pull cursor cannot move backwards".into(),
        ).into());
    }
    let updated = tx.execute(
        "UPDATE sync_meta SET value = ?1 WHERE key = 'cloud_last_applied_sequence'",
        [new_cursor.to_string()],
    )?;
    if updated != 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "cloud pull cursor metadata missing".into(),
        ).into());
    }
    Ok(())
}

/// Check whether an incoming cloud event already has an applied receipt.
/// The caller can pass a SQLite transaction to keep this check alongside
/// event application. An empty event ID is not a valid event.
#[allow(dead_code)]
pub(crate) fn is_cloud_event_applied(conn: &Connection, event_id: &str) -> AppResult<bool> {
    if event_id.trim().is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "cloud event ID must not be empty".into(),
        ).into());
    }
    let applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM cloud_applied_events WHERE event_id = ?1)",
        [event_id],
        |row| row.get(0),
    )?;
    Ok(applied)
}

/// Record an event as applied in the caller's SQLite transaction.
/// Call only after successfully applying its business changes, and commit
/// this receipt together with the cloud cursor. Conflicting duplicates error.
#[allow(dead_code)]
pub(crate) fn record_cloud_applied_event(
    tx: &rusqlite::Transaction<'_>,
    event_id: &str,
    sequence_id: i64,
) -> AppResult<()> {
    if event_id.trim().is_empty() || sequence_id <= 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "cloud event ID and positive sequence are required".into(),
        ).into());
    }
    tx.execute(
        "INSERT INTO cloud_applied_events(event_id, sequence_id) VALUES (?1, ?2)",
        rusqlite::params![event_id, sequence_id],
    )?;
    Ok(())
}

/// Apply a strictly ordered downloaded batch atomically with receipts and cursor.
/// The callback MUST perform actual business-row changes; no callback means no
/// event application. Empty batches leave the cursor unchanged.
#[allow(dead_code)]
pub(crate) fn apply_cloud_event_batch<F>(
    conn: &mut Connection,
    events: &[(&str, i64)],
    mut apply_business_event: F,
) -> AppResult<()>
where
    F: FnMut(&rusqlite::Transaction<'_>, &str, i64) -> AppResult<()>,
{
    // Validate the complete batch before invoking any business mutation.
    let mut seen_ids = std::collections::HashSet::new();
    let mut previous_sequence = 0_i64;
    for &(event_id, sequence_id) in events {
        if event_id.trim().is_empty()
            || sequence_id <= previous_sequence
            || !seen_ids.insert(event_id)
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "cloud batch contains duplicate IDs or unordered/invalid sequences".into(),
            ).into());
        }
        previous_sequence = sequence_id;
    }

    let tx = conn.transaction()?;
    let mut cursor = read_cloud_pull_cursor(&tx)?;
    for &(event_id, sequence_id) in events {
        if event_id.trim().is_empty() || sequence_id <= cursor {
            return Err(rusqlite::Error::InvalidParameterName(
                "cloud event sequences must increase and IDs must be nonempty".into(),
            ).into());
        }
        // A previously applied event must match its original sequence.
        let existing: Option<i64> = tx.query_row(
            "SELECT sequence_id FROM cloud_applied_events WHERE event_id=?1",
            [event_id],
            |row| row.get(0),
        ).optional()?;
        match existing {
            Some(previous) if previous == sequence_id => {}
            Some(_) => return Err(rusqlite::Error::InvalidParameterName(
                "event ID reused with a different cloud sequence".into(),
            ).into()),
            None => {
                apply_business_event(&tx, event_id, sequence_id)?;
                record_cloud_applied_event(&tx, event_id, sequence_id)?;
            }
        }
        cursor = sequence_id;
    }
    if !events.is_empty() {
        advance_cloud_pull_cursor(&tx, cursor)?;
    }
    tx.commit()?;
    Ok(())
}

/// Reuse the device_id key already reserved by migration 005.
/// The ID belongs to the installation, never to the signed-in user.
/// SQLite's randomblob() supplies random bytes without introducing a new crate.
fn ensure_installation_id(conn: &Connection) -> AppResult<String> {
    let existing: Option<String> = conn.query_row(
        "SELECT value FROM sync_meta WHERE key = 'device_id'",
        [],
        |row| row.get(0),
    )?;
    if let Some(id) = existing {
        if !id.trim().is_empty() {
            return Ok(id);
        }
    }

    let mut bytes: Vec<u8> = conn.query_row("SELECT randomblob(16)", [], |row| row.get(0))?;
    // RFC 4122 / RFC 9562 UUID version 4 and RFC variant.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let id = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]
    );
    conn.execute(
        "UPDATE sync_meta SET value = ?1 WHERE key = 'device_id' AND (value IS NULL OR trim(value) = '')",
        [&id],
    )?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_recovers_after_a_panic_poisons_the_mutex() {
        let db = Db::init_in_memory().unwrap();

        // Poison the mutex the same way a bug in some unrelated command
        // would: panic while holding the guard.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _conn = db.lock();
            panic!("simulated bug in a command handler");
        }));
        assert!(result.is_err());

        // A naive `self.0.lock().unwrap()` would panic here too. `lock()`
        // must not — the app has to keep serving other commands.
        let conn = db.lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn installation_id_is_uuid_v4_and_survives_reinitialization() {
        let db = Db::init_in_memory().unwrap();
        let conn = db.lock();
        let first = ensure_installation_id(&conn).unwrap();
        assert_eq!(first.len(), 36);
        assert_eq!(first.chars().nth(14), Some('4'));
        assert!(matches!(first.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
        assert_eq!(first, ensure_installation_id(&conn).unwrap());
        let saved: String = conn.query_row(
            "SELECT value FROM sync_meta WHERE key='device_id'", [], |row| row.get(0)
        ).unwrap();
        assert_eq!(saved, first);
    }

    #[test]
    fn cloud_cursor_starts_at_zero_and_reads_saved_value() {
        let db = Db::init_in_memory().unwrap();
        let conn = db.lock();
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 0);
        conn.execute(
            "UPDATE sync_meta SET value='123' WHERE key='cloud_last_applied_sequence'",
            [],
        ).unwrap();
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 123);
    }

    #[test]
    fn malformed_cloud_cursor_is_rejected() {
        let db = Db::init_in_memory().unwrap();
        let conn = db.lock();
        for bad in ["invalid", "-1", ""] {
            conn.execute(
                "UPDATE sync_meta SET value=?1 WHERE key='cloud_last_applied_sequence'",
                [bad],
            ).unwrap();
            assert!(read_cloud_pull_cursor(&conn).is_err());
        }
    }

    #[test]
    fn cloud_cursor_update_commits_with_transaction() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let tx = conn.transaction().unwrap();
        advance_cloud_pull_cursor(&tx, 42).unwrap();
        tx.commit().unwrap();
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 42);
    }

    #[test]
    fn cloud_cursor_update_rolls_back_and_cannot_regress() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        {
            let tx = conn.transaction().unwrap();
            advance_cloud_pull_cursor(&tx, 10).unwrap();
        }
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 0);
        let tx = conn.transaction().unwrap();
        advance_cloud_pull_cursor(&tx, 10).unwrap();
        assert!(advance_cloud_pull_cursor(&tx, 9).is_err());
        assert!(advance_cloud_pull_cursor(&tx, -1).is_err());
        tx.commit().unwrap();
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 10);
    }

    #[test]
    fn applied_cloud_event_check_recognizes_receipts() {
        let db = Db::init_in_memory().unwrap();
        let conn = db.lock();
        assert!(!is_cloud_event_applied(&conn, "event-a").unwrap());
        conn.execute(
            "INSERT INTO cloud_applied_events(event_id, sequence_id) VALUES (?1, ?2)",
            rusqlite::params!["event-a", 1],
        ).unwrap();
        assert!(is_cloud_event_applied(&conn, "event-a").unwrap());
        assert!(!is_cloud_event_applied(&conn, "event-b").unwrap());
        assert!(is_cloud_event_applied(&conn, " ").is_err());
    }

    #[test]
    fn cloud_event_receipt_commits_and_rejects_duplicates() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let tx = conn.transaction().unwrap();
        record_cloud_applied_event(&tx, "event-1", 1).unwrap();
        assert!(record_cloud_applied_event(&tx, "event-1", 1).is_err());
        assert!(record_cloud_applied_event(&tx, "event-2", 1).is_err());
        assert!(record_cloud_applied_event(&tx, "", 2).is_err());
        assert!(record_cloud_applied_event(&tx, "event-2", 0).is_err());
        advance_cloud_pull_cursor(&tx, 1).unwrap();
        tx.commit().unwrap();
        assert!(is_cloud_event_applied(&conn, "event-1").unwrap());
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 1);
    }

    #[test]
    fn cloud_event_receipt_rollback_preserves_retry() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        {
            let tx = conn.transaction().unwrap();
            record_cloud_applied_event(&tx, "event-retry", 5).unwrap();
            advance_cloud_pull_cursor(&tx, 5).unwrap();
            // Dropping the transaction simulates a failed batch.
        }
        assert!(!is_cloud_event_applied(&conn, "event-retry").unwrap());
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 0);
    }

    #[test]
    fn cloud_batch_commits_events_receipts_and_cursor() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        conn.execute_batch("CREATE TABLE test_cloud_values(value TEXT NOT NULL);").unwrap();
        apply_cloud_event_batch(&mut conn, &[("one", 1), ("two", 2)], |tx, id, _| {
            tx.execute("INSERT INTO test_cloud_values(value) VALUES (?1)", [id])?;
            Ok(())
        }).unwrap();
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 2);
        assert!(is_cloud_event_applied(&conn, "one").unwrap());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM test_cloud_values", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn cloud_batch_failure_rolls_back_business_receipts_and_cursor() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        conn.execute_batch("CREATE TABLE test_cloud_values(value TEXT NOT NULL);").unwrap();
        let result = apply_cloud_event_batch(&mut conn, &[("one", 1), ("two", 2)], |tx, id, _| {
            if id == "two" {
                return Err(rusqlite::Error::InvalidParameterName("test failure".into()).into());
            }
            tx.execute("INSERT INTO test_cloud_values(value) VALUES (?1)", [id])?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 0);
        assert!(!is_cloud_event_applied(&conn, "one").unwrap());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM test_cloud_values", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn invalid_cloud_batch_is_rejected_before_applying_any_events() {
        let db = Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        for events in [
            vec![("same", 1), ("same", 2)],
            vec![("later", 2), ("earlier", 1)],
            vec![("one", 1), ("two", 1)],
            vec![("", 1)],
            vec![("zero", 0)],
        ] {
            let mut applied = 0;
            let result = apply_cloud_event_batch(&mut conn, &events, |_, _, _| {
                applied += 1;
                Ok(())
            });
            assert!(result.is_err());
            assert_eq!(applied, 0);
            assert_eq!(read_cloud_pull_cursor(&conn).unwrap(), 0);
        }
    }

    #[test]
    fn independent_installations_have_different_ids() {
        let first = Db::init_in_memory().unwrap();
        let second = Db::init_in_memory().unwrap();
        assert_ne!(
            ensure_installation_id(&first.lock()).unwrap(),
            ensure_installation_id(&second.lock()).unwrap()
        );
    }

}
