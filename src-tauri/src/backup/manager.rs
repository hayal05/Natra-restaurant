//! Orchestrates the daily local Excel backup and the periodic
//! background loop that drives it automatically when enabled.
//!
//! Independent of `sync` — this writes a plain .xlsx file to disk
//! (`app_data_dir/backups/`), so recovering a restaurant's data never
//! depends on a network connection, a Turso account, or this app even
//! being reinstalled correctly. `settings.backup_enabled` gates it,
//! same pattern as `sync_enabled`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{NaiveDateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::database::Db;
use crate::errors::AppResult;
use crate::services::settings_service;

use super::export;

/// The background loop wakes up this often to check whether a backup
/// is due. Deliberately much shorter than the 24h cadence itself —
/// this just needs to catch "a day has passed" reasonably promptly,
/// including right after the app is opened following an idle stretch.
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// How many backup files to keep on disk. At one a day this is a
/// month of history — enough to recover from "we noticed this a
/// couple weeks late" without the folder growing forever.
const RETAIN_COUNT: usize = 30;

const SQLITE_DATETIME_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// Outcome of one backup check, returned to the frontend by the
/// `backup_now` command so a "Backup now" button can show something
/// more useful than a spinner.
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub ran: bool,
    pub path: Option<String>,
    pub pruned: usize,
    pub error: Option<String>,
}

impl BackupReport {
    fn disabled() -> Self {
        BackupReport { ran: false, path: None, pruned: 0, error: None }
    }

    fn skipped_not_due() -> Self {
        BackupReport { ran: false, path: None, pruned: 0, error: None }
    }

    fn failed(err: &crate::errors::AppError) -> Self {
        BackupReport { ran: true, path: None, pruned: 0, error: Some(err.to_string()) }
    }
}

/// Current backup configuration/state, for the Settings page.
#[derive(Debug, Clone, Serialize)]
pub struct BackupStatus {
    pub enabled: bool,
    pub backup_dir: String,
    pub last_backup_at: Option<String>,
    pub last_backup_path: Option<String>,
    pub last_backup_error: Option<String>,
}

/// Start the background loop as a fire-and-forget task on Tauri's
/// async runtime. Called once from `lib.rs`'s `setup()`; the loop
/// checks `settings.backup_enabled` on every tick, so toggling it from
/// the Settings page takes effect on the next tick without needing to
/// restart the loop itself.
pub fn start_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(CHECK_INTERVAL);
        // First tick fires immediately; skip it so we don't force a
        // backup at every app launch — `run_now` still covers "it's
        // been more than a day" on the very first tick after that.
        interval.tick().await;
        loop {
            interval.tick().await;
            let db = app.state::<Db>();
            match run_now(&app, &db) {
                Ok(report) if report.ran => {
                    if let Some(err) = &report.error {
                        eprintln!("[backup] failed: {err}");
                    } else if let Some(path) = &report.path {
                        eprintln!("[backup] wrote {path} (pruned {})", report.pruned);
                    }
                }
                Ok(_) => {} // disabled or not due yet — nothing to log every hour
                Err(e) => eprintln!("[backup] could not read settings: {e}"),
            }
        }
    });
}

/// Resolve (and ensure) the on-disk backups folder for this app.
pub fn backup_dir_for_app(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::errors::AppError::Internal(format!("could not resolve app data dir: {e}")))?
        .join("backups");
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::errors::AppError::Internal(format!("could not create backups dir: {e}")))?;
    Ok(dir)
}

/// Check whether a backup is due and write one if so. Used by both the
/// background loop and the manual `backup_now` command — a manual
/// backup skips the "is it due yet" check so the button always does
/// something when pressed.
pub fn run_now(app: &AppHandle, db: &Db) -> AppResult<BackupReport> {
    let dir = backup_dir_for_app(app)?;
    run_once(db, &dir, false)
}

pub fn force_now(app: &AppHandle, db: &Db) -> AppResult<BackupReport> {
    let dir = backup_dir_for_app(app)?;
    run_once(db, &dir, true)
}

/// Core logic, independent of Tauri — takes a plain directory so it's
/// easy to unit test with a tempdir.
fn run_once(db: &Db, backup_dir: &Path, force: bool) -> AppResult<BackupReport> {
    let settings = {
        let conn = db.lock();
        settings_service::get_settings(&conn)?
    };

    if !settings.backup_enabled {
        return Ok(BackupReport::disabled());
    }

    if !force {
        let last = read_meta(db, "last_backup_at")?;
        if let Some(last_at) = last {
            if let Ok(parsed) = NaiveDateTime::parse_from_str(&last_at, SQLITE_DATETIME_FMT) {
                let elapsed = Utc::now().naive_utc() - parsed;
                if elapsed < chrono::Duration::hours(24) {
                    return Ok(BackupReport::skipped_not_due());
                }
            }
        }
    }

    match write_backup(db, backup_dir) {
        Ok((path, pruned)) => {
            write_meta(db, "last_backup_at", Some(&now_str()))?;
            write_meta(db, "last_backup_path", Some(&path))?;
            write_meta(db, "last_backup_error", None)?;
            Ok(BackupReport { ran: true, path: Some(path), pruned, error: None })
        }
        Err(e) => {
            write_meta(db, "last_backup_error", Some(&e.to_string()))?;
            Ok(BackupReport::failed(&e))
        }
    }
}

