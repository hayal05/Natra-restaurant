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

        Ok(Db(Mutex::new(conn)))
    }

    /// Open an in-memory database (used in tests) with migrations applied.
    #[allow(dead_code)]
    pub fn init_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        crate::database::migrations::run(&conn)?;
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
}
