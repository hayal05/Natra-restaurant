-- 003_raw_materials.sql
-- Raw materials used for cookable items. Costs are tracked independently
-- of sales — NOT linked per-dish/recipe. They feed the profit formula as
-- an aggregate cost stream:
--   Profit = Sales - Ready-Made Costs - Raw-Material Costs - Other Expenses
--
-- Depends on: 001_initial.sql

PRAGMA foreign_keys = ON;

-- ─────────────────────────────────────────────
-- Raw Materials (catalog, e.g. Chicken, Rice, Oil, Spices)
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS raw_materials (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,
    unit            TEXT NOT NULL DEFAULT 'unit',  -- e.g. kg, liter, piece, box
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ─────────────────────────────────────────────
-- Raw Material Purchases (the actual cost events)
--   Each purchase is a standalone cost record — this is what feeds the
--   "Raw-Material Costs" figure in the profit formula and reports.
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS raw_material_purchases (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    raw_material_id INTEGER NOT NULL REFERENCES raw_materials(id) ON DELETE RESTRICT,

    quantity        REAL NOT NULL CHECK (quantity > 0),
    unit_cost       REAL NOT NULL CHECK (unit_cost >= 0),
    total_cost      REAL NOT NULL,              -- quantity * unit_cost, stored for reporting

    supplier        TEXT,
    purchase_date   TEXT NOT NULL DEFAULT (datetime('now')),
    note            TEXT,

    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_rm_purchases_material ON raw_material_purchases(raw_material_id);
CREATE INDEX IF NOT EXISTS idx_rm_purchases_date ON raw_material_purchases(purchase_date);

CREATE TRIGGER IF NOT EXISTS trg_raw_materials_updated_at
AFTER UPDATE ON raw_materials
FOR EACH ROW
BEGIN
    UPDATE raw_materials SET updated_at = datetime('now') WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_rm_purchases_updated_at
AFTER UPDATE ON raw_material_purchases
FOR EACH ROW
BEGIN
    UPDATE raw_material_purchases SET updated_at = datetime('now') WHERE id = OLD.id;
END;
