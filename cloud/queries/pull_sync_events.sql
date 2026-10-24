-- Task 21: incremental read of cloud events for one authenticated workspace.
-- PostgreSQL only; apply after cloud/007_sync_events.sql.
-- Bind $1 to the workspace verified by the trusted API, $2 to the last
-- DURABLY APPLIED event sequence (use 0 for initial synchronization),
-- and $3 to the batch size. Never trust a client-supplied workspace alone.
-- Use the last returned sequence as the next cursor ONLY AFTER all returned
-- events have committed successfully into the local SQLite database.
-- The sequence ordering guarantee depends on all writers using the
-- workspace row lock in upload_sync_event; direct inserts must be prohibited.
SELECT sequence_id,
       event_id,
       origin_installation_id,
       entity_type,
       entity_id,
       operation,
       payload,
       created_at
FROM sync_events
WHERE workspace_id = $1::uuid
  AND sequence_id > $2::bigint
ORDER BY sequence_id ASC
LIMIT LEAST(GREATEST($3::integer, 1), 500);
