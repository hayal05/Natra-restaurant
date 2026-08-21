//! Categories and items catalog.
//!
//! Enforces the ready-made vs. cookable rule at the application layer
//! (via `NewItem::validate`) before it ever reaches the database, so
//! callers get a clear validation message instead of a raw SQL error.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::{Category, Item, ItemType};

// ── Categories ──────────────────────────────────────────────

pub fn create_category(conn: &Connection, name: &str, description: Option<&str>) -> AppResult<Category> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("category name is required".into()));
    }
    conn.execute(
        "INSERT INTO categories (name, description) VALUES (?1, ?2)",
        rusqlite::params![name, description],
    )?;
    let id = conn.last_insert_rowid();
    Ok(conn.query_row("SELECT * FROM categories WHERE id = ?1", [id], Category::from_row)?)
}

pub fn list_categories(conn: &Connection) -> AppResult<Vec<Category>> {
    let mut stmt = conn.prepare("SELECT * FROM categories ORDER BY name")?;
    let rows = stmt.query_map([], Category::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

// ── Items ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct NewItem {
    pub category_id: Option<i64>,
    pub name: String,
    pub item_type: ItemType,
    pub purchase_cost: Option<f64>,
    pub selling_price: f64,
}

impl NewItem {
    fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation("item name is required".into()));
        }
        if self.selling_price < 0.0 {
            return Err(AppError::Validation("selling price cannot be negative".into()));
        }
        match (self.item_type, self.purchase_cost) {
            (ItemType::ReadyMade, None) => Err(AppError::Validation(
                "ready-made items must have a purchase cost".into(),
            )),
            (ItemType::Cookable, Some(_)) => Err(AppError::Validation(
                "cookable items must not have a purchase cost — cost is tracked via raw materials"
                    .into(),
            )),
            (ItemType::ReadyMade, Some(cost)) if cost < 0.0 => {
                Err(AppError::Validation("purchase cost cannot be negative".into()))
            }
            _ => Ok(()),
        }
    }
}

pub fn create_item(conn: &Connection, new_item: NewItem) -> AppResult<Item> {
    new_item.validate()?;

    conn.execute(
        "INSERT INTO items (category_id, name, type, purchase_cost, selling_price)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            new_item.category_id,
            new_item.name,
            new_item.item_type,
            new_item.purchase_cost,
            new_item.selling_price,
        ],
    )?;
    let id = conn.last_insert_rowid();
    Ok(conn.query_row("SELECT * FROM items WHERE id = ?1", [id], Item::from_row)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemFilter {
    pub only_active: bool,
    pub item_type: Option<ItemType>,
    pub category_id: Option<i64>,
}

pub fn list_items(conn: &Connection, filter: ItemFilter) -> AppResult<Vec<Item>> {
    let mut sql = String::from("SELECT * FROM items WHERE 1=1");
    if filter.only_active {
        sql.push_str(" AND is_active = 1");
    }
    if filter.item_type.is_some() {
        sql.push_str(" AND type = ?1");
    }
    if filter.category_id.is_some() {
        sql.push_str(if filter.item_type.is_some() { " AND category_id = ?2" } else { " AND category_id = ?1" });
    }
    sql.push_str(" ORDER BY name");

    let mut stmt = conn.prepare(&sql)?;
    let rows = match (filter.item_type, filter.category_id) {
        (Some(t), Some(c)) => stmt.query_map(rusqlite::params![t, c], Item::from_row)?,
        (Some(t), None) => stmt.query_map(rusqlite::params![t], Item::from_row)?,
        (None, Some(c)) => stmt.query_map(rusqlite::params![c], Item::from_row)?,
        (None, None) => stmt.query_map([], Item::from_row)?,
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_item_active(conn: &Connection, item_id: i64, is_active: bool) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE items SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![is_active, item_id],
    )?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("item {item_id}")));
    }
    Ok(())
}

/// Update selling price (and, for ready-made items, purchase cost).
/// Re-validates the ready-made/cookable rule after applying changes.
pub fn update_item_pricing(
    conn: &Connection,
    item_id: i64,
    selling_price: f64,
    purchase_cost: Option<f64>,
) -> AppResult<Item> {
    let existing = conn.query_row("SELECT * FROM items WHERE id = ?1", [item_id], Item::from_row)
        .map_err(|_| AppError::NotFound(format!("item {item_id}")))?;

    let updated = Item {
        selling_price,
        purchase_cost,
        ..existing
    };
    updated.validate()?;

    conn.execute(
        "UPDATE items SET selling_price = ?1, purchase_cost = ?2, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![selling_price, purchase_cost, item_id],
    )?;
    Ok(conn.query_row("SELECT * FROM items WHERE id = ?1", [item_id], Item::from_row)?)
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
    fn ready_made_without_cost_is_rejected_before_hitting_db() {
        let conn = db();
        let result = create_item(&conn, NewItem {
            category_id: None,
            name: "Cola".into(),
            item_type: ItemType::ReadyMade,
            purchase_cost: None,
            selling_price: 2.0,
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn cookable_with_cost_is_rejected() {
        let conn = db();
        let result = create_item(&conn, NewItem {
            category_id: None,
            name: "Grilled Chicken".into(),
            item_type: ItemType::Cookable,
            purchase_cost: Some(2.0),
            selling_price: 8.0,
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn valid_items_are_created_and_filterable() {
        let conn = db();
        create_item(&conn, NewItem {
            category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(0.5), selling_price: 2.0,
        }).unwrap();
        create_item(&conn, NewItem {
            category_id: None, name: "Grilled Chicken".into(), item_type: ItemType::Cookable,
            purchase_cost: None, selling_price: 8.0,
        }).unwrap();

        let all = list_items(&conn, ItemFilter::default()).unwrap();
        assert_eq!(all.len(), 2);

        let cookable_only = list_items(&conn, ItemFilter { item_type: Some(ItemType::Cookable), ..Default::default() }).unwrap();
        assert_eq!(cookable_only.len(), 1);
        assert_eq!(cookable_only[0].name, "Grilled Chicken");
    }

    #[test]
    fn update_pricing_reenforces_the_rule() {
        let conn = db();
        let cola = create_item(&conn, NewItem {
            category_id: None, name: "Cola".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(0.5), selling_price: 2.0,
        }).unwrap();

        // Trying to strip the purchase cost from a ready-made item must fail.
        let result = update_item_pricing(&conn, cola.id, 2.5, None);
        assert!(result.is_err());

        // A valid update goes through.
        let updated = update_item_pricing(&conn, cola.id, 2.5, Some(0.6)).unwrap();
        assert_eq!(updated.selling_price, 2.5);
        assert_eq!(updated.purchase_cost, Some(0.6));
    }
}
