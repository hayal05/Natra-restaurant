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
synced. The staged WebView fetch transport validates every receipt against the exact
submitted batch before calling its acknowledgement callback. Connecting these
callbacks to the Tauri outbox commands and user-facing Sync Now action remains
Task 38. No script is loaded into index.html and no request starts automatically.

## Deployment and tests

No cloud migration is applied automatically. During a separately authorized
deployment, review/apply 011 after 007/008 and restrict routine execution to a
trusted API role. PUBLIC execution is revoked. Do not enable the flag, add live
credentials or deploy without explicit approval.

Tests execute the actual routine in ephemeral embedded PostgreSQL and exercise
API limits, committed receipts, retry safety, transactional rollback, scope,
revocation and SQL permissions. They do not establish real multi-connection
concurrency behavior; that remains later multi-device/reliability testing.

## Staged desktop transport

`src/js/sync/cloud_upload.js` exposes `uploadPendingBatch` for Task 38 to call
with explicit HTTPS endpoint, bearer token, installation ID and durable
`readPending`/`acknowledge` callbacks. The callbacks must wrap the existing
SQLite `read_pending_uploads` and atomic `acknowledge_uploads` helpers; no
replacement queue/storage is introduced here. WebView origin/CORS and credential
provisioning are also part of the later connection/configuration work.

It snapshots at most 500 oldest-first events, fits the request within 1 MiB,
and refuses to skip an oversized first event. Empty queues make no request.
Fetch omits cookies, rejects redirects, has a 30-second cancellation timeout
and bounds response reads to 256 KiB. Receipts must match the exact submitted
IDs in order, include valid 64-bit sequence strings and contain no duplicates.
A failed request, malformed/partial receipt or failed local acknowledgement
leaves the same IDs available for retry. API errors are surfaced generically.

Envelope validation is shared between client and server without importing any
Neon or issuer code into the desktop. The SQL routine permits 2 MiB of canonical
jsonb text because PostgreSQL formatting adds spaces; the HTTP/client request
limit remains 1 MiB. This avoids rejecting a valid compact request solely for
canonical formatting overhead.

The integration test sends the desktop request through the API into embedded
PostgreSQL, loses the first response after commit, then retries the unchanged
IDs and confirms that exactly one copy of each event exists.
