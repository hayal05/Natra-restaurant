-- 001_initial.sql
-- Core tables: app settings, admin/user accounts, waiters, categories, items.
-- Everything else (sales, raw materials, expenses, sync) builds on this.

PRAGMA foreign_keys = ON;

-- ─────────────────────────────────────────────
-- App settings (single-row style key config, e.g. currency, sync toggle, restaurant name)
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1), -- enforce single row
    restaurant_name     TEXT NOT NULL DEFAULT 'My Restaurant',
    currency            TEXT NOT NULL DEFAULT 'USD',
    is_initialized      INTEGER NOT NULL DEFAULT 0,          -- 0 = needs first-run setup, 1 = done
    sync_enabled        INTEGER NOT NULL DEFAULT 0,          -- 0 = offline only, 1 = Turso sync on
    turso_url           TEXT,
    turso_auth_token    TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─────────────────────────────────────────────
-- Users (administrator / staff login accounts)
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    full_name       TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'staff')),
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─────────────────────────────────────────────
-- Waiters (selected during POS sales, accrue receivables)
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS waiters (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    full_name       TEXT NOT NULL,
    phone           TEXT,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─────────────────────────────────────────────
-- Categories (group items, e.g. Drinks, Grills, Desserts)
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS categories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,
    description     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─────────────────────────────────────────────
-- Items (Ready-Made or Cookable)
--   Ready-Made: purchase_cost is used directly for cost calculations.
--   Cookable:   purchase_cost is NULL; real cost comes from raw_materials (later migration).
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id     INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    type            TEXT NOT NULL CHECK (type IN ('ready_made', 'cookable')),
    purchase_cost   REAL,                       -- required for ready_made, NULL for cookable
    selling_price   REAL NOT NULL,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),

    CHECK (
        (type = 'ready_made' AND purchase_cost IS NOT NULL)
        OR
        (type = 'cookable' AND purchase_cost IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_items_category ON items(category_id);
CREATE INDEX IF NOT EXISTS idx_items_type ON items(type);
CREATE INDEX IF NOT EXISTS idx_waiters_active ON waiters(is_active);

-- Seed default settings row (id = 1) so the app always has one to read on first launch.
INSERT OR IGNORE INTO settings (id, is_initialized) VALUES (1, 0);
