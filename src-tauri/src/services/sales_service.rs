//! POS checkout — the core of the system.
//!
//! Given a waiter, an optional cashier, a payment method, and a cart of
//! (item_id, quantity) lines, `checkout` looks up each item, computes
//! quantities/line totals/line costs per the ready-made vs. cookable
//! rule, and atomically writes the sale header + line items. Nothing is
//! half-written: if any line references a missing/inactive item, the
//! whole sale is rejected and nothing is saved.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::database::schema::payment_method as pm;
use crate::database::transactions::with_transaction;
use crate::errors::{AppError, AppResult};
use crate::models::item::ItemType;
use crate::models::{PaymentMethod, Sale, SaleItem};

#[derive(Debug, Clone, Deserialize)]
pub struct CartLine {
    pub item_id: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutRequest {
    pub waiter_id: i64,
    pub user_id: Option<i64>,
    pub payment_method: PaymentMethod,
    pub lines: Vec<CartLine>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutResult {
    pub sale: Sale,
    pub items: Vec<SaleItem>,
}

/// Internal row shape fetched for each cart line before totals are computed.
struct PricedItem {
    id: i64,
    name: String,
    item_type: ItemType,
    unit_price: f64,
    unit_cost: Option<f64>,
    is_active: bool,
}

pub fn checkout(conn: &mut Connection, req: CheckoutRequest) -> AppResult<CheckoutResult> {
    if req.lines.is_empty() {
        return Err(AppError::Validation("a sale must have at least one item".into()));
    }
    for line in &req.lines {
        if line.quantity <= 0 {
            return Err(AppError::Validation(format!(
                "quantity for item {} must be greater than zero",
                line.item_id
            )));
        }
    }

    with_transaction(conn, |tx| {
        // Resolve and price every cart line up front. Any missing or
        // inactive item aborts the whole sale before anything is written.
        let mut priced: Vec<(PricedItem, i64)> = Vec::with_capacity(req.lines.len());
        for line in &req.lines {
            let item = tx
                .query_row(
                    "SELECT id, name, type, selling_price, purchase_cost, is_active FROM items WHERE id = ?1",
                    [line.item_id],
                    |row| {
                        Ok(PricedItem {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            item_type: row.get(2)?,
                            unit_price: row.get(3)?,
                            unit_cost: row.get(4)?,
                            is_active: row.get(5)?,
                        })
                    },
                )
                .map_err(|_| AppError::NotFound(format!("item {}", line.item_id)))?;

            if !item.is_active {
                return Err(AppError::Validation(format!("item '{}' is not active", item.name)));
            }

            priced.push((item, line.quantity));
        }

        // Compute totals. Cookable items contribute 0 to cost here —
        // their true cost is tracked independently via raw materials.
        let total_quantity: i64 = priced.iter().map(|(_, q)| q).sum();
        let total_amount: f64 = priced.iter().map(|(item, q)| item.unit_price * (*q as f64)).sum();
        let total_cost: f64 = priced
            .iter()
            .map(|(item, q)| item.unit_cost.unwrap_or(0.0) * (*q as f64))
            .sum();
        let total_profit = total_amount - total_cost;

        let payment_method_str = req.payment_method.as_str();
        debug_assert!([pm::CASH, pm::CARD, pm::OTHER].contains(&payment_method_str));

        tx.execute(
            "INSERT INTO sales (waiter_id, user_id, total_quantity, total_amount, total_cost, total_profit, payment_method, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                req.waiter_id,
                req.user_id,
                total_quantity,
                total_amount,
                total_cost,
                total_profit,
                req.payment_method,
                req.note,
            ],
        )?;
        let sale_id = tx.last_insert_rowid();

        let mut items = Vec::with_capacity(priced.len());
        for (item, quantity) in &priced {
            let line_total = item.unit_price * (*quantity as f64);
            let line_cost = item.unit_cost.unwrap_or(0.0) * (*quantity as f64);

            tx.execute(
                "INSERT INTO sale_items (sale_id, item_id, item_name, item_type, quantity, unit_price, unit_cost, line_total, line_cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    sale_id,
                    item.id,
                    item.name,
                    item.item_type,
                    quantity,
                    item.unit_price,
                    item.unit_cost,
                    line_total,
                    line_cost,
                ],
            )?;
            let sale_item_id = tx.last_insert_rowid();
            items.push(tx.query_row(
                "SELECT * FROM sale_items WHERE id = ?1",
                [sale_item_id],
                SaleItem::from_row,
            )?);
        }

        let sale = tx.query_row("SELECT * FROM sales WHERE id = ?1", [sale_id], Sale::from_row)?;

        Ok(CheckoutResult { sale, items })
    })
}

