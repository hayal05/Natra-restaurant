-- Task 35: revalidate an authenticated caller's session against server state.
-- PostgreSQL only, after cloud/003_sync_sessions.sql.
-- Bind $1/$2 to workspace/installation IDs from verified authentication,
-- and $3 to the verified session ID. Never derive authorization from request
-- body fields. Exactly one returned row is required; no row means deny.
-- Run this check in the same transaction as the protected operation, keeping
-- these locks until commit to prevent concurrent deactivation/completion.
-- This query does not authenticate bearer tokens or activate synchronization.
SELECT s.id AS session_id,
       d.workspace_id,
       d.installation_id
FROM sync_sessions AS s
JOIN sync_devices AS d ON d.installation_id = s.installation_id
WHERE d.workspace_id = $1::uuid
  AND d.installation_id = $2::uuid
  AND s.id = $3::uuid
  AND d.deactivated_at IS NULL
  AND s.status = 'running'
  AND s.completed_at IS NULL
  AND s.started_at <= statement_timestamp()
  AND s.started_at >= statement_timestamp() - interval '15 minutes'
FOR SHARE OF d, s;
