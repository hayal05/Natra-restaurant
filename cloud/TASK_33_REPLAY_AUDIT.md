# Task 33 — Local cloud event replay completion audit

Status: **Local event replay implementation complete; live integration deliberately disabled.**

Baseline: commit `f33da810765a119f540fb5b48ddb35ceb07161db`.
Windows CI: [passed](https://github.com/hayal05/Natra-restaurant/actions/runs/40198112377).

## Implemented and verified by automated tests

- Durable mapping from incoming cloud entity IDs to device-local SQLite IDs; imported reference relationships do not rely on foreign devices' numeric row IDs.
- Transactional batch replay through `apply_cloud_event_batch`: business changes, event receipts and pull cursor commit together, or all roll back.
- Event routing to categories, items, waiters, raw materials, purchases, expenses, sales, reversals, waiter settlements and material-month closing lifecycle operations.
- Local outbox echo suppression, duplicate event/financial-record protections and rejection of unknown event types.
- Cross-device category/item mapping; waiter and item linkage for sales; imported sale/line monetary validation; purchases/expenses; duplicate transaction rejection; rollback on missing references or malformed input.
- Financial replay rejects mismatched update target IDs, invalid quantities, duplicate line IDs and arithmetic overflow.

## Scope boundaries and Task 34+ prerequisites

- **No Cloudflare/Neon network request path is wired to this dispatcher.** Windows CI checks local compilation/tests, not real-world multi-device interoperability.
- Task 34 must establish the end-to-end manual orchestration and ensure downloaded event IDs, sequences, targets and payloads are passed to the same atomic replay transaction. Preserve the existing offline-first startup and never start automatic synchronization.
- Before any live activation, reconcile the legacy Turso `sync/manager.rs` / `sync/turso.rs` path with the new cloud outbox/dispatcher; do not accidentally enable old last-write-wins handling of financial rows.
- Define shared versus installation-local settings/user credentials and keep password hashes, secrets and local auth state out of ordinary business events.
- Audit direct SQL mutation paths, backup restores and migrations for uncaptured changes; validate month-end lifecycle with multi-device scenario tests; add API authentication, server ordering, replay/rate limits and network retry tests.
- **Production deployment and live synchronization are explicitly out of scope** until separately authorized.

This marks Task 33 complete **for local replay implementation**, not production cloud synchronization readiness.
