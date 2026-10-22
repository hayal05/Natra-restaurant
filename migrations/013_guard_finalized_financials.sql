-- Freeze financial inputs in closed months, while allowing waiter settlement
-- (which changes receivables, not historical profit).
CREATE TRIGGER trg_closed_month_sale_insert BEFORE INSERT ON sales
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month=substr(date(NEW.created_at,'+3 hours'),1,7)||'-01')
BEGIN SELECT RAISE(ABORT,'sale belongs to finalized accounting month'); END;
CREATE TRIGGER trg_closed_month_sale_financial_update BEFORE UPDATE OF created_at,total_amount,total_cost,is_reversed ON sales
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month IN (substr(date(OLD.created_at,'+3 hours'),1,7)||'-01',
                        substr(date(NEW.created_at,'+3 hours'),1,7)||'-01'))
BEGIN SELECT RAISE(ABORT,'reopen accounting month before modifying sale financials'); END;
CREATE TRIGGER trg_closed_month_sale_delete BEFORE DELETE ON sales
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month=substr(date(OLD.created_at,'+3 hours'),1,7)||'-01')
BEGIN SELECT RAISE(ABORT,'cannot delete sale in finalized month'); END;
CREATE TRIGGER trg_closed_month_expense_insert BEFORE INSERT ON expenses
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month=substr(date(NEW.expense_date,'+3 hours'),1,7)||'-01')
BEGIN SELECT RAISE(ABORT,'expense belongs to finalized accounting month'); END;
CREATE TRIGGER trg_closed_month_expense_update BEFORE UPDATE ON expenses
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month IN (substr(date(OLD.expense_date,'+3 hours'),1,7)||'-01',
                        substr(date(NEW.expense_date,'+3 hours'),1,7)||'-01'))
BEGIN SELECT RAISE(ABORT,'cannot update expense in finalized month'); END;
CREATE TRIGGER trg_closed_month_expense_delete BEFORE DELETE ON expenses
WHEN EXISTS(SELECT 1 FROM raw_material_month_closings c WHERE c.status='closed'
 AND c.period_month=substr(date(OLD.expense_date,'+3 hours'),1,7)||'-01')
BEGIN SELECT RAISE(ABORT,'cannot delete expense in finalized month'); END;
