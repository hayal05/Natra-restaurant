# Credential provisioning and revocation acceptance

Source implementation and a reviewable staging procedure. No live credentials,
services, SQL or enablement have been changed. Actual staging evidence is still
required; these fixtures do not complete the production acceptance gate.

## Source changes

Migration 017 follows 001–016 and records revoked credential session IDs in a
non-secret revocation table. Revoking a grant records *all* its mapped sessions,
including completed/failed ones, and still fails only running sessions. Repeated
revocation is idempotent. Existing revoked grants are backfilled during migration.
Completed ledger rows, session timestamps and quota accounting remain intact.
Use the approved revocation routine; direct edits to credential mappings/flags
are outside this protocol. Migration/backfill requires quiesced traffic.

Upload, pull, complete, fail and allowance SQL entry points check that table
inside their existing scope/device/session lock sequence. A broker-issued token
can remain cryptographically valid but loses access to all five routes when its
credential revocation commits. Operations already admitted ahead of revocation
may finish first. Actual multi-connection lock behavior still requires staging.

The data runtime needs only SELECT on the new non-secret table and EXECUTE on
assert_sync_session_not_revoked(uuid), in addition to reviewed existing rights.
The revocation administrator needs INSERT on that table. Credential digests,
subjects and signing material remain outside the data runtime. All new/changed
routines are SECURITY INVOKER, deny PUBLIC execution and pin trusted search paths.
No privileges are granted automatically. Review schema ownership/default grants
and separate role membership before applying SQL.

Sessions issued by the separate legacy administrator CLI have no credential
mapping and are unaffected by a credential-specific revocation. Device
revocation and issuer/key retirement are separate controls. Native logout revokes
local callback authority; it does not contact the server or remotely revoke an
already issued token. Do not equate local logout with server-side bearer erasure.
Revocation records must be retained while any mapped token may still be accepted;
this change performs no pruning, deletion or credential-table rewrite.

## Trusted provisioning packet preparation

cloud/admin/provisioning-packet.js reads one bounded JSON envelope from stdin:

- grant: the six fields returned by the trusted credential-registration tool.
- approval: independently verified credential_id, workspace_id, installation_id,
  local_user_id, subject, broker_endpoint, sync_origin, issuer and audience.

The five identity fields must agree exactly (UUID case is normalized). Native
user IDs remain decimal strings, including values above JavaScript's safe integer
range. Endpoint and UTF-8 size checks match the native packet constraints. Extra
fields, interactive input, oversized input and command arguments are rejected.
Successful stdout is *only* the secret version-1 packet, at most 2560 UTF-8 bytes.
Errors are generic and do not echo input. This tool has no database/network/store
access and is not an approval authority.

Run it on a trusted operator host with an approved secure input/output channel:
`node cloud/admin/provisioning-packet.js` (JSON on stdin, secret packet on stdout).
Do not print successful output in a console, chat, log, CI artifact or shell
argument, or leave unencrypted temporary copies. No signing/database secrets go
to the customer machine. Transfer the packet through the separately approved
secure operator channel; under the intended Windows account, feed it to the
existing `natra-sync-provision.exe --install` stdin. It refuses overwrite.

## Reproducible offline evidence

`node --test cloud/tests/provisioning_acceptance.test.js`

The test creates fresh RSA keys and opaque grants in memory, applies SQL to
embedded PostgreSQL, and exercises actual JWT verification, broker/data handlers
and their database adapters. It checks packet binding, bounded stdin and secret-
free errors, prior-revocation backfill, successful upload/pull/completion retries,
all-route denial for revoked running/completed tokens, transactional checks with
stale status, preserved ledger/quota, unrelated-session isolation, explicit
rotation and runtime checks without credential-table access. No network is used.
Windows CI also reruns existing native store/transport tests and builds the
trusted utility and MSI/NSIS packages. Synthetic native store tests use a separate
test namespace; no actual customer credential is provisioned by this suite.

## Staging acceptance procedure — requires separate authorization

1. Approve the isolated staging workspace/devices/native users, operator, exact
   broker/data/issuer/JWKS origins, audience, role owners and secret channel.
   Record public identifiers and expected outcomes only; never record secrets.
2. Quiesce staging issuer/revocation/data traffic. Review/apply SQL 001–017 with
   the approved migration role before restarting traffic. Verify separate
   enrollment/revocation, broker and data runtime grants, including the new
   revocation helper/table. Run denied-role tests using actual separate clients.
3. Configure the approved staging signer/JWKS and TLS endpoints. Pin the exact
   data origin in desktop CSP and run a new Windows build. Enable only the
   specifically approved staging configuration. Current checked-in gates stay off.
4. Register/review the mapping on the issuer host. Prepare the packet through
   the trusted tool and approved channel; install under the correct Windows
   account. Confirm wrong-account/mapping/endpoint and overwrite attempts fail.
5. On two staging Windows installations verify explicit fresh-session issuance,
   upload/pull, completion retry, logout/account changes during issuance, 15-minute
   expiry, credential expiry, no redirects/proxy leaks, TLS/certificate failures
   and no raw secrets in logs or WebView IPC. Review response ambiguity without
   automatic registration/reservation retries.
6. Revoke a mapped grant remotely. Reuse its still-unexpired running and completed
   tokens: upload, pull, complete, fail and allowance must deny. Check broker
   renewal denies, committed history/quota remain, and unrelated sessions work.
   Race transfer/completion against revocation on separate real connections.
7. Rotate deliberately: confirm remote revocation, remove the old local entry,
   register/review and securely install a replacement, then obtain a fresh session.
   Confirm the old secret/tokens remain denied. An ambiguous remote response must
   be inspected/reconciled before removing/reissuing; do not assume success.
8. Exercise signer/JWKS rotation and retirement/cache behavior under the pinned
   issuer/audience, lost-device response, interrupted provisioning and recovery.
   Record source SHA, Windows run, role/migration identifiers and redacted pass/
   fail results. Remove only owned staging credentials/resources as approved.

Stop acceptance on failure. Do not merge, deploy, apply live configuration or
activate live synchronization merely because source/CI tests pass. Packaged
runtime/printer, financial-conflict, ledger-repair and operational gates remain.
