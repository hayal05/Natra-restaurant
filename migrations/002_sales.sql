-- 002_sales.sql
-- POS transactions: sale headers, sale line items, and waiter receivables.
-- Depends on: 001_initial.sql (users, waiters, items)

PRAGMA foreign_keys = ON;

-- ─────────────────────────────────────────────
-- Sales (one row per POS transaction / receipt)
--   total_amount, total_cost, total_profit are calculated and stored at
--   checkout time (not recomputed on the fly) so historical reports stay
--   accurate even if item prices/costs change later.
--
--   Waiter receivables: a sale is money the waiter collected on behalf of
--   the business. is_settled = 0 means it still counts as owed by the
--   waiter until reconciled (is_settled = 1, settled_at stamped).
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sales (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    waiter_id       INTEGER NOT NULL REFERENCES waiters(id) ON DELETE RESTRICT,
    user_id         INTEGER REFERENCES users(id) ON DELETE SET NULL,  -- who ran the POS

    total_quantity  INTEGER NOT NULL DEFAULT 0,     -- sum of sale_items.quantity
    total_amount    REAL NOT NULL DEFAULT 0,        -- sum of sale_items.line_total (sales value)
    total_cost      REAL NOT NULL DEFAULT 0,        -- sum of sale_items.line_cost (ready-made only; cookable = 0 here)
    total_profit    REAL NOT NULL DEFAULT 0,        -- total_amount - total_cost (raw material cost handled separately)

    payment_method  TEXT NOT NULL DEFAULT 'cash' CHECK (payment_method IN ('cash', 'card', 'other')),

    is_settled      INTEGER NOT NULL DEFAULT 0,     -- waiter receivable: 0 = still owed, 1 = settled/reconciled
    settled_at      TEXT,

    note            TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sales_waiter ON sales(waiter_id);
CREATE INDEX IF NOT EXISTS idx_sales_created_at ON sales(created_at);
CREATE INDEX IF NOT EXISTS idx_sales_settled ON sales(is_settled);

-- ─────────────────────────────────────────────
-- Sale Items (line items within a sale)
--   Snapshots item_name / item_type / unit_price / unit_cost at time of
--   sale so edits to the item catalog later don't rewrite past receipts.
--   unit_cost is only populated for ready_made items; cookable items
--   store unit_cost = NULL (their true cost is tracked via raw materials).
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sale_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    sale_id         INTEGER NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL REFERENCES items(id) ON DELETE RESTRICT,

    item_name       TEXT NOT NULL,                  -- snapshot
    item_type       TEXT NOT NULL CHECK (item_type IN ('ready_made', 'cookable')), -- snapshot

    quantity        INTEGER NOT NULL CHECK (quantity > 0),
    unit_price      REAL NOT NULL,                  -- snapshot of selling_price
    unit_cost       REAL,                            -- snapshot of purchase_cost (ready_made only, NULL for cookable)

    line_total      REAL NOT NULL,                  -- quantity * unit_price
    line_cost       REAL NOT NULL DEFAULT 0,         -- quantity * unit_cost (0 for cookable)

    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sale_items_sale ON sale_items(sale_id);
CREATE INDEX IF NOT EXISTS idx_sale_items_item ON sale_items(item_id);

-- ─────────────────────────────────────────────
-- Keep sales.updated_at fresh whenever a sale row changes
-- (e.g. when it gets settled).
-- ─────────────────────────────────────────────
CREATE TRIGGER IF NOT EXISTS trg_sales_updated_at
AFTER UPDATE ON sales
FOR EACH ROW
BEGIN
    UPDATE sales SET updated_at = datetime('now') WHERE id = OLD.id;
END;
