//! Tauri commands for the daily Excel backup.
//!
//! Thin wrappers only, same convention as every other commands module:
//! acquire what's needed, delegate to `backup`/`services::settings_service`,
//! map `AppError` -> `String` at the boundary (handled automatically
//! since `AppError` implements `Serialize`).

use tauri::{AppHandle, State};

use crate::backup;
use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::models::Settings;
use crate::services::settings_service;

/// Current backup configuration and last-run state, for the Settings
/// page (enabled?, where do files go, when did it last run/fail).
#[tauri::command]
pub fn backup_status(app: AppHandle, db: State<Db>, bridge: State<CloudBridge>) -> AppResult<backup::BackupStatus> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    backup::manager::status_with_connection(&app, &conn)
}

/// Powers a "Backup now" button. Ignores the daily cadence — pressing
/// the button always writes a fresh file (if backups are enabled),
/// same way a manual "Sync now" always runs regardless of the
/// background loop's timer.
#[tauri::command]
pub fn backup_now(app: AppHandle, db: State<Db>, bridge: State<CloudBridge>) -> AppResult<backup::BackupReport> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    backup::manager::run_once_with_connection(&conn, &backup::manager::backup_dir_for_app(&app)?, true)
}

#[tauri::command]
pub fn set_backup_enabled(db: State<Db>, bridge: State<CloudBridge>, enabled: bool) -> AppResult<Settings> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    settings_service::set_backup_enabled(&conn, enabled)
}

/// Offline manual range export; business dates interpreted in EAT.
#[tauri::command]
pub fn export_transaction_archive(app: AppHandle, db: State<Db>, bridge: State<CloudBridge>, from: String, through: String) -> AppResult<backup::archive_manager::ArchiveResult> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    let dir = backup::archive_manager::archive_dir(&app)?;
    backup::archive_manager::export_range_with_connection(&conn, &dir, &from, &through)
}

#[derive(serde::Serialize)]
pub struct ArchiveListing {
    pub directory: String,
    pub automatic_daily: bool,
    pub entries: Vec<backup::archive_manager::ArchiveEntry>,
}

#[tauri::command]
pub fn list_transaction_archives(app: AppHandle, db: State<Db>, bridge: State<CloudBridge>) -> AppResult<ArchiveListing> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    let dir = backup::archive_manager::archive_dir(&app)?;
    let entries = backup::archive_manager::list_archives(&dir)?;
    Ok(ArchiveListing {
        directory: dir.to_string_lossy().into_owned(),
        automatic_daily: true,
        entries,
    })
}

#[tauri::command]
pub fn download_transaction_archive(
    app: AppHandle, db: State<Db>, bridge: State<CloudBridge>,
    filename: String,
) -> AppResult<backup::archive_manager::DownloadedArchive> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    use tauri::Manager;
    let archives = backup::archive_manager::archive_dir(&app)?;
    let downloads = app.path().download_dir()
        .map_err(|e| crate::errors::AppError::Internal(format!("Downloads directory unavailable: {e}")))?;
    backup::archive_manager::copy_to_downloads(&archives, &downloads, &filename)
}

#[tauri::command]
pub fn download_transaction_archive_pdf(app: AppHandle, db: State<Db>, bridge: State<CloudBridge>, filename: String) -> AppResult<String> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    use tauri::Manager;
    let archives = backup::archive_manager::archive_dir(&app)?;
    let downloads = app.path().download_dir()
        .map_err(|e| crate::errors::AppError::Internal(format!("Downloads unavailable: {e}")))?;
    backup::archive_pdf::save_pdf_to_downloads(&archives, &downloads, &filename)
}
