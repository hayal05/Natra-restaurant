# Task 37 — incremental download

## Scoped server read

GET `/sync/pull?after=0&limit=100` is staged behind the exact server flag
`SYNC_PULL_ENABLED="true"`. No live flag is set. Cursor `after` is an unsigned
canonical decimal string within positive signed-64-bit range (zero starts a
new reader); limit is 1–500. Unknown/duplicate query parameters are rejected.
Workspace, installation and session always come from verified authentication.

`012_pull_authorized_sync_events.sql` locks workspace, device and session in
that order and checks active scope/expiry inside the read transaction. It reads
only that workspace's ledger, in ascending sequence order, strictly after the
cursor. Sequence gaps are valid. One lookahead row proves another page exists;
it is not returned as applied data and never advances the returned cursor.

Pages are capped by count and approximately 2 MiB of canonical event data;
the API also checks the exact encoded response limit of 2 MiB. An oversized
first event fails closed instead of skipping it. The database transport may
carry one extra lookahead event. All sequence/cursor values remain decimal
strings, including values beyond JavaScript's safe-number range.

The response shape is `{events, next_cursor, has_more}`. The next cursor is the
last returned sequence; an empty page retains the input cursor. Events include
ledger origin metadata and the existing event envelope/payload. Own events
remain in the stream: the existing SQLite outbox-echo guard records their
receipt/cursor without replaying their financial effect.

## Staged desktop transport and atomic local apply

`src/js/sync/cloud_download.js` requires explicit HTTPS endpoint, bearer token,
`readCursor` and `applyPage` callbacks. It rejects redirects, omits cookies,
bounds reads, cancels after 30 seconds and validates count, sequence ordering,
unique event IDs, origins and cursor agreement before calling local apply.

Task 38 will bind the existing Rust `apply_download_page` helper. That helper
commits business rows, receipts and cursor together; a failed replay rolls all
of them back. There is no independent cursor setter in this transport. The
client reads the durable cursor after application and requires agreement with
the page cursor. Failed download/application retries from the same durable
cursor, never a memory-only cursor. Browser origin/CORS and credential delivery
remain Task 38 configuration/integration work.

The Rust wire parser now accepts exact decimal sequence strings and legacy
positive integer JSON values. It rejects malformed, nonpositive or overflowing
values and does not convert through floating point. The existing atomic replay
and own-event handling are unchanged.

## Verification and operating state

Tests run actual PostgreSQL scope/pagination SQL in ephemeral PGlite, mock HTTP,
and exercise large cursors through API and desktop callbacks. A Rust regression
test applies a string sequence above JavaScript's safe range to SQLite and
verifies the exact durable cursor. Production multi-connection concurrency and
real service integration remain later checks.

During a separately authorized deployment, review/apply 012 after 007 and grant
execution only to trusted API roles. PUBLIC execution is revoked. No migrations,
live credentials, service deployment, UI wiring or live flags are applied by
this source implementation. Neither staged desktop module is loaded by HTML.
