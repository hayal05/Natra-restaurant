# Task 39 — per-installation daily synchronization allowance

The authoritative allowance is ten completed/reserved sessions per installation
on an Africa/Addis_Ababa calendar day. It is not tied to local user accounts or
the device's clock. A still-running session across midnight holds one slot;
failed/expired attempts are not completed usage. The existing reservation lock
prevents overlapping sessions for one installation.

`014_authorized_sync_allowance.sql` supplies a scoped, read-only allowance.
The exact flag `SYNC_ALLOWANCE_ENABLED="true"` is required for GET
`/sync/allowance`; the flag remains unset. Authentication binds the session,
installation and workspace; query-based scope overrides are rejected.

The response distinguishes completed usage, an active reservation, remaining
future slots and whether this particular session may run. The tenth reservation
has zero additional slots remaining but may still finish successfully. A
completed session reports usage without permitting another business exchange.
The controller checks eligibility before transfer and refreshes usage after
completion. The header labels the value as last confirmed; it never invents
an offline reset or charges a local user-side counter. Availability refreshes
still do not start network I/O; allowance is fetched only during a manual run.

Reservation/completion routines now capture `clock_timestamp()` after acquiring
locks, use one captured time for expiry/day boundaries and record it on new
reservations/completions. Reapply reviewed 004/005 definitions if an older
version exists during an authorized deployment. No live schema was changed.

Tests exercise exact 21:00 UTC / midnight EAT boundaries, the tenth session,
completion retry, previous-day/failed usage, independent installations, stale
reservation recovery, scoped API access and refusal before business transfer.
They use embedded PostgreSQL and mocked HTTP, not real concurrent service
connections. The historical Task 15 completed-count query remains informational.

No release gates, credentials, live allowance flag or production deployment
were enabled. Provisioning fresh session credentials, real service testing and
production activation remain separately controlled work.
