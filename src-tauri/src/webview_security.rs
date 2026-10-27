//! Navigation is limited to the bundled root document. Hash routes remain local.
pub(crate) fn allow_main_navigation(url: &url::Url) -> bool {
    let origin=match url.scheme() {
        "tauri"=>url.host_str()==Some("localhost"),
        "http"=>url.host_str()==Some("tauri.localhost"),
        _=>false,
    };
    origin && url.username().is_empty() && url.password().is_none() && url.port().is_none()
        && url.query().is_none() && matches!(url.path(),"/"|"/index.html")
}
// The data transport still uses WebView fetch. Require an exact approved HTTPS
// origin in CSP before native delivery can reserve a broker session. Generic
// schemes/wildcards never count as approval. The current policy pins none.
pub(crate) fn allows_data_endpoint(endpoint: &str) -> bool {
    let config: serde_json::Value=match serde_json::from_str(include_str!("../tauri.conf.json")) { Ok(v)=>v,Err(_)=>return false };
    allows_data_endpoint_from(endpoint,config["app"]["security"]["csp"]["connect-src"].as_str().unwrap_or(""))
}
fn allows_data_endpoint_from(endpoint: &str, sources: &str) -> bool {
    let url=match url::Url::parse(endpoint) { Ok(url)=>url,Err(_)=>return false };
    if url.scheme()!="https" || !url.username().is_empty() || url.password().is_some() || url.query().is_some()
        || url.fragment().is_some() || url.port_or_known_default()!=Some(443) { return false; }
    let origin=url.origin().ascii_serialization();
    sources.split_ascii_whitespace().any(|source|source.starts_with("https://") && !source.contains('*') && source==origin)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_origin_must_be_pinned_exactly_before_delivery() {
        let endpoint="https://sync.example.test/sync/upload";
        assert!(allows_data_endpoint_from(endpoint,"'self' ipc: http://ipc.localhost https://sync.example.test"));
        for sources in ["'self' ipc:","https:","*","https://*.example.test","https://sync.example.test/path","https://other.example.test"] {
            assert!(!allows_data_endpoint_from(endpoint,sources),"{sources}");
        }
        for endpoint in ["https://sync.example.test.evil/sync/upload","http://sync.example.test/sync/upload",
            "https://user@sync.example.test/sync/upload","https://sync.example.test:444/sync/upload"] {
            assert!(!allows_data_endpoint_from(endpoint,"https://sync.example.test"));
        }
        assert!(!allows_data_endpoint("https://unconfigured.invalid/sync/upload"));
    }
    #[test]
    fn only_bundled_root_and_hash_routes_can_navigate() {
        for value in ["tauri://localhost/index.html#/pos","tauri://localhost/","http://tauri.localhost/index.html#/reports","http://tauri.localhost/"] {
            assert!(allow_main_navigation(&value.parse().unwrap()),"{value}");
        }
        for value in ["https://example.test/","http://tauri.evil.test/","http://tauri.localhost.evil.test/", "tauri://evil/index.html",
            "http://tauri.localhost:9000/index.html","http://tauri.localhost/js/app.js","tauri://localhost/index.html?next=bad",
            "http://user@tauri.localhost/","file:///tmp/evil.html","about:blank","data:text/html,bad","javascript:alert(1)",
            "blob:http://tauri.localhost/bad"] {
            assert!(!allow_main_navigation(&value.parse().unwrap()),"{value}");
        }
    }
}
