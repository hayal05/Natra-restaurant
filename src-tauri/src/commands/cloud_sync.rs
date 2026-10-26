//! Closed-by-default IPC wiring for manual Neon synchronization.
use tauri::State;
use crate::database::Db;
use crate::errors::{AppError, AppResult};
use crate::sync::cloud_bridge::{CloudBridge, Lease, LIVE_NEON_SYNC_ENABLED};
use crate::sync::cloud_dispatch::{PendingUpload, SequencedDownload};
fn enabled() -> AppResult<()> {
    if !LIVE_NEON_SYNC_ENABLED { return Err(AppError::Sync("cloud synchronization is not configured".into())); }
    Ok(())
}

// Configuration/short-lived credentials must be supplied by an approved
// provisioner, not the old Turso settings or frontend localStorage. None is
// intentional for this unactivated release; no credential setter is exposed.
#[tauri::command]
pub fn cloud_sync_configuration(db: State<Db>, bridge: State<CloudBridge>) -> Option<serde_json::Value> {
    if !LIVE_NEON_SYNC_ENABLED { return None; }
    let conn = db.lock();
    bridge.with_authorization(&conn, || {
        // An approved host provisioner supplies a short-lived session token.
        // No setters, persistent token fields or plaintext config files are added.
        let origin = std::env::var("NATRA_NEON_SYNC_ORIGIN").ok()?;
        let token = std::env::var("NATRA_NEON_SESSION_TOKEN").ok()?;
        if token.is_empty() || token.len() > 8192 || !token.bytes().all(|b| b.is_ascii_graphic()) { return None; }
        let origin = origin.trim_end_matches('/');
        // Full URL/path/origin validation happens before the controller begins.
        Some(serde_json::json!({
            "uploadEndpoint":format!("{origin}/sync/upload"),
            "pullEndpoint":format!("{origin}/sync/pull"),
            "allowanceEndpoint":format!("{origin}/sync/allowance"),
            "completeEndpoint":format!("{origin}/sync/complete"),
            "failEndpoint":format!("{origin}/sync/fail"), "token":token
        }))
    }).ok().flatten()
}
#[tauri::command]
pub fn cloud_sync_begin(db: State<Db>, bridge: State<CloudBridge>) -> AppResult<Lease> {
    enabled()?; let conn=db.lock(); bridge.begin(&conn)
}
#[tauri::command]
pub fn cloud_sync_pending(db: State<Db>, bridge: State<CloudBridge>, exchange_id: String) -> AppResult<Vec<PendingUpload>> {
    enabled()?; let conn=db.lock(); bridge.pending(&exchange_id,&conn)
}
#[tauri::command]
pub fn cloud_sync_acknowledge(db: State<Db>, bridge: State<CloudBridge>, exchange_id: String, event_ids: Vec<String>) -> AppResult<()> {
    enabled()?; let mut conn=db.lock(); bridge.acknowledge(&exchange_id,&mut conn,&event_ids)
}
#[tauri::command]
pub fn cloud_sync_cursor(db: State<Db>, bridge: State<CloudBridge>, exchange_id: String) -> AppResult<String> {
    enabled()?; let conn=db.lock(); bridge.cursor(&exchange_id,&conn)
}
#[tauri::command]
pub fn cloud_sync_apply(db: State<Db>, bridge: State<CloudBridge>, exchange_id: String, after: String, events: Vec<SequencedDownload>) -> AppResult<usize> {
    enabled()?; let mut conn=db.lock(); bridge.apply(&exchange_id,&mut conn,&after,&events)
}
#[tauri::command]
pub fn cloud_sync_end(db: State<Db>, bridge: State<CloudBridge>, exchange_id: String) -> AppResult<()> {
    enabled()?; let conn=db.lock(); bridge.end(&exchange_id,&conn)
}
