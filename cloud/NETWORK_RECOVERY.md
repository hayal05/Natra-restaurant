# Task 40 — manual network recovery

The manual coordinator checks browser connectivity before starting. Offline
clicks do not read an outbox or call the network. Header state explains that
connectivity is needed; all other offline restaurant operations remain usable.
Browser connectivity is only a hint: request failures still fail closed.

An exchange owns an AbortController. Offline and login/session changes invalidate
the exchange and propagate cancellation to upload, download, allowance and
completion reads, while retaining each request's independent timeout. Before
acknowledgement/application, the controller also checks ownership/generation;
a fetch implementation that ignores cancellation cannot apply stale local work.
Request timers and external cancellation listeners are cleaned up after use.

Going online refreshes only local availability. It does not restart an exchange,
read a cloud page or upload in the background. Retry requires another explicit
manual click and a valid authorized session. Expired or failed session tokens
must be renewed by the approved host provisioner; tokens are not persisted or
silently reused past authorization. No credential renewal service is installed
by this change.

- A request lost after commit keeps the original event IDs for retry.
- Already acknowledged uploads stay acknowledged if a later step fails.
- Failed page application keeps the previous durable cursor.
- Cleanup skips server failure notification while offline; the local lease is
  still released and the server reservation can expire after 15 minutes.
- Recovery never creates replacement event IDs or advances a memory-only cursor.
- Reconnection during cleanup cannot permanently hide availability: a local
  availability refresh follows release, without network synchronization.

Tests cancel a real in-flight mock fetch through the propagated AbortSignal,
verify no acknowledgement/cursor movement, reconnect without network activity,
and then run a separate successful manual retry using the unchanged event ID.
Already-aborted upload/download/allowance/completion calls make zero fetches.
Existing lost-response and local-rollback tests remain in the suite.

The backend release gate and live server flags remain closed. No production
configuration, data, migrations or deployment were changed.
