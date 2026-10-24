-- 014_cloud_pull_cursor.sql
-- Last cloud event durably applied to this local SQLite installation.
-- The sync engine must update this value in the SAME SQLite transaction
-- that applies its corresponding event batch; never advance on download alone.
-- Separate from legacy Turso's timestamp-based last_synced_at.
INSERT OR IGNORE INTO sync_meta (key, value)
VALUES ('cloud_last_applied_sequence', '0');
