//! Tauri commands for application settings and sync configuration.

use tauri::State;

use crate::database::Db;
use crate::sync::cloud_bridge::{CloudBridge, BusinessPermission};
use crate::errors::AppResult;
use crate::models::Settings;
use crate::services::settings_service;
use crate::sync::queue::QueueCounts;
use crate::sync::SyncReport;

#[tauri::command]
pub fn get_settings(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<Settings> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Pos)?;
    settings_service::get_settings(&conn)
}

/// Read the installation identity created at database startup.
/// Reading the existing identity requires an active administrator session.
#[tauri::command]
pub fn get_installation_id(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<String> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    Ok(conn.query_row(
        "SELECT value FROM sync_meta WHERE key = 'device_id'",
        [],
        |row| row.get(0),
    )?)
}

#[tauri::command]
pub fn update_general_settings(
    db: State<Db>, bridge: State<CloudBridge>,
    restaurant_name: String,
    currency: String,
) -> AppResult<Settings> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    settings_service::update_general(&conn, &restaurant_name, &currency)
}

#[tauri::command]
pub fn enable_sync(db: State<Db>, bridge: State<CloudBridge>, turso_url: String, turso_auth_token: String) -> AppResult<Settings> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    let _ = (turso_url, turso_auth_token);
    Err(crate::errors::AppError::Sync("legacy synchronization cannot be enabled".into()))
}

#[tauri::command]
pub fn disable_sync(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<Settings> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    settings_service::disable_sync(&conn)
}

/// Retired direct-Turso IPC entry point. New sync uses authorized Neon leases.
#[tauri::command]
pub fn sync_now(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<SyncReport> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    // The retired direct-Turso path has no session-bound async callbacks.
    // Manual Neon exchanges use the separate closed, authorized lease API.
    Err(crate::errors::AppError::Sync("legacy synchronization is unavailable; use the approved manual exchange".into()))
}

/// Outbox backlog counts (pending / synced / failed), so the Settings
/// page can show whether sync is caught up or something needs
/// attention — without exposing raw queue rows to the frontend.
#[tauri::command]
pub fn sync_queue_status(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<QueueCounts> {
    let conn = db.lock();
    let _authorization = bridge.authorize_business(&conn, BusinessPermission::Administrator)?;
    crate::sync::queue::counts(&conn)
}