fn write_backup(db: &Db, backup_dir: &Path) -> AppResult<(String, usize)> {
    let bytes = {
        let conn = db.lock();
        export::build_workbook(&conn)?
    };

    let filename = format!("natra-backup-{}.xlsx", Utc::now().format("%Y-%m-%d_%H%M%S"));
    let path = backup_dir.join(&filename);
    std::fs::write(&path, bytes)
        .map_err(|e| crate::errors::AppError::Internal(format!("could not write backup file: {e}")))?;

    let pruned = prune_old_backups(backup_dir)?;

    Ok((path.to_string_lossy().into_owned(), pruned))
}

/// Keep only the most recent `RETAIN_COUNT` backup files. Filenames
/// are zero-padded/date-prefixed so lexical sort order matches
/// chronological order.
fn prune_old_backups(backup_dir: &Path) -> AppResult<usize> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(backup_dir)
        .map_err(|e| crate::errors::AppError::Internal(format!("could not list backups dir: {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xlsx"))
        .collect();

    files.sort();

    let mut pruned = 0;
    if files.len() > RETAIN_COUNT {
        for old in &files[..files.len() - RETAIN_COUNT] {
            if std::fs::remove_file(old).is_ok() {
                pruned += 1;
            }
        }
    }
    Ok(pruned)
}

fn now_str() -> String {
    Utc::now().format(SQLITE_DATETIME_FMT).to_string()
}

fn read_meta(db: &Db, key: &str) -> AppResult<Option<String>> {
    let conn = db.lock();
    Ok(conn.query_row("SELECT value FROM backup_meta WHERE key = ?1", [key], |row| row.get(0))?)
}

fn write_meta(db: &Db, key: &str, value: Option<&str>) -> AppResult<()> {
    let conn = db.lock();
    conn.execute(
        "UPDATE backup_meta SET value = ?1 WHERE key = ?2",
        rusqlite::params![value, key],
    )?;
    Ok(())
}

/// Full status for the Settings page: is it on, where do files go,
/// and what happened last time.
pub fn status(app: &AppHandle, db: &Db) -> AppResult<BackupStatus> {
    let settings = {
        let conn = db.lock();
        settings_service::get_settings(&conn)?
    };
    let dir = backup_dir_for_app(app)?;

    Ok(BackupStatus {
        enabled: settings.backup_enabled,
        backup_dir: dir.to_string_lossy().into_owned(),
        last_backup_at: read_meta(db, "last_backup_at")?,
        last_backup_path: read_meta(db, "last_backup_path")?,
        last_backup_error: read_meta(db, "last_backup_error")?,
    })
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
    fn run_once_writes_a_file_when_never_backed_up() {
        let db = db();
        let dir = tempfile_dir();
        let report = run_once(&db, &dir, false).unwrap();
        assert!(report.ran);
        assert!(report.error.is_none());
        let path = report.path.unwrap();
        assert!(Path::new(&path).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_once_is_a_no_op_when_backup_disabled() {
        let db = db();
        {
            let conn = db.lock();
            conn.execute("UPDATE settings SET backup_enabled = 0 WHERE id = 1", []).unwrap();
        }
        let dir = tempfile_dir();
        let report = run_once(&db, &dir, false).unwrap();
        assert!(!report.ran);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_once_skips_when_already_backed_up_today() {
        let db = db();
        let dir = tempfile_dir();

        let first = run_once(&db, &dir, false).unwrap();
        assert!(first.ran);

        let second = run_once(&db, &dir, false).unwrap();
        assert!(!second.ran, "a second backup within 24h should be skipped");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn force_ignores_the_daily_cadence() {
        let db = db();
        let dir = tempfile_dir();

        run_once(&db, &dir, false).unwrap();
        let forced = run_once(&db, &dir, true).unwrap();
        assert!(forced.ran, "force=true must write even if one already ran today");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_keeps_only_the_most_recent_files() {
        let dir = tempfile_dir();
        for i in 0..(RETAIN_COUNT + 5) {
            std::fs::write(dir.join(format!("natra-backup-2026-01-{:02}_000000.xlsx", i + 1)), b"x").unwrap();
        }
        let pruned = prune_old_backups(&dir).unwrap();
        assert_eq!(pruned, 5);
        let remaining = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(remaining, RETAIN_COUNT);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("natra-backup-test-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Good enough uniqueness for a test tempdir name without pulling
    /// in a uuid crate just for this.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        format!(
            "{}-{:?}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
            std::thread::current().id()
        )
    }
}
