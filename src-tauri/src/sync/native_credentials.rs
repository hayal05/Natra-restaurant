//! Windows-only opaque credential storage and bounded broker delivery.
//! The provisioned secret never implements Serialize/Debug or crosses IPC.
use crate::errors::{AppError, AppResult};
use serde::Deserialize;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::{Zeroize, Zeroizing};
use url::Url;

pub(crate) const MAX_PACKET: usize = 2560;
const MAX_RESPONSE: usize = 16 * 1024;
pub(crate) fn unavailable() -> AppError { AppError::Sync("secure cloud credentials are unavailable; review provisioning or retry later".into()) }
#[derive(Clone)]
pub(crate) struct Identity { pub installation_id: String, pub local_user_id: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provision {
    version: u8, credential_id: String, workspace_id: String, installation_id: String,
    local_user_id: String, subject: String, credential: String,
    broker_endpoint: String, sync_origin: String, issuer: String, audience: String,
}
impl Drop for Provision { fn drop(&mut self) { self.credential.zeroize(); } }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    token: String, session_id: String, expires_at: String, credential_id: String,
    workspace_id: String, installation_id: String, local_user_id: String, subject: String,
}
impl Drop for Response { fn drop(&mut self) { self.token.zeroize(); } }
#[derive(PartialEq,Eq)]
struct Approval { credential_id: String, workspace_id: String, installation_id: String, local_user_id: String,
    subject: String, broker_endpoint: String, sync_origin: String, issuer: String, audience: String }
impl Provision { fn approval(&self) -> Approval { Approval { credential_id:self.credential_id.clone(),workspace_id:self.workspace_id.clone(),
    installation_id:self.installation_id.clone(),local_user_id:self.local_user_id.clone(),subject:self.subject.clone(),
    broker_endpoint:self.broker_endpoint.clone(),sync_origin:self.sync_origin.clone(),issuer:self.issuer.clone(),audience:self.audience.clone() } } }
