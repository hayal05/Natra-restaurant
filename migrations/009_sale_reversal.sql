-- 009_sale_reversal.sql
-- Lets a sale be reversed (voided) after the fact. Reversal is only
-- permitted within 24 hours of the sale — enforced in sales_service.rs,
-- not here — and a reversed sale is excluded from every revenue/cost/
-- profit calculation and from waiter receivables, but the row itself is
-- kept (not deleted) so the receipt still exists in history for audit.

ALTER TABLE sales ADD COLUMN is_reversed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sales ADD COLUMN reversed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_sales_reversed ON sales(is_reversed);
