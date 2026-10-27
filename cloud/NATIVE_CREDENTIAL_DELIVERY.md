# Token delivery step 2B — Native Windows credential store and transport

Updated 27 October 2026. Source implementation only. The native release gate
and broker gate remain closed. No production credential, migration, route or
service was provisioned. This does not complete live acceptance or the other
security/financial-conflict blockers.

## Storage and trusted provisioning

The desktop reads an opaque credential from Windows Credential Manager using
a generic credential target scoped to installation and native user ID:

```text
NatraTechnology/Restaurant/Sync/v1/INSTALLATION_UUID/LOCAL_USER_ID
```

The target is derived by Rust from the current credential-verified administrator
session and SQLite installation identity. It cannot be selected through IPC.
Persistence is local-machine, same-Windows-account across logons, without
enterprise roaming. Generic credentials are available to other processes under
that Windows account; this is OS account protection, not app isolation from a
malicious local process/administrator. Use a protected Windows account.

A separate trusted operator utility, `natra-sync-provision`, installs or removes
a reviewed packet. It is not invoked by the app, registered as IPC or wired to
a frontend credential form. Secret input is bounded stdin; stdout/error messages
never echo it. Install refuses to overwrite an existing entry; rotation requires
explicit server revocation, local removal, and approved replacement provisioning.
Removing a local entry alone does not remotely revoke already issued tokens.

```text
natra-sync-provision --install
natra-sync-provision --remove INSTALLATION_UUID LOCAL_USER_ID
```

For install, pipe the approved packet through stdin and close the input stream.
Interactive terminal input is refused to prevent console echo of secrets.
Do not put the credential in command arguments, chat, logs, shell history,
plaintext files, settings or localStorage. Run under the same Windows account
that runs the restaurant app, not a different elevated operator account. The
restricted issuer-host grant administration from step 2A must already have
approved the actual installation, native account and cloud subject. This tool
does not infer approval, register grants or carry database/signing credentials.

Packet schema (placeholders are illustrative; no usable credential is supplied):

```json
{
  "version": 1,
  "credential_id": "44444444-4444-4444-8444-444444444444",
  "workspace_id": "11111111-1111-4111-8111-111111111111",
  "installation_id": "22222222-2222-4222-8222-222222222222",
  "local_user_id": "1",
  "subject": "APPROVED_CLOUD_SUBJECT",
  "credential": "OPAQUE_SECRET_FROM_APPROVED_ISSUER",
  "broker_endpoint": "https://APPROVED_BROKER/session",
  "sync_origin": "https://APPROVED_SYNC_ORIGIN",
  "issuer": "https://APPROVED_ISSUER/",
  "audience": "APPROVED_SYNC_AUDIENCE"
}
```

The full UTF-8 packet must fit 2560 bytes. Unknown/duplicate fields, malformed
IDs, noncanonical positive bigint user strings, invalid secrets, unsafe URL
components and wrong installation/user bindings are rejected. Use the exact
lowercase UUIDs and subject/issuer/audience issued/configured on the server.
Approved endpoint metadata is a security boundary: an operator must verify it
before installation. Endpoints require HTTPS on port 443, with no userinfo,
query or fragment; the broker path is exactly `/session` and the data origin
has no path. No environment-token or plaintext-settings fallback remains.
Non-Windows platforms fail closed instead of falling back to plaintext storage.

## Manual delivery and native ownership

Availability refresh reads only local metadata. It never reserves a session,
contacts the broker or returns an opaque credential/access token. Only an
explicit Sync Now run acquires a native lease and invokes `cloud_sync_credentials`.

Rust captures the authorized installation, native user and login generation,
allows one delivery attempt per lease, then drops SQLite/session locks before
OS/network work on a blocking worker. A process-wide delivery guard prevents
parallel broker calls even if an old lease is ended during a request. After the
response, Rust checks the current account, login generation, lease identity and
expiry again, then re-reads the approved credential metadata before returning a
short-lived configuration. Changed/logout/expired contexts cannot install a
token or revive a lease. Restoring account rows is not a fresh login.

The opaque credential stays native. Only the short-lived session JWT and
approved data endpoints cross IPC, in memory, for the existing data transport.
The coordinator checks cancellation/owner and the approved origin after awaiting
delivery. It performs no business reads/transfers if delivery fails or becomes
stale. There is no background renewal; a later explicit exchange requests a
fresh broker session. No native or frontend token cache is persisted.

## HTTPS boundary

Native broker calls use WinHTTP with no proxy discovery/configuration, cookies,
automatic authentication or redirects, one connection attempt, and TLS 1.2/1.3 with Windows
certificate/hostname validation. No certificate-error bypass or custom trust
root is installed. There is no automatic application reservation retry. A POST
contains only native installation/user IDs; the provisioned bearer credential
is sent only to the approved `/session` endpoint. HTTP 200 JSON/no-store is
required; redirects and other statuses are rejected. Encoded responses are not
accepted. Headers/body are bounded; body size is at most 16 KiB and 64 reads.

WinHTTP resolve/connect/send/receive timeouts are five seconds per operation.
An elapsed 20-second budget is checked between operations/reads; an already
blocking operation can take up to its remaining per-operation timeout. This is
a bounded native operation, not a hard 20-second end-to-end guarantee. No
SQLite/session lock is held during it. An ambiguous request can leave a server
reservation until its 15-minute timeout; it is never blindly retried. Losing
local authorization does not prove that a network request was cancelled or
that the server did not reserve a session.

Response fields and JWT claims must match the provisioned credential, subject,
workspace, installation, exact native user ID, issuer/audience, session UUID
and deadline. Lifetime is at most 15 minutes and tokens near expiry are refused.
This parsing is a consistency check, not native JWT signature verification;
local trust is the approved HTTPS broker, and the data service still verifies
RS256 signatures and current session authorization. Source credential/response
buffers are zeroized where owned; Windows/HTTP internals and returned short-lived
WebView tokens are outside that memory-erasure claim. Secrets are never logged.

## Verification and remaining acceptance

Node tests cover metadata-only refresh, delivery after begin, rejected/changed
origins and logout before business transfer. Rust tests cover packet/response
binding/deadlines, native login/lease race rejection, and an isolated synthetic
Windows credential-store read/write/remove roundtrip. A native policy test
constructs the HTTP handles without sending, checking the connection-attempt
count, response-header bound and successful explicit TLS/request-option setup. The TLS protocol
option is set-only; the test does not attempt to query that unsupported value. Test entries use a separate
`.../Sync/Test/` namespace and are removed, including on test failure. No actual
server credential is provisioned in CI. The provisioning utility is compiled
with the Windows build and included separately from the MSI/NSIS executables.

Physical two-device Windows, TLS/redirect/proxy/certificate failure scenarios,
secure operator delivery, deployed issuer/JWKS/roles, remote cancellation/bearer
revocation policy and CSP/IPC acceptance still need separately approved staging
work. Native logout blocks stale local use but is not a universal remote token
revocation action. The app continues to have one process-wide native principal;
additional windows require a separate ownership review. Do not open either live
gate before the remaining release criteria are satisfied.

API references: [Windows CREDENTIALW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw),
[WinHttpSetOption](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpsetoption).

## WebView policy prerequisite

Stage 3A also requires the data endpoint's exact HTTPS origin in the bundled
CSP connect-src before native credential availability/delivery. No remote origin
is currently pinned. Broker requests remain native; approved data transport
uses WebView fetch. See [WebView hardening](WEBVIEW_HARDENING.md). Legacy business
IPC authorization and packaged runtime acceptance remain production blockers.
