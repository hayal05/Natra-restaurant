//! Migration runner.

use crate::errors::AppResult;
use rusqlite::Connection;

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial.sql", include_str!("../../../migrations/001_initial.sql")),
    ("002_sales.sql", include_str!("../../../migrations/002_sales.sql")),
    ("003_raw_materials.sql", include_str!("../../../migrations/003_raw_materials.sql")),
    ("004_expenses.sql", include_str!("../../../migrations/004_expenses.sql")),
    ("005_sync.sql", include_str!("../../../migrations/005_sync.sql")),
    ("006_backups.sql", include_str!("../../../migrations/006_backups.sql")),
    ("007_login_security.sql", include_str!("../../../migrations/007_login_security.sql")),
    ("008_waiter_profile_photo.sql", include_str!("../../../migrations/008_waiter_profile_photo.sql")),
];

pub fn run(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            filename TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    for (filename, sql) in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE filename = ?1)",
            [filename],
            |row| row.get(0),
        )?;
        if already_applied { continue; }
        conn.execute_batch(sql)?;
        conn.execute("INSERT INTO schema_migrations (filename) VALUES (?1)", [filename])?;
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
        run(&conn).unwrap();
        let profile_photo_column: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('waiters') WHERE name = 'profile_photo'", [], |row| row.get(0)
        ).unwrap();
        assert_eq!(profile_photo_column, 1);
    }
}
