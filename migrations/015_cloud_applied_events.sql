-- 015_cloud_applied_events.sql
-- Persistent idempotency receipts for cloud changes already applied locally.
-- Insert a receipt in the SAME SQLite transaction as the affected records and
-- cloud_last_applied_sequence; no receipt should be written on failed apply.
CREATE TABLE IF NOT EXISTS cloud_applied_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    sequence_id INTEGER NOT NULL UNIQUE CHECK (sequence_id > 0),
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
