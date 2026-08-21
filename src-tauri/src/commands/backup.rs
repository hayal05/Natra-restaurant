//! Tauri commands for the daily Excel backup.
//!
//! Thin wrappers only, same convention as every other commands module:
//! acquire what's needed, delegate to `backup`/`services::settings_service`,
//! map `AppError` -> `String` at the boundary (handled automatically
//! since `AppError` implements `Serialize`).

use tauri::{AppHandle, State};

use crate::backup;
use crate::database::Db;
use crate::errors::AppResult;
use crate::models::Settings;
use crate::services::settings_service;

/// Current backup configuration and last-run state, for the Settings
/// page (enabled?, where do files go, when did it last run/fail).
#[tauri::command]
pub fn backup_status(app: AppHandle, db: State<Db>) -> AppResult<backup::BackupStatus> {
    backup::manager::status(&app, db.inner())
}

/// Powers a "Backup now" button. Ignores the daily cadence — pressing
/// the button always writes a fresh file (if backups are enabled),
/// same way a manual "Sync now" always runs regardless of the
/// background loop's timer.
#[tauri::command]
pub fn backup_now(app: AppHandle, db: State<Db>) -> AppResult<backup::BackupReport> {
    backup::force_now(&app, db.inner())
}

#[tauri::command]
pub fn set_backup_enabled(db: State<Db>, enabled: bool) -> AppResult<Settings> {
    let conn = db.lock();
    settings_service::set_backup_enabled(&conn, enabled)
}
