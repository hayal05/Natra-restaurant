# Task 43 — Release readiness handoff

Date: 26 October 2026 (Africa/Addis_Ababa)

**Disposition: NOT READY FOR LIVE ACTIVATION.** The staged source and closed-gate
Windows builds are available for review. Passing CI does not resolve the open
security, provisioning, conflict-policy and operational findings. Task 43's
source handoff is complete; production acceptance remains blocked.

## Candidate and evidence

Branch: `feature/neon-sync-installation-id`. Review: [PR #10](https://github.com/hayal05/Natra-restaurant/pull/10).
No merge, production deployment, cloud migration or live activation is authorized
by this handoff. The latest commit includes this packet and `Project status.md`;
use its exact Actions result when selecting a source snapshot or installer.

| Milestone | Source commit(s) | Passing Windows Actions run |
| --- | --- | --- |
| Tasks 1–35 source baseline | `3eb1e9a7be66a4d0a9de932dc513b0bade415d17` | [40323253833](https://github.com/hayal05/Natra-restaurant/actions/runs/40323253833) |
| 36 scoped upload, desktop receipts | `c5ad501`, `bb27c8b` | [40335135182](https://github.com/hayal05/Natra-restaurant/actions/runs/40335135182) |
| 37 incremental download | `c6c3ba7` | [40341610430](https://github.com/hayal05/Natra-restaurant/actions/runs/40341610430) |
| 38 manual coordinator / native bridge | `0bfad81`, fixture fix `4603d2d` | [40347871334](https://github.com/hayal05/Natra-restaurant/actions/runs/40347871334) |
| 39 authoritative EAT allowance | `032b769` | [40350195718](https://github.com/hayal05/Natra-restaurant/actions/runs/40350195718) |
| 40 offline cancellation / manual recovery | `59f8d2e` | [40352364014](https://github.com/hayal05/Natra-restaurant/actions/runs/40352364014) |
| 41 two SQLite installations / restart | `e845b17` | [40363603378](https://github.com/hayal05/Natra-restaurant/actions/runs/40363603378) |
| 42 source audit / panic recovery | `1219c6e` | [40364954977](https://github.com/hayal05/Natra-restaurant/actions/runs/40364954977) |

Task 42 CI passed 101 Node tests, 170 Rust unit tests and MSI/NSIS builds.
One illustrative Rust doc test is ignored; it is not counted as a pass.
Task 41's fixture used independent initialized SQLite databases, actual local
service writes and an in-process ordered ledger. API SQL tests use embedded
PGlite and generated test keys. None contact live Neon. See
[the audit](SECURITY_RELIABILITY_AUDIT.md) for evidence and test limits.

## Acceptance gates still open

| Gate | Required evidence before live release | Current state |
| --- | --- | --- |
| Native authorization | Backend user/role-bound lease; logout/revocation tests | Administrator-owned native session and callback checks added in source; live WebView acceptance pending |
| Provisioning | Approved signer/JWKS; secure fresh-session delivery and renewal; key rotation | Disabled broker and native OS delivery source added; actual provisioning and staging acceptance pending |
| WebView | CSP, stored-text injection, navigation and IPC least-privilege review on Windows | Stage 3A source and Edge fixtures added; legacy business-command authorization and packaged runtime/printer acceptance pending |
| Financial conflicts | Reviewed handling of two offline settlements/reversals and dependency conflicts; no silent financial skips | Resolution workflow pending |
| Ledger safety | Entity-specific validation and auditable rejected-event repair/quarantine | Production policy pending |
| Database roles | Separate admin/runtime role and helper/table/schema/sequence grant review | Source migration 015 and role regressions added; staging/live evidence pending |
| Concurrency | Multiple real PostgreSQL connections and two Windows installations; lock/revocation/midnight tests | Not run |
| Operations | Throttling, redacted monitoring, backup/restore drill, retention and key revocation drill | Not established |
| Release approval | Explicit approval of merge, deployment/configuration and activation | Not granted |

## Reproducible source and build review

Select an exact branch commit with green Windows CI. Check out that commit,
run `npm install --no-audit --no-fund`, then the explicit Node test command in
`.github/workflows/build-windows.yml`, `cargo test` in `src-tauri`, and
`npm run build` on Windows. Preserve the lockfiles, source SHA, Actions run,
artifact digest and test output with the release record. CI currently installs
from `package.json`; a locked clean-install/reproducibility review remains part
of release acceptance. Do not infer that an installer filename's PR merge SHA
is the branch source SHA; inspect the workflow head commit.

A source handoff must contain source plus `Project status.md` and this packet,
not just MSI/NSIS binaries. Exclude `node_modules`, Cargo target output,
credentials, private keys and local restaurant databases. Existing source ZIPs
through Task 35 remain historical snapshots; they do not include Tasks 36–43.

## Staging rollout sequence — instructions only, not executed

1. Resolve and record acceptance evidence above before changing any gate.
   Obtain explicit approval for a separate isolated staging service/database
   and test installations. Use synthetic restaurant data and restricted roles.
2. Back up SQLite and the cloud database; rehearse restoring both. Inventory
   workspace/installation identities, durable cursors and pending event IDs.
   A restored/cloned database must not silently become a second active device
   with the same installation identity; approve the clone/re-enrollment policy.
3. Review SQL 001–016 in order and least-privilege grants. Run them only in the
   separately approved environment. Provision server secrets/signing material
   outside source and desktop bundles; publish public verification keys only.
4. Configure exact issuer/audience/JWKS and confirmed WebView origins. Test bad
   signatures, wrong workspace/install, expired/revoked sessions, wrong origin,
   changed-content event IDs, oversized input and unavailable dependencies.
5. Use an approved release candidate with native identity binding and fresh
   token delivery. Enable only the explicitly approved environment's upload,
   pull, session-control and allowance flags and native gate. No ordinary UI
   setting should activate this staged implementation.
6. On two independent Windows installations, explicitly click Sync Now and
   verify catalog IDs, sale totals/receivables, settlement/reversal behavior,
   month closing, pending queues and cursors. Include both offline edits,
   interrupted replies, disk failure, logout, concurrent sessions, device
   deactivation and EAT midnight. Record expected versus actual outcomes.
7. Approve a limited production pilot only after staging evidence is accepted.
   Verify monitoring and restore readiness, record exact build/configuration
   and approve expansion separately. Never enable background sync implicitly.

## Stop and recover

Disable new sync requests/issuance and close release gates if authorization,
financial totals, ordering or durable cursor checks fail. Expire/revoke tokens
and deactivate affected installations as appropriate. Preserve logs without
secrets and retain the immutable ledger, receipts and original event IDs.
Do not delete synced outbox rows: local echo suppression depends on them.

Do not reset cursors, regenerate pending IDs, drop failed events or restore one
side independently to force progress. Acknowledged uploads may already be
committed even after a client error. Reconcile the exact event IDs, receipts
and financial records first; use a reviewed recovery/restore plan. Reopening a
gate or resuming a pilot requires accepted recovery evidence and approval.

Local-first operation remains the normal mode while the staged Neon path is
closed. Existing legacy Turso configuration and archive/backup behavior were
not repurposed as Neon provisioning.

## Post-audit source follow-up

Migration 015 hardens named function privileges/search paths without changing
business bodies. See [Database privileges](DATABASE_PRIVILEGES.md) for test
evidence and required environment review. This closes the source-level default
execution/search-path gap; the live-role acceptance gate remains open.

The native-authorization step adds administrator-owned login generations and
per-callback account checks. See [Native authorization](NATIVE_SYNC_AUTHORIZATION.md).
This does not supply secure cloud-token provisioning or remote bearer revocation.

Token delivery step 2A adds a separate disabled issuer broker and unapplied SQL
016. See [Credential broker](CREDENTIAL_BROKER.md). Native credential storage and broker transport (2B) follow in the source step
below; no live token delivery is activated.

Token delivery step 2B adds Windows Credential Manager and native WinHTTP source
wiring, with a separate trusted provisioning utility and login-generation checks.
See [Native delivery](NATIVE_CREDENTIAL_DELIVERY.md). No actual live provisioning
or activation was performed; physical staging acceptance remains required.
