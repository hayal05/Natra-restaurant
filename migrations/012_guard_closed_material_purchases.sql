-- Prevent raw-material purchase edits from changing closed-month valuations.
-- SQLite timestamps are UTC; date(...,'+3 hours') maps into Ethiopian business months.
CREATE TRIGGER trg_rm_closed_purchase_insert
BEFORE INSERT ON raw_material_purchases
WHEN EXISTS (
 SELECT 1 FROM raw_material_month_closings c
 WHERE c.status='closed'
 AND c.period_month=substr(date(COALESCE(NEW.purchase_date,datetime('now')),'+3 hours'),1,7)||'-01'
)
BEGIN SELECT RAISE(ABORT,'raw-material month is closed; reopen before recording purchases'); END;

CREATE TRIGGER trg_rm_closed_purchase_update
BEFORE UPDATE ON raw_material_purchases
WHEN EXISTS (
 SELECT 1 FROM raw_material_month_closings c
 WHERE c.status='closed'
 AND c.period_month IN (
   substr(date(OLD.purchase_date,'+3 hours'),1,7)||'-01',
   substr(date(NEW.purchase_date,'+3 hours'),1,7)||'-01'
 )
)
BEGIN SELECT RAISE(ABORT,'cannot modify raw-material purchases in a closed month'); END;

CREATE TRIGGER trg_rm_closed_purchase_delete
BEFORE DELETE ON raw_material_purchases
WHEN EXISTS (
 SELECT 1 FROM raw_material_month_closings c
 WHERE c.status='closed'
 AND c.period_month=substr(date(OLD.purchase_date,'+3 hours'),1,7)||'-01'
)
BEGIN SELECT RAISE(ABORT,'cannot delete raw-material purchases in a closed month'); END;
