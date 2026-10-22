-- Record deliberate reopening of a finalized material month.
-- Do not delete historical closing quantities or underlying purchases.
CREATE TABLE raw_material_closing_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    closing_id INTEGER NOT NULL REFERENCES raw_material_month_closings(id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK (action IN ('reopen')),
    reason TEXT NOT NULL CHECK (length(trim(reason)) >= 5),
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_rm_closing_audit_closing ON raw_material_closing_audit(closing_id);
