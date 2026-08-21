//! Tauri commands for authentication & first-run setup.
//!
//! Thin wrappers only: acquire the DB lock, delegate to
//! `services::auth_service`, map `AppError` -> `String` at the boundary
//! (handled automatically since `AppError` implements `Serialize`).

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::User;
use crate::services::auth_service;

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
    username: String,
    password: String,
    full_name: String,
) -> AppResult<User> {
    let mut conn = db.lock();
    auth_service::initialize_admin(&mut conn, &username, &password, &full_name)
}

/// Credential-based login.
#[tauri::command]
pub fn login(db: State<Db>, username: String, password: String) -> AppResult<User> {
    let conn = db.lock();
    auth_service::login(&conn, &username, &password)
}
