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

The uploader preserves stable outbox IDs, bounds batch/request sizes, validates
receipts against the exact submitted snapshot, and acknowledges only committed
batches. Tests cover a response lost after commit and a retry without duplicate
ledger entries. Existing local financial calculations are unchanged.

## Remaining work

| Task | Scope | Status |
| --- | --- | --- |
| 37 | Incremental download | Source implemented |
| 38 | Connect Sync Now, durable outbox callbacks, credentials and WebView origin/CORS | Next |
| 39 | Daily synchronization limit integration | Pending |
| 40 | Network recovery | Pending |
| 41 | Multi-device testing | Pending |
| 42 | Security and reliability audit | Pending |
| 43 | Production readiness | Pending |

## Validation

The local API/client/database suite has 77 passing tests. Embedded PostgreSQL,
generated test keys and mocked HTTP are used; no live database is contacted.
Windows CI runs these tests, Rust tests and both MSI/NSIS builds for each source
change. The Actions result for the exact commit is authoritative; Task 37 must pass
Windows CI before starting Task 38.

## Operating state

- Live synchronization is disabled; no upload flag is set in deployment config.
- The staged desktop upload/download modules are not loaded by index.html or connected to UI.
- SQLite read/ack callback binding and Sync Now integration remain Task 38.
- No cloud migrations, live credentials, signing keys or services were applied.
- Nothing was merged into main or deployed to production.
- Production configuration and live/concurrency checks remain pending.

Continue subtasks sequentially, commit each and verify Windows CI before moving
on. Preserve financial behavior and offline operation. Merging main, deploying
or enabling live synchronization requires explicit user approval.
