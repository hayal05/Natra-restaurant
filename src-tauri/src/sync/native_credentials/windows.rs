//! Minimal Windows API boundary. RAII owns every native allocation/handle.
use super::*;
use std::{ffi::c_void,ptr::{null,null_mut},time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::{GetLastError,ERROR_NOT_FOUND},Security::Credentials::*,Networking::WinHttp::*};
fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }
struct Credential(*mut CREDENTIALW);
impl Drop for Credential { fn drop(&mut self) { unsafe {
    if !self.0.is_null() {
        let c=&mut *self.0;
        if !c.CredentialBlob.is_null() && c.CredentialBlobSize as usize<=MAX_PACKET {
            std::slice::from_raw_parts_mut(c.CredentialBlob,c.CredentialBlobSize as usize).zeroize();
        }
        CredFree(self.0.cast());
    }
} } }
pub(super) fn read(target: &str) -> AppResult<Zeroizing<Vec<u8>>> { unsafe {
    let target=wide(target);let mut ptr=null_mut();
    if CredReadW(target.as_ptr(),CRED_TYPE_GENERIC,0,&mut ptr)==0 { return Err(unavailable()); }
    let guard=Credential(ptr); let c=guard.0.as_ref().ok_or_else(unavailable)?;
    if c.Type!=CRED_TYPE_GENERIC || c.CredentialBlob.is_null() || c.CredentialBlobSize==0
        || c.CredentialBlobSize as usize>MAX_PACKET { return Err(unavailable()); }
    Ok(Zeroizing::new(std::slice::from_raw_parts(c.CredentialBlob,c.CredentialBlobSize as usize).to_vec()))
} }
pub(super) fn write(target: &str,bytes: &[u8]) -> AppResult<()> { unsafe {
    if bytes.is_empty() || bytes.len()>MAX_PACKET { return Err(unavailable()); }
    let target=wide(target);let mut existing=null_mut();
    if CredReadW(target.as_ptr(),CRED_TYPE_GENERIC,0,&mut existing)!=0 {
        drop(Credential(existing));return Err(unavailable()); // Explicit remove before rotation.
    }
    if GetLastError()!=ERROR_NOT_FOUND { return Err(unavailable()); }
    let mut c: CREDENTIALW=std::mem::zeroed();
    c.Type=CRED_TYPE_GENERIC;c.TargetName=target.as_ptr() as *mut u16;
    c.CredentialBlobSize=bytes.len() as u32;c.CredentialBlob=bytes.as_ptr() as *mut u8;
    c.Persist=CRED_PERSIST_LOCAL_MACHINE;
    if CredWriteW(&c,0)==0 { return Err(unavailable()); } Ok(())
} }
pub(super) fn remove(target: &str) -> AppResult<()> { unsafe {
    let target=wide(target);
    if CredDeleteW(target.as_ptr(),CRED_TYPE_GENERIC,0)==0 && GetLastError()!=ERROR_NOT_FOUND { return Err(unavailable()); } Ok(())
} }
struct Handle(*mut c_void);
impl Drop for Handle { fn drop(&mut self) { unsafe { WinHttpCloseHandle(self.0); } } }
fn handle(ptr: *mut c_void) -> AppResult<Handle> { if ptr.is_null() { Err(unavailable()) } else { Ok(Handle(ptr)) } }
unsafe fn option(h: &Handle,code: u32,mut value: u32) -> AppResult<()> {
    if WinHttpSetOption(h.0,code,(&mut value as *mut u32).cast(),4)==0 { return Err(unavailable()); } Ok(())
}
unsafe fn header(h: &Handle,code: u32) -> AppResult<String> {
    let mut bytes=0;
    WinHttpQueryHeaders(h.0,code,null(),null_mut(),&mut bytes,null_mut());
    if bytes==0 || bytes>2048 { return Err(unavailable()); }
    let mut buf=vec![0u16;(bytes as usize+1)/2];
    if WinHttpQueryHeaders(h.0,code,null(),buf.as_mut_ptr().cast(),&mut bytes,null_mut())==0 { return Err(unavailable()); }
    let end=buf.iter().position(|c|*c==0).unwrap_or(buf.len());
    String::from_utf16(&buf[..end]).map_err(|_|unavailable())
}
fn client_session() -> AppResult<Handle> { unsafe {
    let agent=wide("NatraRestaurant/SyncBroker");
    let session=handle(WinHttpOpen(agent.as_ptr(),WINHTTP_ACCESS_TYPE_NO_PROXY,null(),null(),0))?;
    if WinHttpSetTimeouts(session.0,5000,5000,5000,5000)==0 { return Err(unavailable()); }
    option(&session,WINHTTP_OPTION_CONNECT_RETRIES,1)?;
    option(&session,WINHTTP_OPTION_SECURE_PROTOCOLS,WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2|WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3)?;
    Ok(session)
} }
fn request_handle(connection: &Handle) -> AppResult<Handle> { unsafe {
    let method=wide("POST");let path=wide("/session");
    let req=handle(WinHttpOpenRequest(connection.0,method.as_ptr(),path.as_ptr(),null(),null(),null(),WINHTTP_FLAG_SECURE))?;
    option(&req,WINHTTP_OPTION_DISABLE_FEATURE,WINHTTP_DISABLE_REDIRECTS|WINHTTP_DISABLE_COOKIES|WINHTTP_DISABLE_AUTHENTICATION)?;
    option(&req,WINHTTP_OPTION_MAX_RESPONSE_HEADER_SIZE,16384)?;
    Ok(req)
} }
#[cfg(test)]
#[test]
fn winhttp_session_applies_single_attempt_and_modern_tls_without_sending() {
    let session=client_session().unwrap();
    unsafe {
        // TLS and disabled-feature options are set-only. Their setters are
        // checked by construction; query only options documented as readable.
        let host=wide("broker.example.invalid");
        let connection=handle(WinHttpConnect(session.0,host.as_ptr(),443,0)).unwrap();
        let request=request_handle(&connection).unwrap(); // No send or receive.
        for (h,code,expected) in [(&session,WINHTTP_OPTION_CONNECT_RETRIES,1),
            (&request,WINHTTP_OPTION_CONNECT_RETRIES,1),(&request,WINHTTP_OPTION_MAX_RESPONSE_HEADER_SIZE,16384)] {
            let mut actual=0u32;let mut size=4u32;
            let queried=WinHttpQueryOption(h.0,code,(&mut actual as *mut u32).cast(),&mut size);
            assert_ne!(queried,0,"policy query {code} failed: {}",GetLastError());
            assert_eq!(actual,expected);
        }
    }
}
pub(super) fn request(p: &Provision) -> AppResult<Zeroizing<Vec<u8>>> { unsafe {
    let started=Instant::now();
    let check=||if started.elapsed()>Duration::from_secs(20) { Err(unavailable()) } else { Ok(()) };
    let url=https(&p.broker_endpoint)?;
    let session=client_session()?;
    let host=wide(url.host_str().ok_or_else(unavailable)?);
    let connection=handle(WinHttpConnect(session.0,host.as_ptr(),443,0))?;
    let req=request_handle(&connection)?;
    let headers=Zeroizing::new(format!("Authorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nCache-Control: no-store\r\n",p.credential));
    let headers=Zeroizing::new(wide(&headers));
    let body=serde_json::to_vec(&serde_json::json!({"installation_id":p.installation_id,"local_user_id":p.local_user_id})).map_err(|_|unavailable())?;
    check()?;
    if WinHttpSendRequest(req.0,headers.as_ptr(),(headers.len()-1) as u32,body.as_ptr().cast(),body.len() as u32,body.len() as u32,0)==0 { return Err(unavailable()); }
    check()?;
    if WinHttpReceiveResponse(req.0,null_mut())==0 { return Err(unavailable()); }
    check()?;
    let mut status=0u32;let mut size=4u32;
    if WinHttpQueryHeaders(req.0,WINHTTP_QUERY_STATUS_CODE|WINHTTP_QUERY_FLAG_NUMBER,null(),(&mut status as *mut u32).cast(),&mut size,null_mut())==0
        || status!=200 { return Err(unavailable()); }
    if header(&req,WINHTTP_QUERY_CONTENT_TYPE)?.split(';').next().unwrap_or("").trim().to_ascii_lowercase()!="application/json"
        || !header(&req,WINHTTP_QUERY_CACHE_CONTROL)?.split(',').any(|v|v.trim().eq_ignore_ascii_case("no-store")) { return Err(unavailable()); }
    // WinHTTP decompression is not enabled. Reject encoded bodies explicitly.
    if let Ok(encoding)=header(&req,WINHTTP_QUERY_CONTENT_ENCODING) {
        if !encoding.eq_ignore_ascii_case("identity") { return Err(unavailable()); }
    }
    let mut bytes=Zeroizing::new(Vec::new());let mut chunk=Zeroizing::new(vec![0u8;4096]);
    for _ in 0..64 {
        check()?;let mut read=0u32;
        if WinHttpReadData(req.0,chunk.as_mut_ptr().cast(),chunk.len() as u32,&mut read)==0 { return Err(unavailable()); }
        check()?;
        if read==0 { return Ok(bytes); }
        if read as usize>chunk.len() || bytes.len()+read as usize>MAX_RESPONSE { return Err(unavailable()); }
        bytes.extend_from_slice(&chunk[..read as usize]);
    }
    Err(unavailable())
} }
