# Natra Restaurant — Project Status

Updated: 26 October 2026 (Africa/Addis_Ababa)

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
- Secure fresh-token delivery/renewal and native user/role binding are pending.
- WebView/CSP/IPC review, financial-conflict resolution, rejected-event repair,
  least-privilege grants and operational/concurrency evidence are pending.
- No cloud migrations, live credentials, signing keys or services were applied.
- Nothing was merged into main or deployed to production.

The next work is resolving the listed production blockers and collecting
staging acceptance evidence. Continue code work with separate commits and
Windows CI. Merge, deployment, live SQL/configuration and activation require
explicit authorization; they were not performed by Tasks 41–43.
