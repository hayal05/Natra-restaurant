//! Orchestrates one full sync cycle (push then pull) and the periodic
//! background loop that drives it automatically when sync is enabled.
//!
//! Nothing here talks to `rusqlite` types directly beyond the tiny
//! `sync_meta` cursor read/write below — the heavy lifting is
//! `turso::sync::push_pending` / `pull_updates`, this module just:
//!   1. reads whether sync is on and what the last-pull cursor was
//!   2. connects, pushes, pulls
//!   3. advances the cursor and reports what happened

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::database::Db;
use crate::errors::AppResult;
use crate::services::settings_service;

use super::turso::{self, TursoClient};

/// How often the background loop attempts a sync cycle while the app
/// is open. Deliberately not configurable from the UI (yet) — this is
/// a restaurant POS, not a chat app; a few minutes of staleness on a
/// second device is fine, and anything faster just burns battery/data
/// for no benefit offline-first apps care about.
const SYNC_INTERVAL: Duration = Duration::from_secs(120);

/// Outcome of one sync cycle, returned to the frontend by the
/// `sync_now` command so a "Sync" button can show something more
/// useful than a spinner.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub ran: bool,
    pub pushed: usize,
    pub pulled: usize,
    pub error: Option<String>,
}

impl SyncReport {
    fn disabled() -> Self {
        SyncReport { ran: false, pushed: 0, pulled: 0, error: None }
    }

    fn failed(err: &crate::errors::AppError) -> Self {
        SyncReport { ran: true, pushed: 0, pulled: 0, error: Some(err.to_string()) }
    }
}

/// Start the background loop as a fire-and-forget task on Tauri's
/// async runtime. Called once from `lib.rs`'s `setup()`; the loop
/// checks `settings.sync_enabled` on every tick, so enabling/disabling
/// sync from the Settings page takes effect on the next tick without
/// needing to restart the loop itself.
pub fn start_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(SYNC_INTERVAL);
        loop {
            interval.tick().await;
            let db = app.state::<Db>();
            match run_once(&db).await {
                Ok(report) if report.ran => {
                    if let Some(err) = &report.error {
                        eprintln!("[sync] cycle failed: {err}");
                    } else {
                        eprintln!("[sync] pushed {} pulled {}", report.pushed, report.pulled);
                    }
                }
                Ok(_) => {} // sync disabled — nothing to log every two minutes
                Err(e) => eprintln!("[sync] could not read settings: {e}"),
            }
        }
    });
}

/// Run exactly one push+pull cycle right now. Used by both the
/// background loop and the manual `sync_now` command — a manual sync
/// is just this function called out of band.
pub async fn run_once(db: &Db) -> AppResult<SyncReport> {
    let settings = {
        let conn = db.lock();
        settings_service::get_settings(&conn)?
    };

    if !settings.sync_enabled {
        return Ok(SyncReport::disabled());
    }

    let (Some(url), Some(token)) = (settings.turso_url.as_deref(), settings.turso_auth_token.as_deref()) else {
        return Ok(SyncReport::failed(&crate::errors::AppError::Sync(
            "sync is enabled but no Turso credentials are configured".into(),
        )));
    };

    match run_cycle(db, url, token).await {
        Ok((pushed, pulled)) => Ok(SyncReport { ran: true, pushed, pulled, error: None }),
        Err(e) => Ok(SyncReport::failed(&e)),
    }
}

async fn run_cycle(db: &Db, url: &str, token: &str) -> AppResult<(usize, usize)> {
    let client: TursoClient = turso::connection::connect(url, token).await?;

    let pushed = turso::sync::push_pending(db, &client).await?;

    let since = read_cursor(db)?;
    let pulled = turso::sync::pull_updates(db, &client, &since).await?;

    write_cursor(db)?;

    Ok((pushed, pulled))
}

/// `sync_meta.last_synced_at` — the timestamp cursor pull uses so it
/// only asks Turso for rows touched since the last successful cycle.
/// Starts at the epoch so a brand new device's first sync pulls
/// everything.
fn read_cursor(db: &Db) -> AppResult<String> {
    let conn = db.lock();
    let value: Option<String> = conn.query_row(
        "SELECT value FROM sync_meta WHERE key = 'last_synced_at'",
        [],
        |row| row.get(0),
    )?;
    Ok(value.unwrap_or_else(|| "1970-01-01 00:00:00".to_string()))
}

fn write_cursor(db: &Db) -> AppResult<()> {
    let conn = db.lock();
    conn.execute(
        "UPDATE sync_meta SET value = datetime('now') WHERE key = 'last_synced_at'",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use rusqlite::Connection;

    fn db() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        Db(std::sync::Mutex::new(conn))
    }

    #[test]
    fn cursor_defaults_to_epoch_before_first_sync() {
        let db = db();
        assert_eq!(read_cursor(&db).unwrap(), "1970-01-01 00:00:00");
    }

    #[test]
    fn cursor_advances_after_write() {
        let db = db();
        write_cursor(&db).unwrap();
        assert_ne!(read_cursor(&db).unwrap(), "1970-01-01 00:00:00");
    }

    #[tokio::test]
    async fn run_once_is_a_no_op_when_sync_disabled() {
        let db = db();
        let report = run_once(&db).await.unwrap();
        assert!(!report.ran);
        assert!(report.error.is_none());
    }

    #[tokio::test]
    async fn run_once_reports_missing_credentials_when_enabled_without_them() {
        let db = db();
        {
            let conn = db.lock();
            // Flip sync_enabled directly (bypassing settings_service's
            // validation, which requires credentials) to exercise the
            // "enabled but misconfigured" path defensively.
            conn.execute("UPDATE settings SET sync_enabled = 1 WHERE id = 1", []).unwrap();
        }
        let report = run_once(&db).await.unwrap();
        assert!(report.ran);
        assert!(report.error.is_some());
    }
}
