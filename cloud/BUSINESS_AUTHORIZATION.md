# Native business authorization — stage 3B

All 48 registered business IPC commands now require the existing native login
session. Only first-run status/setup, login/logout and the separately gated
cloud lease API use their own authorization paths. JavaScript identity and role
values do not grant native authority.

| Access | Commands |
| --- | --- |
| Active admin or staff | checkout, list_sales, list_waiters, list_categories, list_items, get_settings |
| Active administrator | Every other business command: catalog/waiter changes, settlement, reversal, purchases, expenses, month closing, financial reports, settings, installation identity, sync queue, backups and archive exports/downloads |
| Retired sync paths | enable_sync and sync_now require admin and then reject; direct Turso IPC cannot bypass the closed manual Neon lease path |

The staff POS responses retain existing data shapes, including sale and catalog
fields; this is a command-access policy, not field-level financial redaction.
The sole administrator setup and its existing operational workflows remain.
No staff account-management feature or automatic account migration is added.

Checkout overwrites any caller-supplied user_id with the verified native user.
Financial pricing, costs, totals, receipts, month closing and offline outbox
algorithms remain in their existing services.

Every synchronous business wrapper locks SQLite first, then holds a native
session authorization guard through the operation. No business or filesystem
work precedes authorization. Logout waits for an admitted operation to finish;
commands admitted after logout fail. Account deletion, inactivity, changed
password/role or the existing eight-hour absolute expiry revoke the session and
cloud lease; restoring an account row does not restore the old session. A valid
staff session survives an ordinary administrator-permission denial. No session
mutex crosses an await or network call. Backup/archive helpers reuse the locked
connection to avoid recursive locking. Scheduled local backups remain independent
of interactive login. Long local exports can delay logout until they finish.

Staff navigation lands on POS, hides administrative pages/reversal actions and
redirects direct administrative hash routes. Native expiry clears the UI user
and settings and returns to login. Delayed errors from an older login do not
clear a newer UI login. Frontend routing is an affordance; native guards enforce
permissions even if frontend state is changed.

Regression coverage: all registered business callbacks must have a native guard
before work; legacy sync paths cannot enable/run; role/UI/error generation tests;
Rust account revocation, denied access, cashier attribution/financial totals and
logout serialization; Edge staff-login/routing/session-expiry scenarios. Windows
CI must pass for the exact commit. Actual packaged WebView and physical printer,
secure staging provisioning, remote revocation and other release acceptance
remain open. No live SQL, credentials, enablement, deployment or merge is done.
