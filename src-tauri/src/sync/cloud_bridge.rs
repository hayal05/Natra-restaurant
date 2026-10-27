//! Native login ownership and local manual-exchange leases. No network store.
use std::sync::Mutex;
use std::time::{Duration, Instant};
use rusqlite::Connection;
use serde::Serialize;
use crate::errors::{AppError, AppResult};
use crate::models::{User, UserRole};
use crate::services::auth_service;
use super::{cloud_dispatch, native_credentials::{self,Identity}, manager::SyncGuard};

// Remains closed pending secure provisioning and the other release gates.
pub(crate) const LIVE_NEON_SYNC_ENABLED: bool = false;
const NATIVE_SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);

#[derive(Default)]
pub(crate) struct CloudBridge { state: Mutex<BridgeState> }
#[derive(Default)]
struct BridgeState { principal: Option<Principal>, active: Option<Active> }
struct Principal { role: UserRole, user_id: i64, password_hash: String, generation: String, expires: Instant }
struct Active { id: String, owner: String, installation_id: String, expires: Instant, submitted: Vec<String>, cutoff: i64, delivery_started: bool, delivered: bool, _guard: SyncGuard }
pub(crate) struct DeliveryContext { pub identity: Identity, exchange_id: String, owner: String }
#[derive(Serialize)]
pub(crate) struct Lease { exchange_id: String, installation_id: String }
fn invalid() -> AppError { AppError::Sync("manual cloud exchange is unavailable or expired".into()) }
fn unauthorized() -> AppError { AppError::Auth("an active administrator login is required for cloud synchronization".into()) }
fn random_id(conn: &Connection) -> AppResult<String> {
    Ok(conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?)
}
impl BridgeState {
    fn revoke(&mut self) { self.active.take(); self.principal.take(); }
    fn establish(&mut self, conn: &Connection, user: &User) -> AppResult<()> {
        self.principal = Some(Principal { role: user.role, user_id: user.id, password_hash: user.password_hash.clone(),
            generation: random_id(conn)?, expires: Instant::now() + NATIVE_SESSION_TTL });
        Ok(())
    }
    fn authenticated_user(&mut self, conn: &Connection) -> AppResult<User> {
        let result = (|| {
            let error = || AppError::Auth("your local session has ended; please sign in again".into());
            let principal = self.principal.as_ref().ok_or_else(error)?;
            if Instant::now() >= principal.expires { return Err(error()); }
            let user = conn.query_row("SELECT * FROM users WHERE id=?1", [principal.user_id], User::from_row)
                .map_err(|_| error())?;
            if !user.is_active || user.role != principal.role || user.password_hash != principal.password_hash {
                return Err(error());
            }
            Ok(user)
        })();
        if result.is_err() { self.revoke(); }
        result
    }
    fn authorize(&mut self, conn: &Connection) -> AppResult<String> {
        if self.authenticated_user(conn)?.role != UserRole::Admin { return Err(unauthorized()); }
        Ok(self.principal.as_ref().ok_or_else(unauthorized)?.generation.clone())
    }
    fn owned(&mut self, conn: &Connection, id: &str) -> AppResult<&mut Active> {
        let owner = self.authorize(conn)?;
        let installation: String=conn.query_row("SELECT value FROM sync_meta WHERE key='device_id'",[],|r|r.get(0))?;
        if self.active.as_ref().map(|a|a.installation_id!=installation).unwrap_or(false) {
            self.active.take();return Err(invalid());
        }
        self.active.as_mut().filter(|s| s.id == id && s.owner == owner).ok_or_else(invalid)
    }
}
/// Permission policy for local business IPC. Neither role nor user ID comes
/// from JavaScript. Both locks must remain held until the command completes.
#[derive(Clone, Copy)]
pub(crate) enum BusinessPermission { Pos, Administrator }
pub(crate) struct BusinessAuthorization<'a> {
    _state: std::sync::MutexGuard<'a, BridgeState>,
    pub user_id: i64,
}
impl CloudBridge {
    fn state(&self) -> AppResult<std::sync::MutexGuard<'_, BridgeState>> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                // A panic revokes both authentication and uncertain work.
                state.revoke();
                self.state.clear_poison();
                eprintln!("[sync] revoked native session and abandoned exchange after a panic");
                state
            }
        };
        if state.principal.as_ref().map(|s| Instant::now() >= s.expires).unwrap_or(false) {
            state.revoke();
        }
        if state.active.as_ref().map(|s| Instant::now() >= s.expires).unwrap_or(false) {
            state.active.take();
        }
        Ok(state)
    }
    /// Caller holds SQLite first, then this session guard. Never await or
    /// reacquire SQLite while holding the guard. Logout cannot finish while
    /// an authorized local command is still executing.
    pub(crate) fn authorize_business(&self, conn: &Connection, permission: BusinessPermission)
        -> AppResult<BusinessAuthorization<'_>> {
        let mut state = self.state()?;
        let user = state.authenticated_user(conn)?;
        if matches!(permission, BusinessPermission::Administrator) && user.role != UserRole::Admin {
            return Err(AppError::Forbidden("administrator access is required".into()));
        }
        Ok(BusinessAuthorization { _state: state, user_id: user.id })
    }
    pub(crate) fn login(&self, conn: &Connection, username: &str, password: &str) -> AppResult<User> {
        let mut state = self.state()?;
        state.revoke(); // Even failed reauthentication cannot retain old authority.
        let user = auth_service::login(conn, username, password)?;
        state.establish(conn, &user)?;
        Ok(user)
    }
    pub(crate) fn initialize_admin(&self, conn: &mut Connection, username: &str, password: &str, full_name: &str) -> AppResult<User> {
        let mut state = self.state()?;
        state.revoke();
        let user = auth_service::initialize_admin(conn, username, password, full_name)?;
        state.establish(conn, &user)?;
        Ok(user)
    }
    pub(crate) fn logout(&self) -> AppResult<()> { self.state()?.revoke(); Ok(()) }
    pub(crate) fn with_authorization<T>(&self, conn: &Connection, action: impl FnOnce() -> T) -> AppResult<T> {
        let mut state = self.state()?;
        state.authorize(conn)?;
        // Keep authorization locked through credential access, so logout
        // cannot complete between the permission check and the read.
        Ok(action())
    }
    fn identity(state: &mut BridgeState, conn: &Connection) -> AppResult<(Identity,String)> {
        let owner=state.authorize(conn)?;
        let user=state.principal.as_ref().ok_or_else(unauthorized)?.user_id;
        let installation_id: String=conn.query_row("SELECT value FROM sync_meta WHERE key='device_id'",[],|r|r.get(0))?;
        Ok((Identity { installation_id,local_user_id:user.to_string() },owner))
    }
    pub(crate) fn configuration(&self, conn: &Connection) -> AppResult<serde_json::Value> {
        let mut state=self.state()?;
        let (identity,_)=Self::identity(&mut state,conn)?;
        native_credentials::available(&identity)
    }
    pub(crate) fn prepare_delivery(&self, conn: &Connection, id: &str) -> AppResult<DeliveryContext> {
        let mut state=self.state()?;
        let (identity,owner)=Self::identity(&mut state,conn)?;
        let active=state.owned(conn,id)?;
        if active.delivery_started { return Err(invalid()); }
        active.delivery_started=true;
        Ok(DeliveryContext { identity,exchange_id:id.to_string(),owner })
    }
    pub(crate) fn commit_delivery(&self, conn: &Connection, context: &DeliveryContext,
        build: impl FnOnce()->AppResult<serde_json::Value>) -> AppResult<serde_json::Value> {
        let mut state=self.state()?;
        let (identity,owner)=Self::identity(&mut state,conn)?;
        if owner!=context.owner || identity.local_user_id!=context.identity.local_user_id ||
            identity.installation_id!=context.identity.installation_id { return Err(invalid()); }
        let active=state.active.as_mut().filter(|a|a.id==context.exchange_id && a.owner==owner).ok_or_else(invalid)?;
        if !active.delivery_started || active.delivered { return Err(invalid()); }
        let config=build()?;
        active.delivered=true;
        Ok(config)
    }
    pub(crate) fn begin(&self, conn: &Connection) -> AppResult<Lease> {
        let mut state = self.state()?;
        let owner = state.authorize(conn)?;
        if state.active.is_some() { return Err(invalid()); }
        let guard = SyncGuard::acquire().ok_or_else(invalid)?;
        let id = random_id(conn)?;
        let installation_id: String = conn.query_row("SELECT value FROM sync_meta WHERE key='device_id'", [], |r| r.get(0))?;
        let cutoff = conn.query_row("SELECT COALESCE(MAX(rowid),0) FROM cloud_outbox", [], |r| r.get(0))?;
        state.active = Some(Active { id: id.clone(), owner, installation_id:installation_id.clone(), expires: Instant::now()+Duration::from_secs(900), submitted: vec![], cutoff, delivery_started: false, delivered: false, _guard: guard });
        Ok(Lease { exchange_id: id, installation_id })
    }
    pub(crate) fn pending(&self, id: &str, conn: &Connection) -> AppResult<Vec<cloud_dispatch::PendingUpload>> {
        let mut state = self.state()?;
        let active = state.owned(conn, id)?;
        let events = cloud_dispatch::read_pending_uploads_through(conn, 500, Some(active.cutoff))?;
        active.submitted = events.iter().map(|e| e.event_id.clone()).collect();
        Ok(events)
    }
    pub(crate) fn acknowledge(&self, id: &str, conn: &mut Connection, ids: &[String]) -> AppResult<()> {
        let mut state = self.state()?;
        let active = state.owned(conn, id)?;
        if ids.is_empty() || ids.len() > active.submitted.len() || ids != &active.submitted[..ids.len()] {
            return Err(AppError::Validation("upload receipt does not match staged snapshot".into()));
        }
        cloud_dispatch::acknowledge_uploads(conn, ids)?;
        active.submitted.clear();
        Ok(())
    }
    pub(crate) fn cursor(&self, id: &str, conn: &Connection) -> AppResult<String> {
        let mut state = self.state()?;
        state.owned(conn, id)?;
        Ok(crate::database::connection::read_cloud_pull_cursor(conn)?.to_string())
    }
    pub(crate) fn apply(&self, id: &str, conn: &mut Connection, after: &str, events: &[cloud_dispatch::SequencedDownload]) -> AppResult<usize> {
        let mut state = self.state()?;
        state.owned(conn, id)?;
        if events.is_empty() || events.len() > 500 ||
            crate::database::connection::read_cloud_pull_cursor(conn)?.to_string() != after {
            return Err(AppError::Validation("stale or invalid cloud page".into()));
        }
        cloud_dispatch::apply_download_page(conn, events)
    }
    pub(crate) fn end(&self, id: &str, conn: &Connection) -> AppResult<()> {
        let mut state = self.state()?;
        state.owned(conn, id)?;
        state.active.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn business_access_requires_current_login_and_role_without_mutating_on_denial() {
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = CloudBridge::default();
        assert!(bridge.authorize_business(&conn, BusinessPermission::Pos).is_err());
        let admin = bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
        {
            let _guard = bridge.authorize_business(&conn, BusinessPermission::Administrator).unwrap();
            crate::services::inventory_service::create_category(&conn, "Admin writes", None).unwrap();
        }
        conn.execute("INSERT INTO users(username,password_hash,full_name,role) VALUES ('staff',?1,'Cashier','staff')", [&admin.password_hash]).unwrap();
        let staff = bridge.login(&conn, "staff", "test-secret").unwrap();
        let before: i64 = conn.query_row("SELECT count(*) FROM cloud_outbox", [], |r| r.get(0)).unwrap();
        assert!(matches!(bridge.authorize_business(&conn, BusinessPermission::Administrator), Err(AppError::Forbidden(_))));
        // Permission denial must not log out a valid staff POS session.
        assert_eq!(bridge.authorize_business(&conn, BusinessPermission::Pos).unwrap().user_id, staff.id);
        assert_eq!(conn.query_row("SELECT count(*) FROM cloud_outbox", [], |r| r.get::<_,i64>(0)).unwrap(), before);
        bridge.logout().unwrap();
        assert!(bridge.authorize_business(&conn, BusinessPermission::Pos).is_err());
        bridge.login(&conn, "admin", "test-secret").unwrap();
        assert!(bridge.login(&conn, "admin", "wrong-password").is_err());
        assert!(bridge.authorize_business(&conn, BusinessPermission::Administrator).is_err());
        bridge.login(&conn, "admin", "test-secret").unwrap();
        bridge.state.lock().unwrap().principal.as_mut().unwrap().expires = Instant::now();
        assert!(bridge.authorize_business(&conn, BusinessPermission::Pos).is_err());
    }

    #[test]
    fn business_account_changes_require_fresh_credentials_even_after_row_restoration() {
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = CloudBridge::default();
        let admin = bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
        for mutation in ["UPDATE users SET role='staff' WHERE username='admin'",
            "UPDATE users SET is_active=0 WHERE username='admin'",
            "UPDATE users SET password_hash='changed' WHERE username='admin'"] {
            bridge.login(&conn, "admin", "test-secret").unwrap();
            conn.execute_batch(mutation).unwrap();
            assert!(bridge.authorize_business(&conn, BusinessPermission::Pos).is_err());
            conn.execute("UPDATE users SET role='admin',is_active=1,password_hash=?1 WHERE id=?2",
                rusqlite::params![admin.password_hash,admin.id]).unwrap();
            assert!(bridge.authorize_business(&conn, BusinessPermission::Administrator).is_err());
        }
        bridge.login(&conn, "admin", "test-secret").unwrap();
        conn.execute("DELETE FROM users WHERE id=?1", [admin.id]).unwrap();
        assert!(bridge.authorize_business(&conn, BusinessPermission::Pos).is_err());
    }

    #[test]
    fn native_checkout_replaces_forged_cashier_and_preserves_financials() {
        use crate::services::{inventory_service::{self,NewItem}, waiter_service, sales_service::{CheckoutRequest,CartLine}};
        use crate::models::{ItemType,PaymentMethod};
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = CloudBridge::default();
        let admin = bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
        let waiter = waiter_service::create_waiter(&conn,"Waiter",None,None).unwrap();
        let item = inventory_service::create_item(&conn,NewItem {category_id:None,name:"Coffee".into(),item_type:ItemType::ReadyMade,purchase_cost:Some(5.0),selling_price:15.0}).unwrap();
        conn.execute("INSERT INTO users(username,password_hash,full_name,role) VALUES ('staff',?1,'Cashier','staff')", [&admin.password_hash]).unwrap();
        let staff = bridge.login(&conn,"staff","test-secret").unwrap();
        let guard = bridge.authorize_business(&conn,BusinessPermission::Pos).unwrap();
        let result = crate::commands::pos::checkout_authorized(&mut conn,&guard,CheckoutRequest {
            waiter_id:waiter.id,user_id:Some(admin.id),payment_method:PaymentMethod::Cash,
            lines:vec![CartLine {item_id:item.id,quantity:2}],note:None,
        }).unwrap();
        assert_eq!(result.sale.user_id,Some(staff.id));
        assert_eq!((result.sale.total_amount,result.sale.total_cost,result.sale.total_profit),(30.0,10.0,20.0));
        assert!(!result.sale.is_settled);
        assert_eq!(result.items.len(),1);
        let stored: i64 = conn.query_row("SELECT user_id FROM sales WHERE id=?1",[result.sale.id],|r|r.get(0)).unwrap();
        assert_eq!(stored,staff.id);
    }

    #[test]
    fn business_command_finishes_before_logout_can_complete() {
        use std::sync::mpsc;
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = CloudBridge::default();
        bridge.initialize_admin(&mut conn,"admin","test-secret","Owner").unwrap();
        let guard = bridge.authorize_business(&conn,BusinessPermission::Administrator).unwrap();
        std::thread::scope(|scope| {
            let (started_tx,started_rx)=mpsc::channel();
            let (done_tx,done_rx)=mpsc::channel();
            let bridge_ref=&bridge;
            let join=scope.spawn(move|| {
                started_tx.send(()).unwrap();
                bridge_ref.logout().unwrap();
                done_tx.send(()).unwrap();
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
            crate::services::inventory_service::create_category(&conn,"Completed before logout",None).unwrap();
            drop(guard);
            done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            join.join().unwrap();
        });
        assert!(bridge.authorize_business(&conn,BusinessPermission::Administrator).is_err());
        assert_eq!(conn.query_row("SELECT count(*) FROM categories",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }
    #[test]
    fn native_delivery_rechecks_login_generation_and_can_complete_only_once() {
        let _serial = super::super::manager::SYNC_TEST_LOCK.lock().unwrap();
        let db=crate::database::Db::init_in_memory().unwrap();let mut conn=db.lock();let bridge=CloudBridge::default();
        bridge.initialize_admin(&mut conn,"admin","test-secret","Owner").unwrap();
        let lease=bridge.begin(&conn).unwrap();let ctx=bridge.prepare_delivery(&conn,&lease.exchange_id).unwrap();
        assert_eq!(ctx.identity.local_user_id,"1");assert!(bridge.prepare_delivery(&conn,&lease.exchange_id).is_err());
        bridge.commit_delivery(&conn,&ctx,||Ok(serde_json::json!({"test":true}))).unwrap();
        assert!(bridge.commit_delivery(&conn,&ctx,||panic!("duplicate delivery")).is_err());
        bridge.end(&lease.exchange_id,&conn).unwrap();
        let lease=bridge.begin(&conn).unwrap();let old=bridge.prepare_delivery(&conn,&lease.exchange_id).unwrap();
        bridge.logout().unwrap();bridge.login(&conn,"admin","test-secret").unwrap();
        let next=bridge.begin(&conn).unwrap();
        assert!(bridge.commit_delivery(&conn,&old,||panic!("stale owner must not get a token")).is_err());
        let ctx=bridge.prepare_delivery(&conn,&next.exchange_id).unwrap();
        bridge.state.lock().unwrap().active.as_mut().unwrap().expires=Instant::now();
        assert!(bridge.commit_delivery(&conn,&ctx,||panic!("expired lease")).is_err());
        let lease=bridge.begin(&conn).unwrap();let ctx=bridge.prepare_delivery(&conn,&lease.exchange_id).unwrap();
        conn.execute("UPDATE users SET is_active=0 WHERE id=1",[]).unwrap();
        assert!(bridge.commit_delivery(&conn,&ctx,||panic!("deactivated account")).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),0);
        assert!(SyncGuard::acquire().is_some());
    }
    #[test]
    fn native_login_logout_and_account_changes_gate_all_exchange_callbacks() {
        let _serial = super::super::manager::SYNC_TEST_LOCK.lock().unwrap();
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = CloudBridge::default();
        assert!(bridge.with_authorization(&conn, || panic!("must not read credentials")).is_err());
        assert!(bridge.begin(&conn).is_err());
        let admin = bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
        assert_eq!(bridge.with_authorization(&conn, || "authorized").unwrap(), "authorized");
        crate::services::inventory_service::create_category(&conn, "Pending", None).unwrap();
        let lease = bridge.begin(&conn).unwrap();
        let events = bridge.pending(&lease.exchange_id, &conn).unwrap();
        let ids = vec![events[0].event_id.clone()];
        bridge.logout().unwrap();
        assert!(bridge.with_authorization(&conn, || panic!("logged out")).is_err());
        assert!(bridge.pending(&lease.exchange_id, &conn).is_err());
        assert!(bridge.acknowledge(&lease.exchange_id, &mut conn, &ids).is_err());
        assert!(bridge.cursor(&lease.exchange_id, &conn).is_err());
        assert!(bridge.apply(&lease.exchange_id, &mut conn, "0", &[]).is_err());
        assert!(bridge.end(&lease.exchange_id, &conn).is_err());
        assert_eq!(cloud_dispatch::read_pending_uploads(&conn, 500).unwrap().len(), 1);
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(), 0);

        // Login as the same account must create new ownership, never revive
        // a pending callback from a previous authenticated session.
        bridge.login(&conn, "admin", "test-secret").unwrap();
        let next = bridge.begin(&conn).unwrap();
        assert_ne!(next.exchange_id, lease.exchange_id);
        assert!(bridge.acknowledge(&lease.exchange_id, &mut conn, &ids).is_err());
        bridge.login(&conn, "admin", "test-secret").unwrap();
        assert!(bridge.pending(&next.exchange_id, &conn).is_err());
        let next = bridge.begin(&conn).unwrap();
        assert!(bridge.login(&conn, "admin", "wrong-password").is_err());
        assert!(bridge.cursor(&next.exchange_id, &conn).is_err());
        assert!(bridge.begin(&conn).is_err());

        // Changes are checked before a committed upload can be acknowledged.
        for mutation in ["UPDATE users SET role='staff' WHERE username='admin'",
            "UPDATE users SET is_active=0 WHERE username='admin'",
            "UPDATE users SET password_hash='changed' WHERE username='admin'"] {
            bridge.login(&conn, "admin", "test-secret").unwrap();
            let lease = bridge.begin(&conn).unwrap();
            bridge.pending(&lease.exchange_id, &conn).unwrap();
            conn.execute_batch(mutation).unwrap();
            assert!(bridge.acknowledge(&lease.exchange_id, &mut conn, &ids).is_err());
            assert!(bridge.with_authorization(&conn, || panic!("revoked account")).is_err());
            assert_eq!(cloud_dispatch::read_pending_uploads(&conn, 500).unwrap().len(), 1);
            conn.execute("UPDATE users SET role='admin',is_active=1,password_hash=?1 WHERE id=?2",
                rusqlite::params![admin.password_hash, admin.id]).unwrap();
            assert!(bridge.begin(&conn).is_err()); // Restoring rows is not a login.
        }
        conn.execute("INSERT INTO users(username,password_hash,full_name,role) VALUES ('staff',?1,'Staff','staff')",
            [&admin.password_hash]).unwrap();
        assert_eq!(bridge.login(&conn, "staff", "test-secret").unwrap().role, UserRole::Staff);
        assert!(bridge.begin(&conn).is_err());
        assert!(bridge.with_authorization(&conn, || panic!("staff credentials")).is_err());
        bridge.login(&conn, "admin", "test-secret").unwrap();
        let expired = bridge.begin(&conn).unwrap();
        bridge.state.lock().unwrap().principal.as_mut().unwrap().expires = Instant::now();
        assert!(bridge.pending(&expired.exchange_id, &conn).is_err());
        assert!(bridge.begin(&conn).is_err());
        assert!(SyncGuard::acquire().is_some());
        bridge.login(&conn, "admin", "test-secret").unwrap();
        conn.execute("DELETE FROM users WHERE id=?1", [admin.id]).unwrap();
        assert!(bridge.with_authorization(&conn, || panic!("deleted account")).is_err());
        assert!(bridge.begin(&conn).is_err());
    }
    #[test]
    fn poisoned_lease_releases_shared_guard_and_rejects_stale_callbacks() {
        let _serial = super::super::manager::SYNC_TEST_LOCK.lock().unwrap();
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let bridge = std::sync::Arc::new(CloudBridge::default());
        bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
        let abandoned = bridge.begin(&conn).unwrap();
        let poisoned = bridge.clone();
        assert!(std::thread::spawn(move || {
            let _state = poisoned.state.lock().unwrap();
            panic!("simulate a command panic while holding its volatile lease");
        }).join().is_err());
        assert!(bridge.state.is_poisoned());
        assert!(bridge.pending(&abandoned.exchange_id, &conn).is_err());
        assert!(!bridge.state.is_poisoned());
        assert!(bridge.acknowledge(&abandoned.exchange_id, &mut conn, &["stale".into()]).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(), 0);
        bridge.login(&conn, "admin", "test-secret").unwrap();
        let recovered = bridge.begin(&conn).unwrap();
        assert_ne!(recovered.exchange_id, abandoned.exchange_id);
        assert!(bridge.end(&abandoned.exchange_id, &conn).is_err());
        bridge.end(&recovered.exchange_id, &conn).unwrap();
        // Recovery also released the same guard used by the legacy adapter.
        assert!(SyncGuard::acquire().is_some());
    }
    #[test]
    fn closed_release_and_exact_local_exchange_callbacks() {
        let _serial = super::super::manager::SYNC_TEST_LOCK.lock().unwrap();
        assert!(!LIVE_NEON_SYNC_ENABLED);
        let db = crate::database::Db::init_in_memory().unwrap();
        let mut conn = db.lock();
        let event_id = {
            let tx = conn.transaction().unwrap();
            let id = crate::sync::queue::enqueue_cloud_event(&tx,"categories","local:bridge","upsert",
                &serde_json::json!({"name":"Bridge"})).unwrap();
            tx.commit().unwrap(); id
        };
        let bridge = CloudBridge::default();
        bridge.initialize_admin(&mut conn, "admin", "test-secret", "Owner").unwrap();
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
        bridge.end(&lease.exchange_id, &conn).unwrap();
        assert!(bridge.cursor(&lease.exchange_id,&conn).is_err());
        let abandoned = bridge.begin(&conn).unwrap();
        bridge.state.lock().unwrap().active.as_mut().unwrap().expires = Instant::now();
        assert!(bridge.cursor(&abandoned.exchange_id,&conn).is_err());
        let recovered = bridge.begin(&conn).unwrap();
        bridge.end(&recovered.exchange_id, &conn).unwrap();
    }
}
