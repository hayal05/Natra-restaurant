# Task 36 — incremental upload

## Server implementation

The POST `/sync/upload` implementation is staged behind the exact server flag
`SYNC_UPLOAD_ENABLED="true"`. This flag is absent from wrangler.toml and has
not been configured in any live environment. Pull remains unimplemented.

The authenticated identity supplies workspace, installation and session IDs.
The body is strictly `{ "events": [...] }` with 1–500 events and a 1 MiB request
limit. Each event contains only `event_id`, `entity_type`, `entity_id`,
`operation` and object `payload`. Supported upsert types match the local
replay dispatcher; delete events and unknown types are rejected. An optional
payload origin must match the authenticated installation. This boundary
validates envelopes/scope; financial payload semantics are also checked by
the existing local replay handlers. It is not a new financial calculation.

`011_upload_sync_batch.sql` reuses the existing idempotent event-ledger writer.
It locks workspace, device and session in a consistent order and rechecks the
session with current database time inside the write transaction. It rejects
foreign, deactivated, failed, completed and expired sessions. Any event failure
rolls back all newly inserted events in that request. A retry with the same
immutable event IDs/content returns the original sequence IDs. Changed content
under an existing event ID is a conflict, not a silent overwrite.

A successful response is:

```json
{
  "acknowledged_event_ids": ["event UUID"],
  "receipts": [{"event_id": "event UUID", "sequence_id": "64-bit integer as decimal string"}]
}
```

The response is emitted after commit and preserves request order. Timeout or
connection failure returns no acknowledgement: retain the original event IDs
for a later retry. The ledger may already have committed, so never manufacture
new IDs for a retry. Sequence gaps after a rolled-back transaction are normal.

The existing source outbox reads oldest-first and marks only acknowledged IDs
synced. Desktop HTTP transport/receipt validation is the next Task 36 piece;
connecting the user-facing Sync Now action remains Task 38.

## Deployment and tests

No cloud migration is applied automatically. During a separately authorized
deployment, review/apply 011 after 007/008 and restrict routine execution to a
trusted API role. PUBLIC execution is revoked. Do not enable the flag, add live
credentials or deploy without explicit approval.

Tests execute the actual routine in ephemeral embedded PostgreSQL and exercise
API limits, committed receipts, retry safety, transactional rollback, scope,
revocation and SQL permissions. They do not establish real multi-connection
concurrency behavior; that remains later multi-device/reliability testing.
