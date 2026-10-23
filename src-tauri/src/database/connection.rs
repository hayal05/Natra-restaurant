//! Database connection management.
//!
//! The app uses a single local SQLite file as the source of truth.
//! `Db` wraps a `rusqlite::Connection` behind a `Mutex` so it can be
//! shared as Tauri managed state across command handlers.

use crate::errors::AppResult;
use rusqlite::Connection;
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
    fn independent_installations_have_different_ids() {
        let first = Db::init_in_memory().unwrap();
        let second = Db::init_in_memory().unwrap();
        assert_ne!(
            ensure_installation_id(&first.lock()).unwrap(),
            ensure_installation_id(&second.lock()).unwrap()
        );
    }

}
