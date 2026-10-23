# Task 30 — Local outbox coverage audit (read-only)

Audited branch: `feature/neon-sync-installation-id`. This is a static source audit, **not** a runtime or end-to-end sync test.

## Finding

`migrations/005_sync.sql` creates `sync_queue`. `src-tauri/src/sync/queue.rs` implements `enqueue()` plus tests, but **no production enqueue calls were found in the business-service files inspected below**. As a result, the existing outbox does **not currently provide reliable capture of business mutations**. Do not enable uploads or present sync as complete.

| Business operation | Mutation path(s) inspected | Outbox write in inspected path? |
| --- | --- | --- |
| Sales and line items | `services/sales_service.rs` (`sales`, `sale_items`) | No |
| Sales reversals | `services/sales_service.rs` (`sales.is_reversed`) | No |
| Waiter settlements | `services/waiter_service.rs` (`sales.is_settled`) | No |
| Expenses | `services/financial_service.rs` (`expenses`) | No |
| Raw-material setup and purchases | `services/raw_material_service.rs` (`raw_materials`, `raw_material_purchases`) | No |
| Month-end draft, closing and reopen | `services/month_closing_service.rs` (`raw_material_month_closings`, `raw_material_month_closing_lines`, `raw_material_closing_audit`) | No |
| Menu categories and items | `services/inventory_service.rs` (`categories`, `items`) | No |
| Waiter creation and activation | `services/waiter_service.rs` (`waiters`) | No |

## Implementation caveats

1. Queue capture must be added **in the same SQLite transaction** as each real business mutation, not after commit; otherwise a crash can permanently lose events or produce orphaned outbox entries.
2. The legacy queue uses a local auto-increment row ID and local integer `record_id`, plus `insert/update/delete` operations. The new PostgreSQL ledger uses UUID `event_id`, `upsert/delete`, and globally comparable entity identities. These formats need an explicit conversion and globally unique ID strategy; local auto-increment IDs from different POS installations can collide.
3. Sales, line items, settlements, reversals, inventory costs and finalized month-end figures require **domain-specific merge rules**. Do not use last-write-wins for financial data.
4. Verify trigger-driven, secondary and bulk mutations when adding outbox capture. A sale may affect multiple records that must sync together.
5. `sync/queue.rs` tests exercise enqueue directly; they do not prove that POS mutations enqueue automatically. Existing sync manager is not evidence of a working Neon upload.
6. This audit covers the named main Rust business-service paths and legacy queue; it does not establish exhaustive coverage of every command, trigger, import or maintenance path.

## Suggested next microtask

Add a single, transaction-aware outbox helper accepting a stable UUID event ID, without activating uploads. Then connect one nonfinancial mutation path and test that business-row write and outbox receipt commit or roll back together. Audit all financial flows before enabling production sync.