pub(crate) struct Delivered { token: String, approval: Approval, expires: i64 }
impl Drop for Delivered { fn drop(&mut self) { self.token.zeroize(); } }
impl Delivered {
    pub(crate) fn ensure_current(&self, identity: &Identity) -> AppResult<()> {
        let bytes=platform_read(&key(identity)?)?;
        if parse(&bytes,Some(identity))?.approval()!=self.approval { return Err(unavailable()); }
        Ok(())
    }
    pub(crate) fn configuration(&self) -> AppResult<serde_json::Value> {
        if self.expires <= chrono::Utc::now().timestamp()+15 { return Err(unavailable()); }
        let mut config = endpoints(&self.approval.sync_origin);
        config["token"] = serde_json::Value::String(self.token.clone());
        Ok(config)
    }
}
fn uuid(value: &str) -> bool {
    let bytes=value.as_bytes();
    bytes.len()==36 && bytes.iter().enumerate().all(|(i,b)| {
        if [8,13,18,23].contains(&i) { *b==b'-' } else { b.is_ascii_hexdigit() }
    }) && matches!(bytes[14],b'1'..=b'8') && matches!(bytes[19].to_ascii_lowercase(),b'8'|b'9'|b'a'|b'b')
}
fn user_id(value: &str) -> bool {
    !value.is_empty() && value.len()<=19 && !value.starts_with('0') && value.bytes().all(|b|b.is_ascii_digit())
        && value.parse::<i64>().map(|v|v>0).unwrap_or(false)
}
fn https(value: &str) -> AppResult<Url> {
    if value.len()>512 || !value.bytes().all(|b|b.is_ascii_graphic()) { return Err(unavailable()); }
    let url=Url::parse(value).map_err(|_|unavailable())?;
    if url.scheme()!="https" || !url.username().is_empty() || url.password().is_some() || url.query().is_some()
        || url.fragment().is_some() || url.host_str().is_none() || url.port_or_known_default()!=Some(443) {
        return Err(unavailable());
    }
    Ok(url)
}
fn key(identity: &Identity) -> AppResult<String> {
    if !uuid(&identity.installation_id) || !user_id(&identity.local_user_id) { return Err(unavailable()); }
    Ok(format!("NatraTechnology/Restaurant/Sync/v1/{}/{}",identity.installation_id.to_ascii_lowercase(),identity.local_user_id))
}
fn parse(bytes: &[u8], expected: Option<&Identity>) -> AppResult<Provision> {
    if bytes.is_empty() || bytes.len()>MAX_PACKET { return Err(unavailable()); }
    let p: Provision=serde_json::from_slice(bytes).map_err(|_|unavailable())?;
    let broker=https(&p.broker_endpoint)?; let origin=https(&p.sync_origin)?; https(&p.issuer)?;
    let identity=Identity { installation_id:p.installation_id.clone(),local_user_id:p.local_user_id.clone() };
    key(&identity)?;
    if p.version!=1 || !uuid(&p.credential_id) || !uuid(&p.workspace_id) || broker.path()!="/session" || origin.path()!="/"
        || p.subject.trim().is_empty() || p.subject.len()>256 || p.subject.chars().any(char::is_control)
        || p.audience.trim().is_empty() || p.audience.len()>256 || p.audience.chars().any(char::is_control)
        || p.credential.len()!=54 || !p.credential.starts_with("natra_sync_")
        || !p.credential.as_bytes()[11..].iter().all(|b|b.is_ascii_alphanumeric() || *b==b'-' || *b==b'_') {
        return Err(unavailable());
    }
    if let Some(e)=expected {
        if !p.installation_id.eq_ignore_ascii_case(&e.installation_id) || p.local_user_id!=e.local_user_id { return Err(unavailable()); }
    }
    Ok(p)
}
fn endpoints(origin: &str) -> serde_json::Value {
    let origin=origin.trim_end_matches('/');
    serde_json::json!({"uploadEndpoint":format!("{origin}/sync/upload"),"pullEndpoint":format!("{origin}/sync/pull"),
        "completeEndpoint":format!("{origin}/sync/complete"),"failEndpoint":format!("{origin}/sync/fail"),
        "allowanceEndpoint":format!("{origin}/sync/allowance"),"broker":true})
}
fn decode(segment: &str, max: usize) -> AppResult<Zeroizing<Vec<u8>>> {
    if segment.len()>max*2 { return Err(unavailable()); }
    let bytes=Zeroizing::new(URL_SAFE_NO_PAD.decode(segment).map_err(|_|unavailable())?);
    if bytes.len()>max { return Err(unavailable()); } Ok(bytes)
}
fn validate_response(p: &Provision, bytes: &[u8], now: i64) -> AppResult<Delivered> {
    if bytes.len()>MAX_RESPONSE { return Err(unavailable()); }
    let r: Response=serde_json::from_slice(bytes).map_err(|_|unavailable())?;
    if r.token.is_empty() || r.token.len()>8192 || !r.token.bytes().all(|b|b.is_ascii_graphic()) || !uuid(&r.session_id)
        || r.credential_id!=p.credential_id || r.workspace_id!=p.workspace_id || r.installation_id!=p.installation_id
        || r.local_user_id!=p.local_user_id || r.subject!=p.subject { return Err(unavailable()); }
    let parts: Vec<_>=r.token.split('.').collect(); if parts.len()!=3 { return Err(unavailable()); }
    let header: serde_json::Value=serde_json::from_slice(&decode(parts[0],1024)?).map_err(|_|unavailable())?;
    let claims: serde_json::Value=serde_json::from_slice(&decode(parts[1],4096)?).map_err(|_|unavailable())?;
    let signature=decode(parts[2],1024)?;
    let iat=claims["iat"].as_i64().ok_or_else(unavailable)?;
    let exp=claims["exp"].as_i64().ok_or_else(unavailable)?;
    let deadline=chrono::DateTime::parse_from_rfc3339(&r.expires_at).map_err(|_|unavailable())?.timestamp();
    if header["alg"]!="RS256" || header["typ"]!="JWT" || signature.len()<256 || claims["iss"]!=p.issuer
        || claims["aud"]!=p.audience || claims["sub"]!=p.subject || claims["scope"]!="natra:sync"
        || claims["workspace_id"]!=p.workspace_id || claims["installation_id"]!=p.installation_id
        || claims["local_user_id"]!=p.local_user_id || claims["credential_id"]!=p.credential_id
        || claims["session_id"]!=r.session_id || exp!=deadline || exp<=now+30 || iat>now+30
        || exp<=iat || exp.saturating_sub(iat)>900 || exp.saturating_sub(now)>900 { return Err(unavailable()); }
    // Consistency validation, NOT signature verification. Trust here is the
    // OS-provisioned HTTPS broker; the data service verifies the JWT signature.
    Ok(Delivered { token:r.token.clone(),approval:p.approval(),expires:exp })
}
pub(crate) fn available(identity: &Identity) -> AppResult<serde_json::Value> {
    let bytes=platform_read(&key(identity)?)?;
    let p=parse(&bytes,Some(identity))?;
    Ok(endpoints(&p.sync_origin)) // Never return the opaque secret or a token on refresh.
}
static DELIVERY_BUSY: std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
struct DeliveryGuard;
impl Drop for DeliveryGuard { fn drop(&mut self) { DELIVERY_BUSY.store(false,std::sync::atomic::Ordering::Release); } }
pub(crate) fn fetch(identity: &Identity) -> AppResult<Delivered> {
    if DELIVERY_BUSY.compare_exchange(false,true,std::sync::atomic::Ordering::AcqRel,std::sync::atomic::Ordering::Acquire).is_err() {
        return Err(unavailable());
    }
    let _guard=DeliveryGuard;
    let bytes=platform_read(&key(identity)?)?;
    let p=parse(&bytes,Some(identity))?;
    let raw=platform_request(&p)?;
    validate_response(&p,&raw,chrono::Utc::now().timestamp())
}

