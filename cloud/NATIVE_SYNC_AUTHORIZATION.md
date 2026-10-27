# Native authorization for staged manual synchronization

This source step adds native login ownership to the staged Neon commands.
`LIVE_NEON_SYNC_ENABLED` remains false. No live credential, configuration,
cloud migration, merge or deployment is included.

## Authorization policy

Only a currently active local administrator may read staged sync configuration
or use an exchange lease. Staff can still log in and use existing local flows,
but cannot access this cloud-sync surface. The backend reads the current SQLite
account on every configuration/begin/pending/acknowledge/cursor/apply/end call;
frontend user objects, role strings and user IDs are not accepted as authority.

The existing credential verifier and account lockout behavior remain the source
of login verification. Successful native login or first-admin setup establishes
an in-memory principal with a random generation, the verified user's identity,
a password-hash fingerprint and an eight-hour absolute monotonic deadline.
No password, hash or native generation is returned as a new session credential,
and none is persisted to settings/localStorage. User responses continue to omit
the existing password_hash field through serialization.

A lease belongs to that native login generation, not just its user ID. A new
login (including the same account), failed reauthentication, logout, expiry or
poisoned session mutex discards the previous authority and lease. Deactivation,
role change, password-hash change or account deletion is detected before the
next protected callback and revokes authority. Restoring an account row does
not restore the native login. A fresh credential-verified login is required.
An expired lease alone does not end an otherwise valid native login.

## Concurrency and logout

All protected command paths lock SQLite first, then the bridge/session mutex.
The mutex remains held through authorization and local callback work, including
configuration reads; logout cannot finish between the check and that work.
Logout itself needs no SQLite lock. No mutex is held across network I/O.
Revocation drops the shared process guard without resetting pending uploads,
acknowledged events, receipts, cursor or financial rows. In-flight network data
may reach the server, but callbacks from the old lease cannot acknowledge or
apply locally. Preserve stable IDs for reconciliation on a later exchange.

The frontend explicitly invokes native logout and waits for success before
clearing the user and navigating to login. Failure shows an error instead of
presenting logout as complete. WebView bootstrap also revokes any retained
native principal before creating the UI, so a reload does not inherit an old
window's login. Session state is process-wide for this single-window app;
additional windows would require a separate ownership/navigation review.

## Verification

Rust regression coverage exercises anonymous/staff denial, first-admin setup,
credential-verified login, same-account reauthentication, logout, old callbacks,
failed login, account changes, expiry, deletion, panic recovery and unchanged
pending events/cursor after denied acknowledgements. Existing lease cutoff,
exact cursor, overlap and callback tests now establish native login first.
Node tests verify logout ordering and that a failed native logout does not
clear the frontend session. Windows CI compiles the Tauri command wiring and
runs the full Rust/Node suites and MSI/NSIS builds.

## Limits and next step

This protects the staged cloud-sync commands. It is not a claim that every
legacy IPC command has native access control or that the WebView is hardened.
CSP/injection/IPC review remains a separate blocker. This source code does not
protect against an attacker who can alter the local database or desktop process.

The disabled server-side [credential broker](CREDENTIAL_BROKER.md) now provides
a source path for fresh issuance and mapped-session revocation.
Native secure fresh-token delivery now has a separate closed-gate
[source implementation](NATIVE_CREDENTIAL_DELIVERY.md); actual provisioning
and physical acceptance remain pending. The environment-token adapter has been removed. Approved broker mappings
and native OS storage/transport now exist in source; actual operator delivery,
rotation, issuer/role configuration and remote revocation acceptance remain
separate work. Local logout prevents further native use; it does not
revoke an already issued server bearer token. Server signatures and current
workspace/device/session checks remain authoritative for cloud requests.

The eight-hour native session limit is an upper bound for this sync surface,
not a change to local financial/business behavior or a background logout timer.
All live activation gates remain closed until the remaining blockers are met.
