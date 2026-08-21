//! Migration runner.
//!
//! The actual SQL lives in the top-level `/migrations` folder (shared,
//! human-readable, versionable independent of Rust). We embed each file
//! at compile time with `include_str!` and apply any that haven't run
//! yet, tracked in a local `schema_migrations` table.

use crate::errors::AppResult;
use rusqlite::Connection;

/// (filename, sql) pairs, in the order they must be applied.
const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial.sql", include_str!("../../../migrations/001_initial.sql")),
    ("002_sales.sql", include_str!("../../../migrations/002_sales.sql")),
    ("003_raw_materials.sql", include_str!("../../../migrations/003_raw_materials.sql")),
    ("004_expenses.sql", include_str!("../../../migrations/004_expenses.sql")),
    ("005_sync.sql", include_str!("../../../migrations/005_sync.sql")),
    ("006_backups.sql", include_str!("../../../migrations/006_backups.sql")),
    ("007_login_security.sql", include_str!("../../../migrations/007_login_security.sql")),
];

pub fn run(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            filename    TEXT PRIMARY KEY,
            applied_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    for (filename, sql) in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE filename = ?1)",
            [filename],
            |row| row.get(0),
        )?;

        if already_applied {
            continue;
        }

        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_migrations (filename) VALUES (?1)",
            [filename],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_cleanly_and_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        run(&conn).unwrap();
        // Running twice must be a no-op, not an error.
        run(&conn).unwrap();

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // 13 business/sync/backup tables + schema_migrations
        assert_eq!(table_count, 14);
    }

    #[test]
    fn ready_made_cookable_constraint_is_enforced() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run(&conn).unwrap();

        let result = conn.execute(
            "INSERT INTO items (name, type, purchase_cost, selling_price) VALUES ('Bad', 'cookable', 3.0, 5.0)",
            [],
        );
        assert!(result.is_err(), "cookable item with a purchase_cost must be rejected");
    }
}
