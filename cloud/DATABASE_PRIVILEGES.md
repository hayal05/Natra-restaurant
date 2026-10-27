# Post-audit follow-up — Sync function privileges

`015_harden_sync_function_privileges.sql` is an unapplied PostgreSQL migration
for review after SQL 001–014. It hardens all eleven sync functions, including
legacy reserve/complete/fail/upload helpers and the day-boundary helper:

- Revoke PUBLIC execution without removing existing explicit role grants.
- Explicitly retain SECURITY INVOKER; no function acquires owner privileges.
- Fix `search_path` to `pg_catalog, public, pg_temp`, placing temporary tables
  after trusted relations. Function bodies and business data are unchanged.
- Apply these changes in one transaction; reapplying 015 is safe.

The regression creates restricted PostgreSQL roles, tests actual permission
failures, exercises explicitly granted issuer/runtime access, and checks a
complete upload/retry/pull/completion/allowance cycle with shadow temporary
tables and a hostile caller search path. PGlite supplies the database; this
does not establish production Neon grants or multiple-connection behavior.

## Deployment prerequisites — not performed

Only an explicitly authorized operator may apply this migration as the function
owner. First review SQL 001–015 and inventory existing function grants, owners,
role memberships, schema privileges, table grants and sequence grants. Preserve
a backup and a grant inventory. Apply the reviewed migration chain before
accepting service traffic; do not serve requests between legacy function
creation and privilege hardening. Re-running an older CREATE OR REPLACE file
can overwrite the pinned search path; reapply and verify 015 after such changes.

`public` must be controlled by trusted migration owners. Untrusted users and
runtime/issuer roles must not have CREATE there, nor own its tables/functions.
This migration deliberately does not alter database-wide schema grants or
existing explicit role memberships; those require environment-specific review.
Do not treat a fixed search path as sufficient when an untrusted role can
replace objects in one of its trusted schemas.

Grant execution selectively to restricted server-only roles after review:

| Trusted role | Entry points / helpers | Underlying access needed by current functions |
| --- | --- | --- |
| Enrollment administrator | `enroll_sync_device` | Workspace SELECT/UPDATE for key-share locking; device SELECT/INSERT/UPDATE for enrollment and row locks |
| Session issuer | `reserve_authorized_sync_session`, `reserve_sync_session`; `fail_sync_session` for issuance cleanup | Device SELECT/UPDATE; session SELECT/INSERT/UPDATE |
| Sync API runtime | `upload_sync_batch`, `upload_sync_event`, `pull_authorized_sync_events`, `finish_authorized_sync_session`, `complete_sync_session`, `fail_sync_session`, `authorized_sync_allowance`, `sync_day_bounds` | Workspace/device/session SELECT/UPDATE for row locks and lifecycle; ledger SELECT/INSERT and identity sequence USAGE |

All roles require USAGE on the schema. The table describes function dependencies,
not a copy-paste grant script or proof of least privilege in a deployed database.
Restrict access to actual tables/columns where validated in staging; grant no
unneeded DELETE, DDL, role creation or owner/superuser rights. Do not grant these
roles to desktop users or distribute database credentials in the client.

Because functions are SECURITY INVOKER, a role executing a wrapper also needs
execution rights on its called helpers and the required table rights. The
trusted backend is therefore a security boundary: its database role can call
those helpers directly and may have direct table access. These grants do not
provide row-level isolation from a compromised backend or an untrusted SQL
client. Keep JWT/workspace/session validation in the API and transaction SQL;
any different database trust model requires its own design and tests.

Before live acceptance, verify with actual separate roles/connections that
unauthorized calls fail, approved calls succeed, PUBLIC has no execution rights
on these eleven functions, no untrusted schema CREATE or role inheritance grants
remain, SECURITY INVOKER is retained and `pg_temp` is last in each function's
search path. Configure deployment-owner default privileges for future routines
and review new migrations separately; 015 only hardens the named existing ones.

## References

PostgreSQL documents invoker/definer behavior, function execution defaults and
trusted-schema search paths in [CREATE FUNCTION](https://www.postgresql.org/docs/current/sql-createfunction.html).
Its [schema guidance](https://www.postgresql.org/docs/current/ddl-schemas.html)
explains why CREATE access to a searched schema must be restricted.

No live migration, grant, role, credential, merge, deployment or activation was
performed by this source follow-up. Other production blockers remain recorded
in `SECURITY_RELIABILITY_AUDIT.md` and `PRODUCTION_READINESS.md`.

## Credential-session revocation source (017)

Migration 017 adds SECURITY INVOKER assert_sync_session_not_revoked(uuid), with
PUBLIC execution revoked and pg_temp last. Review EXECUTE plus SELECT on the
non-secret sync_revoked_sessions table for the trusted data runtime; its existing
upload/pull/terminal/allowance functions call that helper. The revocation role
also needs INSERT on that table. Runtime credential-table access is not required.
No grants are automatically applied. See [acceptance](PROVISIONING_ACCEPTANCE.md).
