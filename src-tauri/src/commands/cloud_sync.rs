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

// Availability is local-only and includes no credential/token. Fresh session
// delivery is a separate explicit command after the native lease is acquired.
#[tauri::command]
pub fn cloud_sync_configuration(db: State<Db>, bridge: State<CloudBridge>) -> Option<serde_json::Value> {
    if !LIVE_NEON_SYNC_ENABLED { return None; }
    bridge.configuration(&db.lock()).ok()
}
#[tauri::command]
pub async fn cloud_sync_credentials(db: State<'_,Db>, bridge: State<'_,CloudBridge>, exchange_id: String) -> AppResult<serde_json::Value> {
    enabled()?;
    let context={let conn=db.lock();bridge.prepare_delivery(&conn,&exchange_id)?};
    let identity=context.identity.clone();
    // No SQLite or session lock crosses blocking OS/network work.
    let delivered=tokio::task::spawn_blocking(move||crate::sync::native_credentials::fetch(&identity))
        .await.map_err(|_|crate::sync::native_credentials::unavailable())??;
    let conn=db.lock();
    bridge.commit_delivery(&conn,&context,||{delivered.ensure_current(&context.identity)?;delivered.configuration()})
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
