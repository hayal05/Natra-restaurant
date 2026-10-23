-- 016_cloud_outbox.sql
-- New Neon event outbox, independent of legacy Turso sync_queue.
-- Insert in the same SQLite transaction as the local business mutation.
CREATE TABLE IF NOT EXISTS cloud_outbox (
    event_id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL CHECK (length(trim(entity_type)) > 0),
    entity_id TEXT NOT NULL CHECK (length(trim(entity_id)) > 0),
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_type(payload) = 'object'),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'synced')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_cloud_outbox_pending
ON cloud_outbox(status, created_at);