pub fn list_sales(conn: &Connection, waiter_id: Option<i64>, limit: i64) -> AppResult<Vec<Sale>> {
    let sql = if waiter_id.is_some() {
        "SELECT * FROM sales WHERE waiter_id = ?1 ORDER BY created_at DESC LIMIT ?2"
    } else {
        "SELECT * FROM sales ORDER BY created_at DESC LIMIT ?1"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = match waiter_id {
        Some(id) => stmt.query_map(rusqlite::params![id, limit], Sale::from_row)?,
        None => stmt.query_map(rusqlite::params![limit], Sale::from_row)?,
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use crate::services::{inventory_service, waiter_service};
    use crate::services::inventory_service::NewItem;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn mixed_cart_computes_correct_totals() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let cola = inventory_service::create_item(&conn, NewItem {
            category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(0.5), selling_price: 2.0,
        }).unwrap();
        let chicken = inventory_service::create_item(&conn, NewItem {
            category_id: None, name: "Grilled Chicken".into(), item_type: ItemType::Cookable,
            purchase_cost: None, selling_price: 8.0,
        }).unwrap();

        let result = checkout(&mut conn, CheckoutRequest {
            waiter_id: waiter.id,
            user_id: None,
            payment_method: PaymentMethod::Cash,
            lines: vec![
                CartLine { item_id: cola.id, quantity: 3 },
                CartLine { item_id: chicken.id, quantity: 2 },
            ],
            note: None,
        }).unwrap();

        assert_eq!(result.sale.total_quantity, 5);
        assert_eq!(result.sale.total_amount, 22.0);
        assert_eq!(result.sale.total_cost, 1.5); // only cola contributes cost
        assert_eq!(result.sale.total_profit, 20.5);
        assert_eq!(result.items.len(), 2);

        // Waiter receivable should reflect the new unsettled sale.
        assert_eq!(waiter_service::get_receivable(&conn, waiter.id).unwrap(), 22.0);
    }

    #[test]
    fn empty_cart_is_rejected() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let result = checkout(&mut conn, CheckoutRequest {
            waiter_id: waiter.id, user_id: None, payment_method: PaymentMethod::Cash,
            lines: vec![], note: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_item_aborts_the_whole_sale() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let cola = inventory_service::create_item(&conn, NewItem {
            category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(0.5), selling_price: 2.0,
        }).unwrap();

        let result = checkout(&mut conn, CheckoutRequest {
            waiter_id: waiter.id, user_id: None, payment_method: PaymentMethod::Cash,
            lines: vec![
                CartLine { item_id: cola.id, quantity: 1 },
                CartLine { item_id: 9999, quantity: 1 }, // doesn't exist
            ],
            note: None,
        });
        assert!(result.is_err());

        // Nothing should have been written — not even the valid cola line.
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM sales", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "a failed checkout must not leave a partial sale");
    }

    #[test]
    fn inactive_item_is_rejected() {
        let mut conn = db();
        let waiter = waiter_service::create_waiter(&conn, "Alex", None, None).unwrap();
        let cola = inventory_service::create_item(&conn, NewItem {
            category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(0.5), selling_price: 2.0,
        }).unwrap();
        inventory_service::set_item_active(&conn, cola.id, false).unwrap();

        let result = checkout(&mut conn, CheckoutRequest {
            waiter_id: waiter.id, user_id: None, payment_method: PaymentMethod::Cash,
            lines: vec![CartLine { item_id: cola.id, quantity: 1 }],
            note: None,
        });
        assert!(result.is_err());
    }
}
