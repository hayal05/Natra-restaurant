//! Builds the daily backup workbook.
//!
//! Deliberately a full dump of every table, not an aggregated report —
//! the point of a backup is that the business's data can be
//! reconstructed from it, so each sheet mirrors a table's columns
//! 1:1 rather than summarizing. `reports::` (see `services::report_service`)
//! is for the "what happened" views; this is for "what do we have".

use rusqlite::Connection;
use rust_xlsxwriter::{Format, Workbook, Worksheet};

use crate::errors::AppResult;
use crate::models::category::Category;
use crate::models::{Expense, Item, RawMaterial, RawMaterialPurchase, Sale, SaleItem, Waiter};

/// Query every business table and return the finished .xlsx file as
/// bytes, ready to write to disk. Runs entirely against the given
/// connection — the caller (backup::manager) owns locking it.
pub fn build_workbook(conn: &Connection) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let header = header_format();

    write_sales(&mut workbook, conn, &header)?;
    write_sale_items(&mut workbook, conn, &header)?;
    write_expenses(&mut workbook, conn, &header)?;
    write_raw_material_purchases(&mut workbook, conn, &header)?;
    write_raw_materials(&mut workbook, conn, &header)?;
    write_items(&mut workbook, conn, &header)?;
    write_categories(&mut workbook, conn, &header)?;
    write_waiters(&mut workbook, conn, &header)?;

    Ok(workbook.save_to_buffer()?)
}

fn header_format() -> Format {
    Format::new().set_bold().set_background_color("#EFEAE3")
}

/// Writes a header row, auto-fits columns, and returns the sheet ready
/// for data rows starting at row 1.
fn new_sheet<'a>(workbook: &'a mut Workbook, name: &str, headers: &[&str], header_fmt: &Format) -> AppResult<&'a mut Worksheet> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name)?;
    for (col, title) in headers.iter().enumerate() {
        sheet.write_with_format(0, col as u16, *title, header_fmt)?;
    }
    Ok(sheet)
}

fn opt_str(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("")
}

