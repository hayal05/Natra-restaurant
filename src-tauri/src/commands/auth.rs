//! Tauri commands for authentication & first-run setup.
//!
//! Credential verification delegates through the native session owner so
//! successful/failed logins revoke old sync leases atomically. Errors serialize
//! through AppError; password hashes remain omitted from User responses.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::User;
use crate::services::auth_service;
use crate::sync::cloud_bridge::CloudBridge;

/// Whether first-run setup (creating the admin account) has happened yet.
/// The frontend calls this on launch to decide which screen to show.
#[tauri::command]
pub fn is_initialized(db: State<Db>) -> AppResult<bool> {
    let conn = db.lock();
    auth_service::is_initialized(&conn)
}

/// First-launch setup: create the sole administrator account.
#[tauri::command]
pub fn initialize_admin(
    db: State<Db>,
    bridge: State<CloudBridge>,
    username: String,
    password: String,
    full_name: String,
) -> AppResult<User> {
    let mut conn = db.lock();
    bridge.initialize_admin(&mut conn, &username, &password, &full_name)
}

/// Credential-based login.
#[tauri::command]
pub fn login(db: State<Db>, bridge: State<CloudBridge>, username: String, password: String) -> AppResult<User> {
    let conn = db.lock();
    bridge.login(&conn, &username, &password)
}

/// Revoke native sync authority and release any in-flight exchange lease.
#[tauri::command]
pub fn logout(bridge: State<CloudBridge>) -> AppResult<()> { bridge.logout() }
