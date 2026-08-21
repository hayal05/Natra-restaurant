-- 005_sync.sql
-- Sync infrastructure for optional Turso cloud synchronization.
-- The app works fully offline without this; when sync is enabled
-- (settings.sync_enabled = 1), the Rust sync/ module drains this queue.
--
-- Conflict resolution strategy: last-write-wins, using each table's
-- existing `updated_at` column — no extra per-row versioning needed.
--
-- Depends on: 001_initial.sql (does not modify business tables)

PRAGMA foreign_keys = ON;

-- ─────────────────────────────────────────────
-- Sync Queue (outbox)
--   Every local create/update/delete on a syncable table is queued here.
--   `payload` holds a JSON snapshot of the row at write time so the sync
--   manager can push it without re-reading (and possibly missing) the
--   current state. Rows are marked 'synced' once Turso confirms, or
--   'failed' with an error message for retry/inspection.
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_queue (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    table_name      TEXT NOT NULL,               -- e.g. 'sales', 'items', 'expenses'
    record_id       INTEGER NOT NULL,             -- primary key of the affected row
    operation       TEXT NOT NULL CHECK (operation IN ('insert', 'update', 'delete')),
    payload         TEXT,                          -- JSON snapshot of the row (NULL for delete)

    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'synced', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,

    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    synced_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_queue_status ON sync_queue(status);
CREATE INDEX IF NOT EXISTS idx_sync_queue_table_record ON sync_queue(table_name, record_id);

-- ─────────────────────────────────────────────
-- Sync Meta (small key/value store for sync state)
--   e.g. 'last_synced_at', 'device_id', 'last_pull_cursor'
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_meta (
    key             TEXT PRIMARY KEY,
    value           TEXT,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS trg_sync_meta_updated_at
AFTER UPDATE ON sync_meta
FOR EACH ROW
BEGIN
    UPDATE sync_meta SET updated_at = datetime('now') WHERE key = OLD.key;
END;

-- Seed baseline sync state so the app always has something to read.
INSERT OR IGNORE INTO sync_meta (key, value) VALUES ('last_synced_at', NULL);
INSERT OR IGNORE INTO sync_meta (key, value) VALUES ('device_id', NULL);
