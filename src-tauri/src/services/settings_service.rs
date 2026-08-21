//! Application settings: restaurant name/currency, and optional Turso
//! sync configuration. Backed by the single-row `settings` table
//! (id is always 1 — enforced by a DB CHECK constraint).

use rusqlite::Connection;

use crate::errors::{AppError, AppResult};
use crate::models::Settings;

/// Read the (always-present) settings row.
pub fn get_settings(conn: &Connection) -> AppResult<Settings> {
    Ok(conn.query_row("SELECT * FROM settings WHERE id = 1", [], Settings::from_row)?)
}

/// Update the general, user-facing settings (name shown in the app,
/// currency used for formatting). Does not touch sync configuration.
pub fn update_general(conn: &Connection, restaurant_name: &str, currency: &str) -> AppResult<Settings> {
    if restaurant_name.trim().is_empty() {
        return Err(AppError::Validation("restaurant name is required".into()));
    }
    if currency.trim().is_empty() {
        return Err(AppError::Validation("currency is required".into()));
    }

    conn.execute(
        "UPDATE settings SET restaurant_name = ?1, currency = ?2, updated_at = datetime('now') WHERE id = 1",
        rusqlite::params![restaurant_name, currency],
    )?;
    get_settings(conn)
}

/// Enable Turso cloud sync with the given connection details.
/// The `sync` module reads these fields whenever `sync_enabled = 1`.
pub fn enable_sync(conn: &Connection, turso_url: &str, turso_auth_token: &str) -> AppResult<Settings> {
    if turso_url.trim().is_empty() || turso_auth_token.trim().is_empty() {
        return Err(AppError::Validation(
            "a Turso URL and auth token are required to enable sync".into(),
        ));
    }
    conn.execute(
        "UPDATE settings SET sync_enabled = 1, turso_url = ?1, turso_auth_token = ?2, updated_at = datetime('now') WHERE id = 1",
        rusqlite::params![turso_url, turso_auth_token],
    )?;
    get_settings(conn)
}

/// Disable sync. Credentials are cleared so a stale token can't linger
/// around and be reused if sync is re-enabled incorrectly later.
pub fn disable_sync(conn: &Connection) -> AppResult<Settings> {
    conn.execute(
        "UPDATE settings SET sync_enabled = 0, turso_url = NULL, turso_auth_token = NULL, updated_at = datetime('now') WHERE id = 1",
        [],
    )?;
    get_settings(conn)
}

/// Toggle the daily Excel backup on/off (see `crate::backup`). On by
/// default — the background loop checks this every tick, so flipping
/// it here takes effect on the next tick without a restart, same as
/// sync.
pub fn set_backup_enabled(conn: &Connection, enabled: bool) -> AppResult<Settings> {
    conn.execute(
        "UPDATE settings SET backup_enabled = ?1, updated_at = datetime('now') WHERE id = 1",
        rusqlite::params![enabled],
    )?;
    get_settings(conn)
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
    fn defaults_are_seeded_on_migration() {
        let conn = db();
        let settings = get_settings(&conn).unwrap();
        assert_eq!(settings.restaurant_name, "My Restaurant");
        assert_eq!(settings.currency, "USD");
        assert!(!settings.is_initialized);
        assert!(!settings.sync_enabled);
        assert!(settings.backup_enabled);
    }

    #[test]
    fn backup_enabled_toggles() {
        let conn = db();
        let disabled = set_backup_enabled(&conn, false).unwrap();
        assert!(!disabled.backup_enabled);

        let enabled = set_backup_enabled(&conn, true).unwrap();
        assert!(enabled.backup_enabled);
    }

    #[test]
    fn update_general_persists_changes() {
        let conn = db();
        let updated = update_general(&conn, "NATRA Grill", "ETB").unwrap();
        assert_eq!(updated.restaurant_name, "NATRA Grill");
        assert_eq!(updated.currency, "ETB");
    }

    #[test]
    fn empty_restaurant_name_is_rejected() {
        let conn = db();
        let result = update_general(&conn, "  ", "USD");
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn enable_then_disable_sync_round_trips() {
        let conn = db();
        let enabled = enable_sync(&conn, "libsql://example.turso.io", "secret-token").unwrap();
        assert!(enabled.sync_enabled);

        let disabled = disable_sync(&conn).unwrap();
        assert!(!disabled.sync_enabled);
    }

    #[test]
    fn enable_sync_requires_both_fields() {
        let conn = db();
        let result = enable_sync(&conn, "", "secret-token");
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
