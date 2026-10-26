# Natra Restaurant — Project Status

Updated: 26 October 2026

Branch: `feature/neon-sync-installation-id`

## Implemented source work

- Tasks 1–35: implemented foundations, identity mapping, transactional local
  replay, staged manual exchange, authentication and trusted provisioning.
- Task 36: atomic session-scoped upload API and staged desktop fetch uploader.
  Server upload commit: `c5ad501` (Windows CI passed before the desktop piece).
  Desktop receipt handling and transport are included.
- Task 37: scoped incremental download, bounded lookahead pagination, exact
  cursor strings and staged desktop page-apply transport are implemented.
- Task 38: manual header coordinator, gated IPC callbacks, shared exchange
  guard, session completion/failure and exact-origin CORS are implemented.
- Task 39: authoritative per-installation allowance, EAT day boundaries,
  post-lock timing and last-confirmed header allowance are implemented.
- Task 40: offline detection, propagated cancellation and manual-only recovery
  retain pending events and durable cursors without background retry loops.
- Task 41: two independent SQLite installations exercise real checkout, stable
  identity mapping, settlement, own echoes, stale-page rejection and restart.

The uploader preserves stable outbox IDs, bounds batch/request sizes, validates
receipts against the exact submitted snapshot, and acknowledges only committed
batches. Tests cover a response lost after commit and a retry without duplicate
ledger entries. Existing local financial calculations are unchanged.

## Remaining work

| Task | Scope | Status |
| --- | --- | --- |
| 37 | Incremental download | Source implemented |
| 38 | Manual Sync Now source integration behind closed release gates | Source implemented |
| 39 | Daily synchronization limit integration | Source implemented |
| 40 | Network recovery | Source implemented |
| 41 | Multi-device testing | Source implemented |
| 42 | Security and reliability audit | Next |
| 43 | Production readiness | Pending |

## Validation

The local API/client/database suite has 101 passing tests. Embedded PostgreSQL,
generated test keys and mocked HTTP are used; no live database is contacted.
Windows CI runs these tests, Rust tests and both MSI/NSIS builds for each source
change. Task 40 Windows CI passed (`40352364014`). The Actions result for the
exact commit is authoritative; Task 41 must pass Windows CI before Task 42.

Task 41 uses actual SQLite databases and services with an in-process ordered
ledger fixture. It does not replace hardware or multi-connection PostgreSQL
concurrency testing; those remain required before live activation.

## Operating state

- Live synchronization is disabled; no upload flag is set in deployment config.
- The header has manual coordinator wiring; the backend release gate is false
  and configuration returns None, so no live network exchange can start.
- Approved host provisioning, role binding and fresh token delivery remain pending.
- No cloud migrations, live credentials, signing keys or services were applied.
- Nothing was merged into main or deployed to production.
- Production configuration and live/concurrency checks remain pending.

Continue subtasks sequentially, commit each and verify Windows CI before moving
on. Preserve financial behavior and offline operation. Merging main, deploying
or enabling live synchronization requires explicit user approval.
