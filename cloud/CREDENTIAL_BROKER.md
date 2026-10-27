# Token delivery step 2A — Trusted credential broker source

Updated 27 October 2026. This is the server half of token delivery/renewal.
Native Windows storage and transport now have a separate closed-gate source
implementation: [Native delivery](NATIVE_CREDENTIAL_DELIVERY.md). The desktop
does not contact this broker while the release gate is closed. Nothing was deployed, provisioned, merged or activated.

## Trust and enrollment

A restricted administrator approves a mapping of workspace, installation,
local native user ID and cloud subject. Local IDs, passwords, hardware IDs and
restaurant logins alone are not cloud authorization. No HTTP/desktop route can
create a mapping or choose a subject/workspace. Operator approval of the mapping
is the trust boundary; it must be verified against the actual installation and
intended account before delivering any credential.

`admin/sync-credential-admin.js` provides these issuer-host-only forms:

```text
node cloud/admin/sync-credential-admin.js create WORKSPACE_UUID INSTALLATION_UUID LOCAL_USER_ID SUBJECT
node cloud/admin/sync-credential-admin.js revoke WORKSPACE_UUID INSTALLATION_UUID CREDENTIAL_UUID
```

Do not execute them against a live service without separate approval. They use
server-only `NEON_DATABASE_URL`. Create generates a 32-byte random opaque bearer
credential, prefixed `natra_sync_`, and sends only its SHA-256 digest to PostgreSQL.
Successful stdout contains the raw secret once and must be handled as a secret;
it must never enter chat, logs, command arguments, source or frontend storage.
Do not run issuer administration or distribute database credentials/signing
keys on a customer desktop. The separate native utility supplies the OS storage path; actual approved
operator delivery and staging provisioning remain pending.

The registered credential expires after 30 days. One non-revoked credential per
installation/native user is allowed. Creation never silently replaces an
existing credential, even when it has expired. Rotation is explicit: approve
revocation, then create and securely provision its replacement. An ambiguous
registration response can leave an unreturned grant; inspect/revoke it before
retrying rather than blindly creating another identity.

## Separate broker service

`broker-worker.js` is a dedicated, trusted issuer. It is not imported by the
sync/data Worker and has no browser CORS support. POST `/session` requires HTTPS,
no Origin header, an opaque bearer credential, JSON content type and this exact
body (positive PostgreSQL bigint user IDs are decimal strings):

```json
{"installation_id":"22222222-2222-4222-8222-222222222222","local_user_id":"1"}
```

It accepts no caller-selected workspace, subject, session UUID or TTL. Bodies
are capped at 2 KiB and 64 chunks. Both a common edge limiter key and a credential
digest key must pass before issuance. Missing/broken limiters fail closed.
Responses use no-store, and errors omit key/database details. Header/origin
checks complement possession of the provisioned secret; they do not authenticate
an installation by themselves.

Configure only during a separately approved deployment:

| Issuer-only binding | Purpose |
| --- | --- |
| `SYNC_BROKER_ENABLED` | Exact `true` enables the broker handler |
| `SYNC_BROKER_RATE_LIMITER` | Mandatory Workers rate-limiter binding |
| `NEON_DATABASE_URL` | Restricted, separate issuer-role connection |
| `SYNC_JWT_ISSUER`, `SYNC_JWT_AUDIENCE` | Pinned sync JWT issuer and audience |
| `SYNC_SIGNING_KEY_ID`, `SYNC_BROKER_SIGNING_KEY` | Issuer-only key ID and restricted PKCS#8 RSA secret binding |

`broker.wrangler.toml` supplies no routes, secret values, limiter or enablement
flag, and workers.dev exposure is false. The signing secret belongs only to
this trusted issuer service, never the data Worker or desktop bundle. Publish
its matching public key at the existing configured JWKS endpoint and review
rotation/access controls before deployment. The sync service verifies its JWTs
through the existing pinned issuer/audience/JWKS configuration.

