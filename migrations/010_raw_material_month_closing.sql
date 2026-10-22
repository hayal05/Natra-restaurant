-- 010_raw_material_month_closing.sql
-- Month-end stock valuation ledger. Never modifies historical purchases.
-- period_month is the calendar month YYYY-MM-01; closing date basis is
-- Ethiopia local calendar time (Africa/Addis_Ababa, UTC+03).
-- The balance's closing values become the NEXT month's opening values,
-- not new purchases. Profit recognition is implemented in a later task.
CREATE TABLE raw_material_month_closings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    period_month TEXT NOT NULL UNIQUE
        CHECK (length(period_month) = 10 AND period_month GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-01'
               AND substr(period_month, 6, 2) BETWEEN '01' AND '12'),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','closed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    closed_at TEXT,
    CHECK ((status = 'draft' AND closed_at IS NULL)
        OR (status = 'closed' AND closed_at IS NOT NULL))
);

CREATE TABLE raw_material_month_closing_lines (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    closing_id INTEGER NOT NULL REFERENCES raw_material_month_closings(id) ON DELETE CASCADE,
    raw_material_id INTEGER NOT NULL REFERENCES raw_materials(id) ON DELETE RESTRICT,
    -- Opening quantities and value are copied from the previous closing,
    -- rather than recorded as new purchases.
    opening_quantity REAL NOT NULL DEFAULT 0 CHECK (opening_quantity >= 0),
    opening_value REAL NOT NULL DEFAULT 0 CHECK (opening_value >= 0),
    purchased_quantity REAL NOT NULL DEFAULT 0 CHECK (purchased_quantity >= 0),
    purchased_value REAL NOT NULL DEFAULT 0 CHECK (purchased_value >= 0),
    remaining_quantity REAL CHECK (remaining_quantity IS NULL OR remaining_quantity >= 0),
    closing_value REAL CHECK (closing_value IS NULL OR closing_value >= 0),
    UNIQUE(closing_id, raw_material_id),
    CHECK (remaining_quantity IS NULL OR remaining_quantity <= opening_quantity + purchased_quantity + 0.000001),
    CHECK ((remaining_quantity IS NULL AND closing_value IS NULL)
        OR (remaining_quantity IS NOT NULL AND closing_value IS NOT NULL))
);

CREATE INDEX idx_rm_closing_lines_material ON raw_material_month_closing_lines(raw_material_id);
CREATE INDEX idx_rm_closings_status_month ON raw_material_month_closings(status, period_month);