// Separate trusted operator utility. Secrets arrive via bounded stdin, never
// command arguments or a desktop IPC setter. CI uses synthetic test entries only.
pub fn provision_cli(args: &[String]) -> Result<(), ()> {
    use std::io::{Read,IsTerminal};
    if args.len()==1 && args[0]=="--install" {
        if std::io::stdin().is_terminal() { return Err(()); }
        let mut bytes=Zeroizing::new(Vec::new());
        std::io::stdin().take((MAX_PACKET+1) as u64).read_to_end(&mut bytes).map_err(|_|())?;
        let p=parse(&bytes,None).map_err(|_|())?;
        let target=key(&Identity { installation_id:p.installation_id.clone(),local_user_id:p.local_user_id.clone() }).map_err(|_|())?;
        platform_write(&target,&bytes).map_err(|_|())
    } else if args.len()==3 && args[0]=="--remove" {
        let target=key(&Identity { installation_id:args[1].clone(),local_user_id:args[2].clone() }).map_err(|_|())?;
        platform_remove(&target).map_err(|_|())
    } else { Err(()) }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::{read as platform_read,write as platform_write,remove as platform_remove,request as platform_request};
#[cfg(not(windows))]
fn platform_read(_: &str) -> AppResult<Zeroizing<Vec<u8>>> { Err(unavailable()) }
#[cfg(not(windows))]
fn platform_write(_: &str,_: &[u8]) -> AppResult<()> { Err(unavailable()) }
#[cfg(not(windows))]
fn platform_remove(_: &str) -> AppResult<()> { Err(unavailable()) }
#[cfg(not(windows))]
fn platform_request(_: &Provision) -> AppResult<Zeroizing<Vec<u8>>> { Err(unavailable()) }

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> Identity { Identity { installation_id:"22222222-2222-4222-8222-222222222222".into(),local_user_id:"9007199254740993".into() } }
    fn packet() -> serde_json::Value { serde_json::json!({"version":1,"credential_id":"44444444-4444-4444-8444-444444444444",
        "workspace_id":"11111111-1111-4111-8111-111111111111","installation_id":identity().installation_id,
        "local_user_id":identity().local_user_id,"subject":"approved-subject","credential":format!("natra_sync_{}","A".repeat(43)),
        "broker_endpoint":"https://broker.example.test/session","sync_origin":"https://sync.example.test",
        "issuer":"https://issuer.example.test/","audience":"natra-sync"}) }
    fn provision() -> Provision { parse(&serde_json::to_vec(&packet()).unwrap(),Some(&identity())).unwrap() }
    #[test]
    fn provisioning_is_strict_and_metadata_does_not_include_credentials() {
        let p=provision();let config=endpoints(&p.sync_origin);
        assert_eq!(config["broker"],true);assert!(config.get("token").is_none());
        assert!(!config.to_string().contains(&p.credential));
        for (field,value) in [("version",serde_json::json!(2)),("local_user_id",serde_json::json!("01")),
            ("local_user_id",serde_json::json!("9223372036854775808")),("subject",serde_json::json!("bad\nsubject")),
            ("credential",serde_json::json!("not-a-credential")),("broker_endpoint",serde_json::json!("http://broker.example.test/session")),
            ("broker_endpoint",serde_json::json!("https://user:password@broker.example.test/session")),
            ("broker_endpoint",serde_json::json!("https://broker.example.test/session?next=attacker")),
            ("broker_endpoint",serde_json::json!("https://broker.example.test:444/session")),
            ("sync_origin",serde_json::json!("https://sync.example.test/another/path")),("extra",serde_json::json!(true))] {
            let mut v=packet();v[field]=value;
            assert!(parse(&serde_json::to_vec(&v).unwrap(),None).is_err(),"{field}");
        }
        let other=Identity { local_user_id:"1".into(),..identity() };
        assert!(parse(&serde_json::to_vec(&packet()).unwrap(),Some(&other)).is_err());
        assert!(parse(&vec![b' ';MAX_PACKET+1],None).is_err());
        assert!(parse(br#"{"version":1,"version":1}"#,None).is_err());
        assert!(key(&Identity { local_user_id:"1\n".into(),..identity() }).is_err());
    }
    fn claims(now: i64) -> serde_json::Value { let p=packet();serde_json::json!({"iss":p["issuer"],"aud":p["audience"],"sub":p["subject"],
        "credential_id":p["credential_id"],"workspace_id":p["workspace_id"],"installation_id":p["installation_id"],
        "local_user_id":p["local_user_id"],"session_id":"33333333-3333-4333-8333-333333333333","scope":"natra:sync","iat":now,"exp":now+900}) }
    fn response(claims: &serde_json::Value) -> serde_json::Value {
        // Deliberately synthetic signature: native validation checks consistency,
        // not cryptography. Node broker tests verify real generated RSA signatures.
        let token=format!("{}.{}.{}",URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap()),URL_SAFE_NO_PAD.encode([0u8;256]));
        let p=packet();serde_json::json!({"token":token,"session_id":claims["session_id"],"expires_at":
            chrono::DateTime::from_timestamp(claims["exp"].as_i64().unwrap(),0).unwrap().to_rfc3339(),
            "credential_id":p["credential_id"],"workspace_id":p["workspace_id"],"installation_id":p["installation_id"],
            "local_user_id":p["local_user_id"],"subject":p["subject"]})
    }
    #[test]
    fn delivered_tokens_must_match_approved_scope_and_deadlines() {
        let p=provision();let now=chrono::Utc::now().timestamp();let c=claims(now);let r=response(&c);
        let delivered=validate_response(&p,&serde_json::to_vec(&r).unwrap(),now).unwrap();
        assert_eq!(delivered.configuration().unwrap()["token"],r["token"]);
        for field in ["iss","aud","sub","credential_id","workspace_id","installation_id","local_user_id","scope"] {
            let mut changed=c.clone();changed[field]=serde_json::json!("wrong");
            assert!(validate_response(&p,&serde_json::to_vec(&response(&changed)).unwrap(),now).is_err(),"{field}");
        }
        for (field,value) in [("exp",now+901),("exp",now+10),("iat",now+60),("iat",now-1)] {
            let mut changed=c.clone();changed[field]=serde_json::json!(value);
            assert!(validate_response(&p,&serde_json::to_vec(&response(&changed)).unwrap(),now).is_err(),"{field}");
        }
        for field in ["credential_id","workspace_id","installation_id","local_user_id","subject","expires_at","token"] {
            let mut changed=r.clone();changed[field]=serde_json::json!("wrong");
            assert!(validate_response(&p,&serde_json::to_vec(&changed).unwrap(),now).is_err(),"{field}");
        }
        assert!(validate_response(&p,&vec![b' ';MAX_RESPONSE+1],now).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn windows_store_roundtrip_uses_an_isolated_synthetic_target() {
        let conn=rusqlite::Connection::open_in_memory().unwrap();
        let random: String=conn.query_row("SELECT lower(hex(randomblob(16)))",[],|r|r.get(0)).unwrap();
        let target=format!("NatraTechnology/Restaurant/Sync/Test/{random}");
        struct Cleanup(String);impl Drop for Cleanup { fn drop(&mut self) { let _=platform_remove(&self.0); } }
        let _cleanup=Cleanup(target.clone());
        let bytes=Zeroizing::new(serde_json::to_vec(&packet()).unwrap());
        platform_write(&target,&bytes).unwrap();
        assert_eq!(&*platform_read(&target).unwrap(),&*bytes);
        assert_eq!(&*platform_read(&target).unwrap(),&*bytes); // Clearing read buffers does not alter the stored copy.
        assert!(platform_write(&target,&bytes).is_err()); // Never overwrite silently.
        platform_remove(&target).unwrap();assert!(platform_read(&target).is_err());
        platform_remove(&target).unwrap();
    }
}
