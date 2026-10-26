# Task 42 — Security and reliability source audit

Reviewed 26 October 2026. Scope: staged Neon manual exchange, HTTP adapters,
authorization SQL, native lease and local replay. This is a source review and
automated regression pass, not penetration-test or production certification.
No live database, credentials, deployment or release flags were changed.

## Fixed finding

A panic while holding `CloudBridge.active` poisoned the mutex permanently.
The retained `Active` also held the process-wide `SyncGuard`, blocking later
manual exchanges and legacy sync until process restart. `cloud_bridge.rs` now
recovers the mutex, discards the uncertain volatile lease/snapshot, drops the
shared guard and clears poison while still holding the mutex. Old lease IDs
fail; a new explicit exchange can start. SQLite acknowledgements, financial
rows, receipts and cursor are not reset. A thread-panic regression checks
stale-callback rejection, fresh lease acquisition and shared-guard release.

## Evidence reviewed

| Boundary | Source / regression evidence | Result |
| --- | --- | --- |
| Activation | `cloud_bridge.rs`, `commands/cloud_sync.rs`, `wrangler.toml` | Native gate false; no live server flags configured |
| Authentication | `auth/jwt.js`, `tests/sync.test.js`, `tests/issuance.test.js` | Pinned server JWKS, RS256, required expiry/scope and bound workspace/install/session; issuance is trusted admin tooling |
| Database authorization | SQL 010–014, API tests | Scope rechecked inside operations; workspace/device/session locks; terminal retries idempotent |
| Upload | SQL 011, `tests/upload.test.js` | Bounded atomic batch, immutable IDs, stable receipts, retry after lost response |
| Pull/replay | SQL 012, `cloud_dispatch.rs`, Rust replay tests | Ordered bounded pages, exact bigint strings, transaction includes financial changes/receipt/cursor |
| Manual lifecycle | `tests/sync_now.test.js`, `tests/recovery.test.js`, native bridge tests | Explicit click, overlap guard, offline cancellation, no automatic retry |
| Two local installations | `sync/multi_device_tests.rs` (Task 41 CI 40363603378) | Real service checkout/settlement, differing local IDs, own echoes, stale-page rejection, restart persistence |
| Origin policy | `api/cors.js`, CORS tests | Exact configured origins, no wildcard/cookie access; origin is not authentication |

## Open findings — live activation blocked

1. **Native identity and credential lifecycle:** commands do not yet bind a
   lease to the authenticated native user/role. Process environment token
   delivery is a staging adapter, not an approved secure provisioner. Implement
   fresh session issuance/delivery, expiry/renewal, logout/revocation and user
   authorization before returning tokens to an enabled WebView.
2. **WebView hardening:** `tauri.conf.json` currently has `csp: null`; existing
   application HTML rendering and IPC access need a focused injection and
   least-privilege review before exposing new credentials. A broad unrelated
   UI rewrite is outside this audit's fix. Test malicious stored text in the
   actual Windows WebView and verify remote navigation/IPC restrictions.
3. **Offline business conflicts:** distinct events settling or reversing the
   same sale on two disconnected installations can encounter an already
   changed target. Current handlers reject and roll back the page rather than
   silently changing financials. A reviewed resolution policy and operator
   recovery workflow are required; stable event-ID deduplication alone does
   not resolve two different business actions. The successful Task 41 scenario
   uses one settlement writer and does not certify this conflict path.
4. **Poisoned ledger events:** upload validates envelope/scope/size, while
   entity-specific financial validation occurs on local replay. An authorized
   malformed event or missing dependency can block subsequent ordered pages.
   Define server validation and auditable repair/quarantine without silently
   skipping financial events or advancing a failed cursor.
5. **Database privileges:** new entry points revoke PUBLIC execution and use
   SECURITY INVOKER. Older routines 004–008 retain default grants/search paths.
   Audit actual role memberships, schema/table/sequence grants and helper
   execution rights in staging; provision separate admin/runtime roles with
   fixed search paths. No grant changes were applied to any service.
6. **Operational/concurrency evidence:** embedded PGlite is not multiple real
   PostgreSQL connections; local SQLite scenarios are not two Windows devices.
   Test simultaneous commits, session completion/revocation under load, lock
   timing, EAT midnight, interrupted networking, disk/backup recovery and
   conflicting edits in an isolated authorized environment.
7. **Abuse and operations:** establish failed-attempt throttling (the daily
   allowance counts completed/reserved sessions), safe metadata retention,
   redacted logs, alerts, backup restore and credential rotation procedures.
   Do not log bearer tokens, signing keys or business payloads.

## Dependency spot check

`src-tauri/Cargo.lock` pins `tauri` 2.11.5. The upstream
[GHSA-7gmj-67g7-phm9 advisory](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9)
lists 2.11.1 and later as patched, so this lockfile is outside that advisory's
affected range. This one check is not a complete current dependency audit;
repeat advisory and lockfile review for the actual release candidate.

## Disposition

The narrow panic-recovery fix is regression-tested in Windows CI. The staged
source remains closed by default. Open findings above require implementation
and/or deployment evidence before live readiness can be approved. Keep local
outbox history: synced rows are also the own-echo financial replay guard.
