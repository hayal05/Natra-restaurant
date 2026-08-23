//! Expenses and the profit formula.
//!
//! Profit = Sales - Ready-Made Costs - Raw-Material Costs - Other Expenses

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::Expense;
use crate::services::raw_material_service;

pub fn create_expense(conn: &Connection, category: &str, description: Option<&str>, amount: f64, expense_date: Option<&str>, note: Option<&str>) -> AppResult<Expense> {
    if category.trim().is_empty() { return Err(AppError::Validation("expense category is required".into())); }
    if amount <= 0.0 { return Err(AppError::Validation("expense amount must be greater than zero".into())); }
    match expense_date {
        Some(date) => { conn.execute("INSERT INTO expenses (category, description, amount, expense_date, note) VALUES (?1, ?2, ?3, ?4, ?5)", rusqlite::params![category, description, amount, date, note])?; }
        None => { conn.execute("INSERT INTO expenses (category, description, amount, note) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![category, description, amount, note])?; }
    }
    let id = conn.last_insert_rowid();
    Ok(conn.query_row("SELECT * FROM expenses WHERE id = ?1", [id], Expense::from_row)?)
}

pub fn list_expenses(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<Expense>> {
    // Keep the prepared statement alive for the whole query_map/collect operation.
    // Returning from each match arm avoids borrowing the statement across the
    // outer match expression on newer Rust/rusqlite combinations.
    match (from, to) {
        (Some(f), Some(t)) => {
            let mut stmt = conn.prepare("SELECT * FROM expenses WHERE expense_date BETWEEN ?1 AND ?2 ORDER BY expense_date DESC")?;
            let rows = stmt
                .query_map(rusqlite::params![f, t], Expense::from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        _ => {
            let mut stmt = conn.prepare("SELECT * FROM expenses ORDER BY expense_date DESC")?;
            let rows = stmt
                .query_map([], Expense::from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitSummary { pub sales: f64, pub ready_made_costs: f64, pub raw_material_costs: f64, pub other_expenses: f64, pub profit: f64 }

pub fn calculate_profit(conn: &Connection, from: Option<&str>, to: Option<&str>) -> AppResult<ProfitSummary> {
    // is_reversed = 0 excludes voided sales (see sales_service::reverse_sale)
    // from every revenue/cost figure.
    let (sales, ready_made_costs): (f64, f64) = match (from, to) {
        (Some(f), Some(t)) => conn.query_row("SELECT COALESCE(SUM(total_amount),0), COALESCE(SUM(total_cost),0) FROM sales WHERE is_reversed = 0 AND created_at BETWEEN ?1 AND ?2", [f, t], |row| Ok((row.get(0)?, row.get(1)?)))?,
        _ => conn.query_row("SELECT COALESCE(SUM(total_amount),0), COALESCE(SUM(total_cost),0) FROM sales WHERE is_reversed = 0", [], |row| Ok((row.get(0)?, row.get(1)?)))?,
    };
    let raw_material_costs = raw_material_service::total_cost(conn, from, to)?;
    let other_expenses: f64 = match (from, to) {
        (Some(f), Some(t)) => conn.query_row("SELECT COALESCE(SUM(amount),0) FROM expenses WHERE expense_date BETWEEN ?1 AND ?2", [f, t], |row| row.get(0))?,
        _ => conn.query_row("SELECT COALESCE(SUM(amount),0) FROM expenses", [], |row| row.get(0))?,
    };
    let profit = sales - ready_made_costs - raw_material_costs - other_expenses;
    Ok(ProfitSummary { sales, ready_made_costs, raw_material_costs, other_expenses, profit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use crate::models::PaymentMethod;
    use crate::services::inventory_service::{self, NewItem};
    use crate::services::sales_service::{checkout, CartLine, CheckoutRequest};
    use crate::services::waiter_service;
    use crate::models::item::ItemType;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn full_profit_formula_matches_hand_calculation() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let cola = inventory_service::create_item(&conn, NewItem { category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade, purchase_cost: Some(0.5), selling_price: 2.0 }).unwrap();
        let chicken = inventory_service::create_item(&conn, NewItem { category_id: None, name: "Grilled Chicken".into(), item_type: ItemType::Cookable, purchase_cost: None, selling_price: 8.0 }).unwrap();
        checkout(&mut conn, CheckoutRequest { waiter_id: waiter.id, user_id: None, payment_method: PaymentMethod::Cash, lines: vec![CartLine { item_id: cola.id, quantity: 3 }, CartLine { item_id: chicken.id, quantity: 2 }], note: None }).unwrap();
        let chicken_material = raw_material_service::create_raw_material(&conn, "Chicken", "kg").unwrap();
        raw_material_service::record_purchase(&conn, chicken_material.id, 2.0, 3.0, None, None).unwrap();
        create_expense(&conn, "Utilities", Some("Electricity"), 4.5, None, None).unwrap();
        let summary = calculate_profit(&conn, None, None).unwrap();
        assert_eq!(summary.sales, 22.0);
        assert_eq!(summary.ready_made_costs, 1.5);
        assert_eq!(summary.raw_material_costs, 6.0);
        assert_eq!(summary.other_expenses, 4.5);
        assert_eq!(summary.profit, 10.0);
    }

    #[test]
    fn zero_amount_expense_is_rejected() {
        let conn = db();
        let result = create_expense(&conn, "Rent", None, 0.0, None, None);
        assert!(result.is_err());
    }
}
