-- Historical completed-count report. Runtime allowance uses 014 and also
-- accounts for the active session reservation; this query is not enforcement.
-- Task 15: read-only daily sync allowance, per installation.
-- PostgreSQL only; depends on cloud/003_sync_sessions.sql.
-- Bind $1 to the authenticated installation UUID (not an arbitrary client ID).
-- Use fixed UTC+03:00 boundaries, so resets happen at midnight in Ethiopia.
-- This is informational; later API tasks must enforce the cap atomically.
WITH today AS (
    SELECT
        (date_trunc('day', now() AT TIME ZONE 'Africa/Addis_Ababa') AT TIME ZONE 'Africa/Addis_Ababa') AS day_start,
        ((date_trunc('day', now() AT TIME ZONE 'Africa/Addis_Ababa') + interval '1 day') AT TIME ZONE 'Africa/Addis_Ababa') AS next_day_start
),
usage AS (
    SELECT count(*)::integer AS completed_count
    FROM sync_sessions s
    CROSS JOIN today t
    WHERE s.installation_id = $1::uuid
      AND s.status = 'completed'
      AND s.completed_at >= t.day_start
      AND s.completed_at < t.next_day_start
)
SELECT completed_count AS used_today,
       greatest(0, 10 - completed_count) AS remaining_today,
       10 AS daily_limit
FROM usage;
