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
    ("009_sale_reversal.sql", include_str!("../../../migrations/009_sale_reversal.sql")),
    ("010_raw_material_month_closing.sql", include_str!("../../../migrations/010_raw_material_month_closing.sql")),
    ("011_raw_material_closing_audit.sql", include_str!("../../../migrations/011_raw_material_closing_audit.sql")),
    ("012_guard_closed_material_purchases.sql", include_str!("../../../migrations/012_guard_closed_material_purchases.sql")),
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
        conn.execute_batch("SAVEPOINT natra_migration")?;
        let applied = (|| -> AppResult<()> {
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO schema_migrations (filename) VALUES (?1)", [filename])?;
            Ok(())
        })();
        if let Err(err) = applied {
            conn.execute_batch("ROLLBACK TO natra_migration; RELEASE natra_migration")?;
            return Err(err);
        }
        conn.execute_batch("RELEASE natra_migration")?;
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
    #[test]
    fn closing_schema_constraints_and_idempotence() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        run(&conn).unwrap();
        run(&conn).unwrap();
        conn.execute("INSERT INTO raw_materials(name,unit) VALUES ('Meat','kg')", []).unwrap();
        conn.execute("INSERT INTO raw_material_month_closings(period_month) VALUES ('2026-10-01')", []).unwrap();
        assert!(conn.execute("INSERT INTO raw_material_month_closings(period_month) VALUES ('2026-10-01')", []).is_err());
        assert!(conn.execute("INSERT INTO raw_material_month_closings(period_month) VALUES ('2026-10-15')", []).is_err());
        conn.execute("INSERT INTO raw_material_month_closing_lines(closing_id,raw_material_id,purchased_quantity,purchased_value,remaining_quantity,closing_value) VALUES(1,1,20,6000,5,1500)", []).unwrap();
        assert!(conn.execute("INSERT INTO raw_material_month_closing_lines(closing_id,raw_material_id) VALUES(1,1)", []).is_err());
        assert!(conn.execute("UPDATE raw_material_month_closing_lines SET remaining_quantity=21 WHERE id=1", []).is_err());
        assert!(conn.execute("UPDATE raw_material_month_closings SET status='closed' WHERE id=1", []).is_err());
        conn.execute("UPDATE raw_material_month_closings SET status='closed',closed_at=datetime('now') WHERE id=1", []).unwrap();
        let value: f64 = conn.query_row("SELECT closing_value FROM raw_material_month_closing_lines WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(value, 1500.0);
    }

    #[test]
    fn failed_sql_batch_rolls_back_in_savepoint() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("SAVEPOINT natra_migration").unwrap();
        assert!(conn.execute_batch("CREATE TABLE should_not_remain(x INTEGER); NOT VALID SQL;").is_err());
        conn.execute_batch("ROLLBACK TO natra_migration; RELEASE natra_migration").unwrap();
        let exists: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name='should_not_remain'", [], |r| r.get(0)).unwrap();
        assert_eq!(exists, 0);
    }

}
