# WebView hardening — stage 3A

Bundled scripts only: CSP rejects inline scripts, event attributes, remote scripts,
remote connections, workers, objects, form submission and base URL overrides.
Inline styles remain supported by the existing UI. The main window denies remote
navigation and new windows; developer tools are disabled in its configuration.

All 60 registered application commands are enrolled in the Tauri application
manifest and granted explicitly to the local main window only. Registry drift
is tested. This restricts execution context, not business roles: native session
and role checks on legacy business commands are implemented in
[stage 3B](BUSINESS_AUTHORIZATION.md); packaged runtime acceptance remains open. Existing native cloud-administrator checks remain in place.

Charts, backup status, numeric attributes and printable cells escape stored
values. Portraits accept bounded raster base64 data URLs only. Receipts and
purchase-history tables print through a script-disabled sandboxed frame.
Physical printer and actual packaged WebView navigation/print acceptance remain
manual staging requirements; Edge browser fixtures are not those acceptance tests.

The data transport still uses WebView fetch. No remote data origin is currently
allowed. Before authorized staging, pin the exact approved HTTPS data origin in
connect-src in tauri.conf.json, rebuild, and repeat acceptance. Native credential
delivery requires this same pin before reserving a session. Do not add wildcard
hosts or broad HTTPS permission. Broker delivery remains native WinHTTP.

Validation includes Node policy/template regressions, real Edge injection/CSP/
startup/print-frame fixtures on Windows CI, and Rust navigation/origin tests.
No live flags, credentials, SQL, deployment or main-branch merge were changed.
