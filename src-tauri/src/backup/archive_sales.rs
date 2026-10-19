//! Read-only, item-level transaction export for the permanent archive feature.
//! This does not use or modify the rolling full-database backup system.

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use rust_xlsxwriter::{Format, Workbook};

use crate::errors::{AppError, AppResult};

const SQLITE_TIMESTAMP: &str = "%Y-%m-%d %H:%M:%S";

/// Archive time ranges are expressed as UTC instants, [start, end).
/// The caller is responsible for converting EAT business-day boundaries to UTC.
pub fn build_sales_archive(
    conn: &Connection,
    batch_id: &str,
    start_utc: &str,
    end_utc: &str,
) -> AppResult<Vec<u8>> {
    let start = parse_utc(start_utc)?;
    let end = parse_utc(end_utc)?;
    if start >= end {
        return Err(AppError::Validation("archive start must precede end".into()));
    }
    if batch_id.trim().is_empty() {
        return Err(AppError::Validation("batch ID is required".into()));
    }

    let mut stmt = conn.prepare(
        "SELECT s.id, si.id, s.created_at, s.waiter_id, w.full_name,
                si.item_id, si.item_name, si.item_type, si.quantity,
                si.unit_price, si.line_total, s.payment_method,
                s.is_settled, s.is_reversed, s.user_id, s.note
         FROM sales s
         INNER JOIN sale_items si ON si.sale_id = s.id
         LEFT JOIN waiters w ON w.id = s.waiter_id
         WHERE s.created_at >= ?1 AND s.created_at < ?2
         ORDER BY s.created_at ASC, s.id ASC, si.id ASC",
    )?;

    // Materialize rows while SQLite statement is alive. No write query executes.
    let rows = stmt.query_map(params![start_utc, end_utc], |r| {
        Ok(SalesArchiveRow {
            receipt: r.get(0)?, line_id: r.get(1)?, timestamp: r.get(2)?,
            waiter_id: r.get(3)?, waiter_name: r.get(4)?,
            item_id: r.get(5)?, item_name: r.get(6)?, item_type: r.get(7)?,
            quantity: r.get(8)?, unit_price: r.get(9)?, line_total: r.get(10)?,
            payment: r.get(11)?, settled: r.get(12)?, reversed: r.get(13)?,
            user_id: r.get(14)?, note: r.get(15)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name("Sales Details")?;
    let heading = Format::new().set_bold().set_background_color("#EFEAE3");
    let money = Format::new().set_num_format("#,##0.00");
    let headers = [
        "Batch ID", "Receipt", "Line ID", "Date", "Time", "Waiter ID",
        "Waiter Name", "Item ID", "Item", "Item Type", "Qty",
        "Unit Price", "Total", "Payment Method", "Settled",
        "Reversed", "Cashier ID", "Note",
    ];
    for (i, name) in headers.iter().enumerate() {
        sheet.write_with_format(0, i as u16, *name, &heading)?;
    }
    for (i, sale) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        let (date, time) = eat_display(&sale.timestamp)?;
        // write_string rather than write(): user-originated fields must never
        // be interpreted as an Excel formula.
        sheet.write_string(row, 0, batch_id)?;
        sheet.write_number(row, 1, sale.receipt as f64)?;
        sheet.write_number(row, 2, sale.line_id as f64)?;
        sheet.write_string(row, 3, &date)?;
        sheet.write_string(row, 4, &time)?;
        sheet.write_number(row, 5, sale.waiter_id as f64)?;
        sheet.write_string(row, 6, sale.waiter_name.as_deref().unwrap_or(""))?;
        sheet.write_number(row, 7, sale.item_id as f64)?;
        sheet.write_string(row, 8, &sale.item_name)?;
        sheet.write_string(row, 9, &sale.item_type)?;
        sheet.write_number(row, 10, sale.quantity as f64)?;
        sheet.write_number_with_format(row, 11, sale.unit_price, &money)?;
        sheet.write_number_with_format(row, 12, sale.line_total, &money)?;
        sheet.write_string(row, 13, &sale.payment)?;
        sheet.write_boolean(row, 14, sale.settled)?;
        sheet.write_boolean(row, 15, sale.reversed)?;
        if let Some(id) = sale.user_id {
            sheet.write_number(row, 16, id as f64)?;
        }
        sheet.write_string(row, 17, sale.note.as_deref().unwrap_or(""))?;
    }
    sheet.autofit();

    // Detail preserves reversals; revenue summaries exclude reversed sales.
    let mut receipts: BTreeMap<i64, (String, String, String, i64, f64, bool)> = BTreeMap::new();
    let mut waiters: BTreeMap<i64, (String, i64, f64)> = BTreeMap::new();
    let mut items: BTreeMap<i64, (String, i64, f64)> = BTreeMap::new();
    let (mut gross, mut reversed, mut net, mut active_qty) = (0.0, 0.0, 0.0, 0i64);
    for sale in &rows {
        gross += sale.line_total;
        if sale.reversed {
            reversed += sale.line_total;
        } else {
            net += sale.line_total;
            active_qty += sale.quantity;
            let w = waiters.entry(sale.waiter_id).or_insert((sale.waiter_name.clone().unwrap_or_default(), 0, 0.0));
            w.1 += sale.quantity;
            w.2 += sale.line_total;
            let item = items.entry(sale.item_id).or_insert((sale.item_name.clone(), 0, 0.0));
            item.1 += sale.quantity;
            item.2 += sale.line_total;
        }
        let (date, time) = eat_display(&sale.timestamp)?;
        let receipt = receipts.entry(sale.receipt).or_insert((date, time, sale.waiter_name.clone().unwrap_or_default(), 0, 0.0, sale.reversed));
        receipt.3 += sale.quantity;
        receipt.4 += sale.line_total;
    }
    {
        let s = book.add_worksheet();
        s.set_name("Receipt Summary")?;
        for (i, h) in ["Receipt", "Date", "Time", "Waiter Name", "Qty", "Amount", "Reversed"].iter().enumerate() {
            s.write_with_format(0, i as u16, *h, &heading)?;
        }
        for (i, (id, (date, time, name, qty, amount, is_reversed))) in receipts.iter().enumerate() {
            let r = (i + 1) as u32;
            s.write_number(r, 0, *id as f64)?;
            s.write_string(r, 1, date)?;
            s.write_string(r, 2, time)?;
            s.write_string(r, 3, name)?;
            s.write_number(r, 4, *qty as f64)?;
            s.write_number_with_format(r, 5, *amount, &money)?;
            s.write_boolean(r, 6, *is_reversed)?;
        }
        s.autofit();
    }
    {
        let s = book.add_worksheet();
        s.set_name("Waiter Sales Summary")?;
        for (i, h) in ["Waiter ID", "Waiter Name", "Qty Sold", "Net Sales"].iter().enumerate() {
            s.write_with_format(0, i as u16, *h, &heading)?;
        }
        for (i, (id, (name, qty, total))) in waiters.iter().enumerate() {
            let r = (i + 1) as u32;
            s.write_number(r, 0, *id as f64)?;
            s.write_string(r, 1, name)?;
            s.write_number(r, 2, *qty as f64)?;
            s.write_number_with_format(r, 3, *total, &money)?;
        }
        s.autofit();
    }
    {
        let s = book.add_worksheet();
        s.set_name("Item Sales Summary")?;
        for (i, h) in ["Item ID", "Item Name", "Qty Sold", "Net Sales"].iter().enumerate() {
            s.write_with_format(0, i as u16, *h, &heading)?;
        }
        for (i, (id, (name, qty, total))) in items.iter().enumerate() {
            let r = (i + 1) as u32;
            s.write_number(r, 0, *id as f64)?;
            s.write_string(r, 1, name)?;
            s.write_number(r, 2, *qty as f64)?;
            s.write_number_with_format(r, 3, *total, &money)?;
        }
        s.autofit();
    }
    let expenses = {
        let mut stmt = conn.prepare(
            "SELECT id, expense_date, category, description, amount, note
             FROM expenses WHERE expense_date >= ?1 AND expense_date < ?2
             ORDER BY expense_date, id"
        )?;
        let result = stmt.query_map(params![start_utc, end_utc], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?, r.get::<_, f64>(4)?,
                r.get::<_, Option<String>>(5)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        result
    };
    let expense_total: f64 = expenses.iter().map(|e| e.4).sum();
    {
        let s = book.add_worksheet();
        s.set_name("Expenses")?;
        for (i, h) in ["Expense ID", "Date", "Time", "Category", "Description", "Amount", "Note"].iter().enumerate() {
            s.write_with_format(0, i as u16, *h, &heading)?;
        }
        for (i, (id, timestamp, category, description, amount, note)) in expenses.iter().enumerate() {
            let r = (i + 1) as u32;
            let (date, time) = eat_display(timestamp)?;
            s.write_number(r, 0, *id as f64)?;
            s.write_string(r, 1, &date)?;
            s.write_string(r, 2, &time)?;
            s.write_string(r, 3, category)?;
            s.write_string(r, 4, description.as_deref().unwrap_or(""))?;
            s.write_number_with_format(r, 5, *amount, &money)?;
            s.write_string(r, 6, note.as_deref().unwrap_or(""))?;
        }
        s.autofit();
    }
    {
        let s = book.add_worksheet();
        s.set_name("Batch Summary")?;
        s.write_with_format(0, 0, "Metric", &heading)?;
        s.write_with_format(0, 1, "Value", &heading)?;
        for (i, (label, value)) in [
            ("Gross sales including reversed", gross),
            ("Reversed sales", reversed),
            ("Net active sales", net),
            ("Operating expenses", expense_total),
            ("Net sales less expenses (not profit)", net - expense_total)
        ].iter().enumerate() {
            let r = (i + 1) as u32;
            s.write_string(r, 0, *label)?;
            s.write_number_with_format(r, 1, *value, &money)?;
        }
        s.write_string(6, 0, "Active quantity sold")?;
        s.write_number(6, 1, active_qty as f64)?;
        s.write_string(7, 0, "Receipts including reversed")?;
        s.write_number(7, 1, receipts.len() as f64)?;
        s.write_string(8, 0, "Batch ID")?;
        s.write_string(8, 1, batch_id)?;
        s.write_string(9, 0, "UTC window start (inclusive)")?;
        s.write_string(9, 1, start_utc)?;
        s.write_string(10, 0, "UTC window end (exclusive)")?;
        s.write_string(10, 1, end_utc)?;
        s.autofit();
    }
    Ok(book.save_to_buffer()?)
}

struct SalesArchiveRow {
    receipt: i64,
    line_id: i64,
    timestamp: String,
    waiter_id: i64,
    waiter_name: Option<String>,
    item_id: i64,
    item_name: String,
    item_type: String,
    quantity: i64,
    unit_price: f64,
    line_total: f64,
    payment: String,
    settled: bool,
    reversed: bool,
    user_id: Option<i64>,
    note: Option<String>,
}

fn parse_utc(value: &str) -> AppResult<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value, SQLITE_TIMESTAMP)
        .map_err(|_| AppError::Validation(format!("invalid UTC archive timestamp: {value}")))?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn eat_display(value: &str) -> AppResult<(String, String)> {
    let eat = FixedOffset::east_opt(3 * 3600)
        .ok_or_else(|| AppError::Internal("invalid EAT offset".into()))?;
    let dt = parse_utc(value)?.with_timezone(&eat);
    Ok((dt.format("%d/%m/%Y").to_string(), dt.format("%I:%M:%S %p").to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn eat_time_rolls_into_next_day_and_uses_am_pm() {
        assert_eq!(
            eat_display("2026-10-19 21:15:30").unwrap(),
            ("20/10/2026".to_string(), "12:15:30 AM".to_string()),
        );
        assert_eq!(eat_display("2026-10-19 09:00:00").unwrap().1, "12:00:00 PM");
    }

    #[test]
    fn archive_is_a_zip_workbook_and_does_not_change_source_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        migrations::run(&conn).unwrap();
        conn.execute("INSERT INTO waiters (full_name) VALUES ('Abebe Kebede')", []).unwrap();
        conn.execute(
            "INSERT INTO items (name, type, purchase_cost, selling_price)
             VALUES ('Water', 'ready_made', 4, 10)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sales (waiter_id, total_quantity, total_amount, total_cost, total_profit, created_at)
             VALUES (1, 2, 20, 8, 12, '2026-10-19 09:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sale_items (sale_id, item_id, item_name, item_type, quantity, unit_price, unit_cost, line_total, line_cost)
             VALUES (1, 1, 'Water', 'ready_made', 2, 10, 4, 20, 8)",
            [],
        ).unwrap();
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM sale_items", [], |r| r.get(0)).unwrap();
        let bytes = build_sales_archive(
            &conn, "B-20261019", "2026-10-18 21:00:00", "2026-10-19 21:00:00"
        ).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        let after: i64 = conn.query_row("SELECT COUNT(*) FROM sale_items", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn empty_archive_and_bad_window() {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        assert!(build_sales_archive(&conn, "B", "2026-10-18 21:00:00", "2026-10-19 21:00:00").is_ok());
        assert!(build_sales_archive(&conn, "B", "2026-10-19 21:00:00", "2026-10-19 21:00:00").is_err());
    }
}
