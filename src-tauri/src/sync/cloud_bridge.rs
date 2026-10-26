//! Local manual-exchange lease and IPC adapter. No network or credential store.
use std::sync::Mutex;
use std::time::{Duration, Instant};
use rusqlite::Connection;
use serde::Serialize;
use crate::errors::{AppError, AppResult};
use super::{cloud_dispatch, manager::SyncGuard};

// Deliberately closed in this release. Approval plus production configuration
// and role/credential review are required before changing this gate.
pub(crate) const LIVE_NEON_SYNC_ENABLED: bool = false;

#[derive(Default)]
pub(crate) struct CloudBridge { active: Mutex<Option<Active>> }
struct Active { id: String, expires: Instant, submitted: Vec<String>, cutoff: i64, _guard: SyncGuard }
#[derive(Serialize)]
pub(crate) struct Lease { exchange_id: String, installation_id: String }
fn invalid() -> AppError { AppError::Sync("manual cloud exchange is unavailable or expired".into()) }
impl CloudBridge {
    fn state(&self) -> AppResult<std::sync::MutexGuard<'_, Option<Active>>> {
        let mut state = self.active.lock().map_err(|_| invalid())?;
        if state.as_ref().map(|s| Instant::now() >= s.expires).unwrap_or(false) {
            state.take();
        }
        Ok(state)
    }
    pub(crate) fn begin(&self, conn: &Connection) -> AppResult<Lease> {
        let mut state = self.state()?;
        if state.is_some() { return Err(invalid()); }
        let guard = SyncGuard::acquire().ok_or_else(invalid)?;
        let id: String = conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
        let installation_id = conn.query_row("SELECT value FROM sync_meta WHERE key='device_id'", [], |r| r.get(0))?;
        let cutoff = conn.query_row("SELECT COALESCE(MAX(rowid),0) FROM cloud_outbox", [], |r| r.get(0))?;
        *state = Some(Active { id: id.clone(), expires: Instant::now()+Duration::from_secs(900), submitted: vec![], cutoff, _guard: guard });
        Ok(Lease { exchange_id: id, installation_id })
    }
    pub(crate) fn pending(&self, id: &str, conn: &Connection) -> AppResult<Vec<cloud_dispatch::PendingUpload>> {
        let mut state = self.state()?;
        let active = state.as_mut().filter(|s| s.id == id).ok_or_else(invalid)?;
        let events = cloud_dispatch::read_pending_uploads_through(conn, 500, Some(active.cutoff))?;
        active.submitted = events.iter().map(|e| e.event_id.clone()).collect();
        Ok(events)
    }
    pub(crate) fn acknowledge(&self, id: &str, conn: &mut Connection, ids: &[String]) -> AppResult<()> {
        let mut state = self.state()?;
        let active = state.as_mut().filter(|s| s.id == id).ok_or_else(invalid)?;
        if ids.is_empty() || ids.len() > active.submitted.len() || ids != &active.submitted[..ids.len()] {
            return Err(AppError::Validation("upload receipt does not match staged snapshot".into()));
        }
        cloud_dispatch::acknowledge_uploads(conn, ids)?;
        active.submitted.clear();
        Ok(())
    }
    pub(crate) fn cursor(&self, id: &str, conn: &Connection) -> AppResult<String> {
        let state = self.state()?;
        state.as_ref().filter(|s| s.id == id).ok_or_else(invalid)?;
        Ok(crate::database::connection::read_cloud_pull_cursor(conn)?.to_string())
    }
    pub(crate) fn apply(&self, id: &str, conn: &mut Connection, after: &str, events: &[cloud_dispatch::SequencedDownload]) -> AppResult<usize> {
        let state = self.state()?;
        state.as_ref().filter(|s| s.id == id).ok_or_else(invalid)?;
        if events.is_empty() || events.len() > 500 ||
            crate::database::connection::read_cloud_pull_cursor(conn)?.to_string() != after {
            return Err(AppError::Validation("stale or invalid cloud page".into()));
        }
        cloud_dispatch::apply_download_page(conn, events)
    }
    pub(crate) fn end(&self, id: &str) -> AppResult<()> {
        let mut state = self.state()?;
        state.as_ref().filter(|s| s.id == id).ok_or_else(invalid)?;
        state.take(); // Drop releases the shared Turso/Neon process guard.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_release_and_exact_local_exchange_callbacks() {
        let _serial = super::super::manager::SYNC_TEST_LOCK.lock().unwrap();
        assert!(!LIVE_NEON_SYNC_ENABLED);
        let mut conn = Connection::open_in_memory().unwrap();
        crate::database::migrations::run(&conn).unwrap();
        let event_id = {
            let tx = conn.transaction().unwrap();
            let id = crate::sync::queue::enqueue_cloud_event(&tx,"categories","local:bridge","upsert",
                &serde_json::json!({"name":"Bridge"})).unwrap();
            tx.commit().unwrap(); id
        };
        let bridge = CloudBridge::default();
        let lease = bridge.begin(&conn).unwrap();
        assert!(bridge.begin(&conn).is_err());
        {
            let tx = conn.transaction().unwrap();
            crate::sync::queue::enqueue_cloud_event(&tx,"categories","local:newer","upsert",
                &serde_json::json!({"name":"Newer change"})).unwrap();
            tx.commit().unwrap();
        }
        assert!(bridge.pending("wrong-lease",&conn).is_err());
        assert_eq!(bridge.pending(&lease.exchange_id,&conn).unwrap().len(),1);
        assert!(bridge.acknowledge(&lease.exchange_id,&mut conn,&["unknown".into()]).is_err());
        bridge.acknowledge(&lease.exchange_id,&mut conn,&[event_id]).unwrap();
        assert_eq!(bridge.cursor(&lease.exchange_id,&conn).unwrap(),"0");
        let page: cloud_dispatch::SequencedDownload = serde_json::from_value(serde_json::json!({
            "event_id":"remote-bridge-page","sequence_id":"9007199254740993",
            "entity_type":"categories","entity_id":"remote:bridge",
            "operation":"upsert","payload":{"name":"Remote bridge"}
        })).unwrap();
        assert!(bridge.apply(&lease.exchange_id,&mut conn,"1",&[]).is_err());
        bridge.apply(&lease.exchange_id,&mut conn,"0",&[page]).unwrap();
        assert_eq!(bridge.cursor(&lease.exchange_id,&conn).unwrap(),"9007199254740993");
        bridge.end(&lease.exchange_id).unwrap();
        assert!(bridge.cursor(&lease.exchange_id,&conn).is_err());
        let abandoned = bridge.begin(&conn).unwrap();
        bridge.active.lock().unwrap().as_mut().unwrap().expires = Instant::now();
        assert!(bridge.cursor(&abandoned.exchange_id,&conn).is_err());
        let recovered = bridge.begin(&conn).unwrap();
        bridge.end(&recovered.exchange_id).unwrap();
    }
}