Cloudflare's [rate-limiter documentation](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/)
describes approximate limits per location. The common `broker:global` key is
shared within that limiter's location, not a strict worldwide quota. Review
edge limits, abuse monitoring and service-capacity policy in staging; the
transactional ten-completed-sessions daily allowance remains authoritative.

## Atomic reservation, renewal and revocation

Unapplied SQL 016 follows 001–015. It adds digest-only credentials, a
credential/session mapping and three SECURITY INVOKER routines with PUBLIC
execution revoked and trusted search paths. Credential tables have no PUBLIC
grant. No migration is automatically applied by the broker or admin command.

Reservation derives scope from the credential record and rechecks it under
workspace/device/credential locks before invoking existing scoped session
reservation. Device locking serializes issuance with revocation; row locks
follow the existing operation order. A signed token binds the stored subject,
workspace, installation, credential ID, native user ID and a new server-generated
session UUID. It expires by the earlier of the 15-minute session deadline and
credential expiry. Signing rules are shared with the existing trusted CLI.

Each later explicit exchange requests a new session token, after the prior
session is completed, failed or expired. The broker never replaces an active
session or bypasses the daily allowance. There is no automatic reservation
retry or background renewal. An invalid/ambiguous reservation or signing
failure attempts cleanup only for that generated session ID; a network failure
can leave it running until timeout. Never reuse an old session/token as renewal.

Credential revocation is scoped, idempotent and marks only its mapped running
sessions failed. Subsequent transfers fail existing session checks; new issuance
fails. Completed sessions and quota history are retained, and unrelated
installations/credentials/legacy issuer sessions are untouched. A completed
session's already-issued token is also blocked from terminal retry and allowance
requests after source migration 017: all mapped sessions enter a non-secret
revocation table checked by all five API SQL entry points. This applies to
credential-mapped sessions; unrelated legacy issuer sessions remain unchanged.
See [Provisioning acceptance](PROVISIONING_ACCEPTANCE.md). Native logout remains
local-only and does not remotely erase bearer tokens.

## Role and release review still required

Use separate restricted enrollment/revocation, broker-issuer and data-runtime
roles. The broker role requires credential SELECT, mapping INSERT, and execution
on reservation/helpers plus the underlying workspace/device/session privileges.
Only the administrative role needs credential registration/revocation rights.
Do not give the data-runtime role credential-table access or issuer keys.
Grant only reviewed table/column/sequence rights and keep public schema CREATE
restricted to trusted owners. See `DATABASE_PRIVILEGES.md` for invoker-model
limits; these are trusted server roles, not untrusted direct-SQL clients.

Native implementation must store the provisioned opaque credential in the OS
credential store, never expose it through WebView IPC, derive installation/user
identity from native authorization, perform bounded HTTPS without redirecting
credentials, and revalidate the native login generation before installing a
returned short-lived token. Losing authorization during issuance must not
install a token or authorize stale callbacks. Production delivery cannot be
claimed until that code, secure operator provisioning and actual staging tests
are complete. The other CSP, conflict and operational blockers remain open.

## Tests

Generated test keys, mocked HTTP and embedded PostgreSQL cover digest-only
registration, exact native IDs, fresh signed scopes, renewal after completion,
active-session rejection, foreign/expired/deactivated credentials, deadline caps,
revocation, explicit rotation, unrelated-session preservation, quota, PUBLIC
denial, ambiguous-response cleanup, strict request boundaries, closed gates,
mandatory throttling and ordinary sync-Worker isolation. These are source
regressions, not real multi-connection or live-issuer acceptance evidence.

Step 2B implements the native storage/transport requirements above in source,
behind the unchanged closed gate. This document remains the broker-side protocol;
see the native delivery document for current behavior and remaining acceptance.

## Provisioning/revocation follow-up

[Provisioning acceptance](PROVISIONING_ACCEPTANCE.md) documents migration 017,
the trusted stdin-only packet builder, generated-key offline lifecycle tests and
the separately authorized staging procedure. Apply through 017 and review new
runtime-helper/table privileges before staging. No live grants were changed.
