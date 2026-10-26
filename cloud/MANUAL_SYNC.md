# Task 38 — manual Sync Now integration

## Manual exchange

The header's existing Sync Now button is wired to a single coordinator. Merely
loading the app, refreshing availability or editing local data does not start
network traffic. The source release gate remains closed, so the button stays
unavailable and all new state-changing IPC commands reject execution.

For an approved, configured release, a click:

1. Validates one HTTPS service origin and obtains a backend exchange lease.
2. Checks that the provisioned token claims the local installation ID. This is
   a consistency check; server JWT signature verification remains authoritative.
3. Uploads the initial outbox backlog using stable IDs and committed receipts.
4. Downloads ordered pages, applying rows/receipts/cursor atomically via SQLite.
5. Confirms session completion with the server and releases the local lease.
6. Only then reports Synced. Errors report failure and attempt session cleanup.

The backend lease shares the existing Turso process guard. Only one exchange
can run; wrong/expired lease IDs are rejected. Each click fixes a rowid cutoff,
so newer local edits wait for the next manual cycle. Acknowledgements must be
an exact prefix of the staged snapshot and are committed through the existing
SQLite helper. Page application requires the expected durable cursor. No
SQLite mutex is held during network I/O. Abandoned leases expire after 15
minutes; reloaded windows cannot hold the process guard forever.

The coordinator has bounded batch/page loops. It does not continuously chase
new local writes. User/session changes invalidate the current run, preventing
subsequent local acknowledgement or application. Accepted uploads remain
acknowledged even if a later pull fails; failed page application retains the
previous durable cursor. Token delivery/renewal is host provisioning work.

## Configuration and gates — not activated

`LIVE_NEON_SYNC_ENABLED` is a Rust constant set to false. There is no frontend
switch or IPC credential setter. Configuration returns None and IPC writes
reject while this gate is false. Changing it requires explicit approval and
an approved build/configuration review.

For a future approved host provisioner, the configuration adapter reads
`NATRA_NEON_SYNC_ORIGIN` and `NATRA_NEON_SESSION_TOKEN` from the process environment.
The latter must be a short-lived session token issued for this installation.
No tokens are saved to localStorage, settings, config files or the repository.
The environment is a provisioning adapter, not an identity provider or renewal
service; production user/role binding and fresh-session delivery remain setup
and security-review work. No environment values were set by this change.

Worker flags `SYNC_UPLOAD_ENABLED`, `SYNC_PULL_ENABLED` and
`SYNC_SESSION_CONTROL_ENABLED` must each be the exact string `true` in an
approved deployment. All are absent from wrangler.toml. There is no change to
the old optional Turso configuration or independent backup/archive schedules.

## Session completion/failure

POST `/sync/complete` and `/sync/fail` authenticate the signed session scope,
accept no caller-selected scope/action body and revalidate ownership inside
`013_finish_authorized_sync_session.sql`. The routine reuses existing completion
and failure rules. Completion retry is idempotent even if its first response
was lost after commit. Failure never reverses a completed session or its quota.
Expired/future sessions cannot be completed successfully. SQL must be reviewed
and applied separately after 005/006; PUBLIC execution is revoked.

## Browser origins

`SYNC_ALLOWED_ORIGINS` is a JSON array of exact allowed browser origins, supplied
only during an approved deployment. Windows WebView's production origin is
normally `http://tauri.localhost`; confirm the actual runtime origin before
configuration. Wildcard and opaque `null` origins are rejected. Preflights allow
only the defined route's method and Authorization/Content-Type headers. Cookies
are not enabled; responses vary by Origin. Origin filtering complements JWT
verification and is not authentication. No allowed-origin setting is installed
by this change.

## Verification and remaining work

Local tests cover manual ordering, large backlogs, failed pulls, overlap,
logout, foreign installation tokens, missing completion confirmation, header
states, exact-origin CORS and actual PostgreSQL terminal-session rules. Rust
regression tests exercise the lease, snapshot cutoff, atomic local callbacks,
exact cursors and abandoned-lease recovery. Windows CI must pass this source
commit before advancing.

Daily allowance presentation/integration, further network recovery, real
multi-device/concurrency checks and security/production review remain Tasks
39–43. The deployed system, live database and live synchronization were not
changed. This is source integration behind closed release gates.
