//! Tauri commands for application settings and sync configuration.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::Settings;
use crate::services::settings_service;
use crate::sync::queue::QueueCounts;
use crate::sync::{self, SyncReport};

#[tauri::command]
pub fn get_settings(db: State<Db>) -> AppResult<Settings> {
    let conn = db.lock();
    settings_service::get_settings(&conn)
}

#[tauri::command]
pub fn update_general_settings(
    db: State<Db>,
    restaurant_name: String,
    currency: String,
) -> AppResult<Settings> {
    let conn = db.lock();
    settings_service::update_general(&conn, &restaurant_name, &currency)
}

#[tauri::command]
pub fn enable_sync(db: State<Db>, turso_url: String, turso_auth_token: String) -> AppResult<Settings> {
    let conn = db.lock();
    settings_service::enable_sync(&conn, &turso_url, &turso_auth_token)
}

#[tauri::command]
pub fn disable_sync(db: State<Db>) -> AppResult<Settings> {
    let conn = db.lock();
    settings_service::disable_sync(&conn)
}

/// Trigger a push+pull cycle right now, instead of waiting for the
/// background loop's next tick. Powers a "Sync now" button on the
/// Settings page; returns what the cycle actually did so the UI can
/// show something more useful than "done".
#[tauri::command]
pub async fn sync_now(db: State<'_, Db>) -> AppResult<SyncReport> {
    sync::run_once(db.inner()).await
}

/// Outbox backlog counts (pending / synced / failed), so the Settings
/// page can show whether sync is caught up or something needs
/// attention — without exposing raw queue rows to the frontend.
#[tauri::command]
pub fn sync_queue_status(db: State<Db>) -> AppResult<QueueCounts> {
    let conn = db.lock();
    crate::sync::queue::counts(&conn)
}
