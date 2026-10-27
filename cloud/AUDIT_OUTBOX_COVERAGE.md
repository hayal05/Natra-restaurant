# Task 32 — Cloud outbox coverage review

Reviewed branch `feature/neon-sync-installation-id` after commit `68b23bf9`.
Latest Windows CI: **passed** (run 40010659485). This verifies compilation and unit tests, **not live multi-device sync**.

## Covered in production service paths

All operations below enqueue an event in the **same SQLite transaction** as their local business changes. These paths have unit tests covering event capture and/or rollback.

| Business operation | Service | Captured event |
| --- | --- | --- |
| Category creation | inventory_service | categories |
| Item creation, activation, pricing | inventory_service | items |
| Waiter creation, activation | waiter_service | waiters |
| Expense creation | financial_service | expenses |
| Raw-material creation, purchase recording | raw_material_service | raw_materials, raw_material_purchases |
| Sales checkout + all sale items | sales_service | sales (one bundled financial event) |
| Sale reversal | sales_service | sale_reversals |
| Waiter settlement | waiter_service | waiter_settlements |
| Month closing: draft, finalization, reopen | month_closing_service | material_month_closings |

## Gaps to resolve BEFORE connecting live upload/download

1. **Cross-installation IDs and relationships:** Current events identify locally created entities with `installation_id:local_id`. Several payloads still carry local integer foreign keys. Download/apply must translate them through durable identity mappings, and updates of *downloaded* records must retain the original entity ID rather than accidentally attributing them to the editing installation. This is not implemented yet.
2. **Financial event replay:** Remote application must make sales + lines, reversals, settlements, expenses, purchases and month-end changes exactly-once or idempotent as applicable; validate related entities and apply domain-specific rules. A cloud event UUID alone does not make local financial mutations safe.
3. **Month closing draft/reopen snapshots:** Reopen currently sends an action/reason rather than the full closing lines; remote apply logic must handle that state transition and audit correctly. Month-closing lines and audits are not independently captured.
4. **Settings and users:** `settings_service` changes restaurant profile and device sync/backup settings; `auth_service` changes local authentication state. No new Neon outbox capture was added to these paths. Define which fields are workspace-shared versus installation-local before syncing. **Never upload passwords, password hashes or cloud credentials as ordinary business events.**
5. **Other write paths:** Command-layer direct SQL, backup restores, migrations and database triggers were not exhaustively audited. Do not claim total coverage until these are checked.
6. **Not connected:** Actual upload, incremental pull, remote application, auth, session allowance enforcement in the live API, and the active Sync Now click path are not yet integrated.

## Practical next step

Before enabling cloud synchronization, add minimal durable mapping of cloud entity IDs to local IDs and use it consistently for remote records and subsequent edits. Reuse the existing outbox and pull cursor; avoid a second queue, background scheduler, or new backend.
