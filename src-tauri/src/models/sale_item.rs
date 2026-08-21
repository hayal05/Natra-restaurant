use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::models::item::ItemType;

/// A single line within a sale. Snapshots the item's name/type/price/cost
/// at the moment of sale so later edits to the item catalog never rewrite
/// historical receipts (see 002_sales.sql).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleItem {
    pub id: i64,
    pub sale_id: i64,
    pub item_id: i64,

    pub item_name: String,
    pub item_type: ItemType,

    pub quantity: i64,
    pub unit_price: f64,
    /// Populated only for ready-made items (snapshot of purchase_cost).
    /// `None` for cookable items — their true cost lives in raw materials.
    pub unit_cost: Option<f64>,

    pub line_total: f64,
    pub line_cost: f64,

    pub created_at: String,
}

impl SaleItem {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(SaleItem {
            id: row.get("id")?,
            sale_id: row.get("sale_id")?,
            item_id: row.get("item_id")?,
            item_name: row.get("item_name")?,
            item_type: row.get("item_type")?,
            quantity: row.get("quantity")?,
            unit_price: row.get("unit_price")?,
            unit_cost: row.get("unit_cost")?,
            line_total: row.get("line_total")?,
            line_cost: row.get("line_cost")?,
            created_at: row.get("created_at")?,
        })
    }
}
