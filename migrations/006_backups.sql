-- 006_backups.sql
-- Daily local Excel (.xlsx) backup of the operational data.
--
-- Independent of Turso sync — this is a plain file dropped on disk
-- (app_data_dir/backups/) so a restaurant never depends on a network
-- connection or a cloud account to recover its data. The Rust
-- `backup` module reads `settings.backup_enabled` on a timer and
-- writes one workbook per day, pruning old ones.
--
-- Depends on: 001_initial.sql (does not modify business tables)

PRAGMA foreign_keys = ON;

ALTER TABLE settings ADD COLUMN backup_enabled INTEGER NOT NULL DEFAULT 1;

-- ─────────────────────────────────────────────
-- Backup Meta (small key/value store for backup state)
--   e.g. 'last_backup_at', 'last_backup_path', 'last_backup_error'
-- Same shape as sync_meta (005_sync.sql) — a plain key/value table is
-- enough state for a once-a-day job, no need for a dedicated table.
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS backup_meta (
    key             TEXT PRIMARY KEY,
    value           TEXT,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER IF NOT EXISTS trg_backup_meta_updated_at
AFTER UPDATE ON backup_meta
FOR EACH ROW
BEGIN
    UPDATE backup_meta SET updated_at = datetime('now') WHERE key = OLD.key;
END;

INSERT OR IGNORE INTO backup_meta (key, value) VALUES ('last_backup_at', NULL);
INSERT OR IGNORE INTO backup_meta (key, value) VALUES ('last_backup_path', NULL);
INSERT OR IGNORE INTO backup_meta (key, value) VALUES ('last_backup_error', NULL);
