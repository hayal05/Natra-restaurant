# Task 35 authentication and trusted provisioning

Current progress and release disposition: see [Project status](../Project%20status.md)
and [Production readiness](PRODUCTION_READINESS.md). Later-task references below
describe the state when this milestone was introduced.

Source implementation is ready for review. No live credentials, migrations,
identity-provider configuration or deployments have been applied. Upload and
pull return HTTP 501 by default after successful authentication/session validation.

## Verification in the sync Worker

The Worker verifies RS256 JWTs against a configured issuer's public JWKS.
It requires the expected issuer and audience, a valid signature, expiration,
a nonempty subject, UUID workspace/installation/session claims and a
space-separated scope containing `natra:sync`. It checks `nbf` when present.
Installation IDs and ordinary local restaurant logins alone grant no access.

JWKS retrieval uses only the pinned server URL, never token `jku` or `x5u`
headers. It has a five-second timeout and cached keys with rotation support.
Authentication errors return generic 401 responses; missing/invalid server
configuration returns 503. Current Neon state must also authorize the scope:
the device is active in the workspace and its session is running/unexpired.

| Server variable | Purpose |
| --- | --- |
| `SYNC_JWT_ISSUER` | Exact expected HTTPS issuer |
| `SYNC_JWT_AUDIENCE` | Audience dedicated to this sync API |
| `SYNC_JWKS_URL` | Pinned HTTPS public-key endpoint |
| `NEON_DATABASE_URL` | Server-only Neon connection secret |

Function-based verifier/validator overrides remain available for trusted
server composition and tests; request fields cannot set them. Future
financial operations must revalidate scope in their own database transaction;
preflight query locks do not survive its HTTP request.

## Trusted administration command

`cloud/admin/sync-admin.js` is a Node server-administration entry point. Its
trust boundary is restricted host/OS access plus provisioning credentials,
not the local desktop login. Only an operator authorized to administer the
workspace and grant the subject access may invoke it. No public HTTP route
or desktop action exposes provisioning. The workspace must already exist.

Command forms (placeholder IDs must be replaced with approved UUIDs):

```text
node cloud/admin/sync-admin.js --help
node cloud/admin/sync-admin.js enroll WORKSPACE_UUID INSTALLATION_UUID
node cloud/admin/sync-admin.js issue-token WORKSPACE_UUID INSTALLATION_UUID SUBJECT
```

Enrollment uses bound parameters and `009_enroll_sync_device.sql`. It is
idempotent for an active installation in the same workspace, preserves its
registration timestamp, refuses reassignment and refuses automatic
reactivation. The command does not create workspaces or infer permission
from a user-supplied hardware/installation ID.

Token issuance additionally requires these issuer-host settings:

| Issuer-only variable | Purpose |
| --- | --- |
| `SYNC_SIGNING_KEY_FILE` | Restricted PKCS#8 RSA private-key file outside the checkout |
| `SYNC_SIGNING_KEY_ID` | Key ID matching the issuer's public JWKS entry |

The issuer uses `SYNC_JWT_ISSUER`, `SYNC_JWT_AUDIENCE` and `NEON_DATABASE_URL`
as above. Provisioning the restricted key and publishing its matching public
JWKS at the configured HTTPS URL remain deployment configuration work. Never
put a signing key in the sync Worker, desktop bundle, repository or command
arguments. The signing module is imported only by the administration command.

`010_reserve_authorized_sync_session.sql` checks the workspace/device binding
under a device row lock and reuses `reserve_sync_session`: active-device
checks, one active session per installation and the ten-session daily limit
remain enforced. Tokens bind the subject, workspace, installation and a
server-generated session UUID. Their expiration is capped at the database
session's 15-minute deadline; the caller cannot choose a session ID or TTL.

If issuance fails, cleanup attempts to mark only that attempt's generated
session failed. An ambiguous network response can prevent cleanup; abandoned
sessions expire after 15 minutes. There is no automatic reservation retry.
Successful command output includes a bearer token and must be handled as a
secret; errors emit a generic message without database/key details.

New routines are SECURITY INVOKER and revoke PUBLIC execution. During an
authorized deployment, apply 009 after the device schema and 010 after 004;
005/006 must also exist for session lifecycle/issuance cleanup. Review grants
for trusted provisioning roles. No migration is applied by the command.

## Validation and remaining work

Tests use generated test keys, mocked HTTPS and ephemeral embedded PostgreSQL
(PGlite, a development-only dependency). They exercise enrollment, real
session SQL, quota checks, signature verification and both administration
actions without contacting live Neon or an identity provider. They do not
establish production concurrency behavior.

Before production: configure the trusted issuer, public JWKS, restricted
signing host and database roles; run authorized environment checks. Client token delivery, Sync Now integration and operational concurrency testing
remain later integration work; staged upload/download modules have separate unset
enablement flags. Live synchronization and
production deployment require separate authorization.
