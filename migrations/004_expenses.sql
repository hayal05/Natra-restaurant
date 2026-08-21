-- 004_expenses.sql
-- Other operating expenses (rent, utilities, salaries, maintenance, etc.)
-- Recorded independently of sales and raw materials. Feeds the
-- "Other Expenses" term in the profit formula:
--   Profit = Sales - Ready-Made Costs - Raw-Material Costs - Other Expenses
--
-- Depends on: 001_initial.sql

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS expenses (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    category        TEXT NOT NULL,               -- e.g. Rent, Utilities, Salaries, Maintenance, Other
    description     TEXT,
    amount          REAL NOT NULL CHECK (amount > 0),

    expense_date    TEXT NOT NULL DEFAULT (datetime('now')),
    note            TEXT,

    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_expenses_date ON expenses(expense_date);
CREATE INDEX IF NOT EXISTS idx_expenses_category ON expenses(category);

CREATE TRIGGER IF NOT EXISTS trg_expenses_updated_at
AFTER UPDATE ON expenses
FOR EACH ROW
BEGIN
    UPDATE expenses SET updated_at = datetime('now') WHERE id = OLD.id;
END;