fn write_sales(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM sales ORDER BY id")?;
    let rows: Vec<Sale> = stmt.query_map([], Sale::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Sales",
        &[
            "ID", "Waiter ID", "User ID", "Total Qty", "Total Amount", "Total Cost",
            "Total Profit", "Payment Method", "Settled", "Settled At", "Note", "Created At", "Updated At",
        ],
        header_fmt,
    )?;

    for (i, s) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, s.id)?;
        sheet.write(r, 1, s.waiter_id)?;
        sheet.write(r, 2, s.user_id.unwrap_or_default())?;
        sheet.write(r, 3, s.total_quantity)?;
        sheet.write(r, 4, s.total_amount)?;
        sheet.write(r, 5, s.total_cost)?;
        sheet.write(r, 6, s.total_profit)?;
        sheet.write(r, 7, s.payment_method.as_str())?;
        sheet.write_boolean(r, 8, s.is_settled)?;
        sheet.write(r, 9, opt_str(&s.settled_at))?;
        sheet.write(r, 10, opt_str(&s.note))?;
        sheet.write(r, 11, s.created_at.as_str())?;
        sheet.write(r, 12, s.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_sale_items(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM sale_items ORDER BY id")?;
    let rows: Vec<SaleItem> = stmt.query_map([], SaleItem::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Sale Items",
        &[
            "ID", "Sale ID", "Item ID", "Item Name", "Item Type", "Quantity",
            "Unit Price", "Unit Cost", "Line Total", "Line Cost", "Created At",
        ],
        header_fmt,
    )?;

    for (i, si) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, si.id)?;
        sheet.write(r, 1, si.sale_id)?;
        sheet.write(r, 2, si.item_id)?;
        sheet.write(r, 3, si.item_name.as_str())?;
        sheet.write(r, 4, si.item_type.as_str())?;
        sheet.write(r, 5, si.quantity)?;
        sheet.write(r, 6, si.unit_price)?;
        sheet.write(r, 7, si.unit_cost.unwrap_or_default())?;
        sheet.write(r, 8, si.line_total)?;
        sheet.write(r, 9, si.line_cost)?;
        sheet.write(r, 10, si.created_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_expenses(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM expenses ORDER BY id")?;
    let rows: Vec<Expense> = stmt.query_map([], Expense::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Expenses",
        &["ID", "Category", "Description", "Amount", "Expense Date", "Note", "Created At", "Updated At"],
        header_fmt,
    )?;

    for (i, e) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, e.id)?;
        sheet.write(r, 1, e.category.as_str())?;
        sheet.write(r, 2, opt_str(&e.description))?;
        sheet.write(r, 3, e.amount)?;
        sheet.write(r, 4, e.expense_date.as_str())?;
        sheet.write(r, 5, opt_str(&e.note))?;
        sheet.write(r, 6, e.created_at.as_str())?;
        sheet.write(r, 7, e.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_raw_material_purchases(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM raw_material_purchases ORDER BY id")?;
    let rows: Vec<RawMaterialPurchase> = stmt.query_map([], RawMaterialPurchase::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Raw Material Purchases",
        &[
            "ID", "Raw Material ID", "Quantity", "Unit Cost", "Total Cost",
            "Supplier", "Purchase Date", "Note", "Created At", "Updated At",
        ],
        header_fmt,
    )?;

    for (i, p) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, p.id)?;
        sheet.write(r, 1, p.raw_material_id)?;
        sheet.write(r, 2, p.quantity)?;
        sheet.write(r, 3, p.unit_cost)?;
        sheet.write(r, 4, p.total_cost)?;
        sheet.write(r, 5, opt_str(&p.supplier))?;
        sheet.write(r, 6, p.purchase_date.as_str())?;
        sheet.write(r, 7, opt_str(&p.note))?;
        sheet.write(r, 8, p.created_at.as_str())?;
        sheet.write(r, 9, p.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_raw_materials(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM raw_materials ORDER BY id")?;
    let rows: Vec<RawMaterial> = stmt.query_map([], RawMaterial::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Raw Materials",
        &["ID", "Name", "Unit", "Active", "Created At", "Updated At"],
        header_fmt,
    )?;

    for (i, m) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, m.id)?;
        sheet.write(r, 1, m.name.as_str())?;
        sheet.write(r, 2, m.unit.as_str())?;
        sheet.write_boolean(r, 3, m.is_active)?;
        sheet.write(r, 4, m.created_at.as_str())?;
        sheet.write(r, 5, m.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_items(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM items ORDER BY id")?;
    let rows: Vec<Item> = stmt.query_map([], Item::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Items",
        &[
            "ID", "Category ID", "Name", "Type", "Purchase Cost", "Selling Price",
            "Active", "Created At", "Updated At",
        ],
        header_fmt,
    )?;

    for (i, it) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, it.id)?;
        sheet.write(r, 1, it.category_id.unwrap_or_default())?;
        sheet.write(r, 2, it.name.as_str())?;
        sheet.write(r, 3, it.item_type.as_str())?;
        sheet.write(r, 4, it.purchase_cost.unwrap_or_default())?;
        sheet.write(r, 5, it.selling_price)?;
        sheet.write_boolean(r, 6, it.is_active)?;
        sheet.write(r, 7, it.created_at.as_str())?;
        sheet.write(r, 8, it.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_categories(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM categories ORDER BY id")?;
    let rows: Vec<Category> = stmt.query_map([], Category::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Categories",
        &["ID", "Name", "Description", "Created At", "Updated At"],
        header_fmt,
    )?;

    for (i, c) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, c.id)?;
        sheet.write(r, 1, c.name.as_str())?;
        sheet.write(r, 2, opt_str(&c.description))?;
        sheet.write(r, 3, c.created_at.as_str())?;
        sheet.write(r, 4, c.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

fn write_waiters(workbook: &mut Workbook, conn: &Connection, header_fmt: &Format) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT * FROM waiters ORDER BY id")?;
    let rows: Vec<Waiter> = stmt.query_map([], Waiter::from_row)?.collect::<Result<_, _>>()?;

    let sheet = new_sheet(
        workbook,
        "Waiters",
        &["ID", "Full Name", "Phone", "Active", "Created At", "Updated At"],
        header_fmt,
    )?;

    for (i, w) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, w.id)?;
        sheet.write(r, 1, w.full_name.as_str())?;
        sheet.write(r, 2, opt_str(&w.phone))?;
        sheet.write_boolean(r, 3, w.is_active)?;
        sheet.write(r, 4, w.created_at.as_str())?;
        sheet.write(r, 5, w.updated_at.as_str())?;
    }
    sheet.autofit();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn builds_a_workbook_against_an_empty_database() {
        let conn = db();
        let bytes = build_workbook(&conn).unwrap();
        // A real xlsx is a zip archive — "PK" magic bytes at minimum.
        assert!(bytes.len() > 2);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn builds_a_workbook_with_data() {
        let conn = db();
        conn.execute("INSERT INTO waiters (full_name) VALUES ('Abebe')", []).unwrap();
        conn.execute(
            "INSERT INTO categories (name) VALUES ('Drinks')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (name, type, purchase_cost, selling_price) VALUES ('Water', 'ready_made', 1.0, 2.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO expenses (category, amount) VALUES ('Rent', 100.0)",
            [],
        )
        .unwrap();

        let bytes = build_workbook(&conn).unwrap();
        assert!(bytes.len() > 2);
    }
}
