# Natra Restaurant — Project Status

Updated: 27 October 2026 (Africa/Addis_Ababa)

Branch: `feature/neon-sync-installation-id`
Review: [PR #10](https://github.com/hayal05/Natra-restaurant/pull/10), unmerged.

## Source progress

| Tasks | Delivered source work | State |
| --- | --- | --- |
| 1–35 | Installation identity, local outbox/replay, financial identity mapping, staged exchange, authentication and trusted provisioning | Implemented; Windows CI passed |
| 36 | Atomic scoped upload and validated desktop receipts | Implemented; Windows CI passed |
| 37 | Bounded incremental download and exact sequence cursors | Implemented; Windows CI passed |
| 38 | Explicit manual coordinator, gated native callbacks, shared lease, completion/failure and exact-origin CORS | Implemented; Windows CI passed |
| 39 | Server-authoritative per-installation allowance and EAT boundaries | Implemented; Windows CI passed |
| 40 | Offline detection, propagated cancellation and manual recovery | Implemented; Windows CI passed |
| 41 | Two independent SQLite installations: checkout, differing IDs, settlement, echoes and restart | Implemented; Windows CI passed |
| 42 | Scoped audit and abandoned-lease recovery after panic | Implemented; Windows CI passed |
| 43 | Release evidence, acceptance gates and rollout/recovery handoff | Source packet complete; production acceptance blocked |

Tasks 1–43 have source work or handoff material recorded. This does **not** mean
live production synchronization is ready. See
[Production readiness](cloud/PRODUCTION_READINESS.md) for the commit/build ledger
and remaining acceptance gates, and
[Security/reliability audit](cloud/SECURITY_RELIABILITY_AUDIT.md) for findings.

## Post-audit follow-up

The next source fix adds unapplied migration 015: PUBLIC execution is revoked
and trusted search paths with pg_temp last are pinned for all eleven sync
functions. Restricted-role and temporary-shadow regressions pass locally.
[Database privileges](cloud/DATABASE_PRIVILEGES.md) records role dependencies
and the remaining environment review. No live grants or gates changed.
The follow-up commit requires Windows CI before being treated as a build.

## Completed step — native sync authorization

Native credential-verified administrator sessions now own staged cloud leases.
Configuration and every local exchange callback check the current account.
Logout, reauthentication, account changes, expiry and panic recovery revoke old
leases. Frontend logout waits for native revocation; reload starts anonymous.
See [Native authorization](cloud/NATIVE_SYNC_AUTHORIZATION.md).

This step passed Windows CI `40378138213` at `1aacfd3`: 109 Node tests,
171 Rust tests and MSI/NSIS builds. Live synchronization remains disabled.

## Completed source step — token broker (2A)

The server half of secure token delivery adds a separately disabled broker,
digest-only provisioned credentials, approved native-user/cloud-subject mapping,
fresh scoped session issuance and credential revocation. SQL 016 is unapplied.
See [Credential broker](cloud/CREDENTIAL_BROKER.md). Step 2A passed Windows CI `40456734254` at `e4e4926`: 122 Node tests,
171 Rust tests and MSI/NSIS builds. The broker remains disabled.

## Current source step — native credential delivery (2B)

Windows Credential Manager storage, a separate stdin provisioning utility and
bounded native WinHTTP broker transport are implemented behind the closed gate.
Delivery is bound to the native login generation and exact lease; logout or
account/context changes prevent returned tokens from reaching the coordinator.
Refresh remains local-only. No plaintext/environment token fallback remains.
See [Native delivery](cloud/NATIVE_CREDENTIAL_DELIVERY.md). Initial native
commit `6f0bbef` passed Windows CI `40463541727` (124 Node and 175 Rust tests,
provisioning utility and MSI/NSIS builds). The follow-up one-attempt WinHTTP
policy and its native regression also require green CI before advancing.

The remaining work includes staging provisioning/role/issuer/TLS acceptance,
remote revocation policy, WebView/CSP/IPC hardening, financial-conflict recovery,
and operational/concurrency acceptance. Neither live gate was opened.

## Verification

Task 42 commit `1219c6e` passed Windows CI run `40364954977`: 101 Node tests,
170 Rust unit tests, MSI and NSIS builds. One illustrative Rust doc test is
ignored. Task 41 passed run `40363603378`; Task 40 passed `40352364014`.
The final Task 43 documentation commit also requires a green Windows workflow;
consult the exact commit's Actions result when selecting the latest build.

Tests use generated keys, mocked HTTP, embedded PostgreSQL and real local
SQLite databases. Task 41's ordered ledger is an in-process fixture. These
results do not establish live multi-connection PostgreSQL behavior, actual
two-device Windows operation or production security acceptance.

Stable event IDs, committed upload acknowledgements and atomic local
financial-row/receipt/cursor replay remain in place. Offline operation and
existing local financial calculations are preserved. No background sync was
introduced.

## Live state and blockers

- `LIVE_NEON_SYNC_ENABLED` remains false; new native writes reject and
  configuration returns None. All four Worker enablement flags remain absent.
- Native administrator/lease binding, broker mappings and OS token delivery
  are implemented in source; actual provisioning, issuer/role/TLS acceptance and
  remote revocation policy remain pending.
- WebView/CSP/IPC review, financial-conflict resolution, rejected-event repair,
  least-privilege grants and operational/concurrency evidence are pending.
- No cloud migrations, live credentials, signing keys or services were applied.
- Nothing was merged into main or deployed to production.

The next work is resolving the listed production blockers and collecting
staging acceptance evidence. Continue code work with separate commits and
Windows CI. Merge, deployment, live SQL/configuration and activation require
explicit authorization; they were not performed by Tasks 41–43.

## WebView hardening follow-up — stage 3A

Source now includes bundled-script CSP, denied main-window navigation/popups,
explicit local-main ACL coverage for all 60 commands, escaped stored text and
bounded raster portraits, and sandboxed receipt/table printing. Native delivery
also requires an exact data-origin CSP pin; none is currently configured.
See [WebView hardening](cloud/WEBVIEW_HARDENING.md). Windows CI must pass for
this commit before proceeding. Legacy business-command native session/role
authorization is the next stage (3B), followed by staging/runtime/printer and
other production acceptance. Live sync remains disabled.

Stage 3A source commit `0e1915f` passed Windows CI `40546264035`: 129 Node
security tests, six Edge scenarios (seven including the parent test), 178 Rust
unit tests, trusted provisioning utility, MSI and NSIS packaging. One illustrative
Rust doc test remains ignored. These fixtures do not certify actual packaged
WebView/physical-printer or live multi-device acceptance. Stage 3B is next.
