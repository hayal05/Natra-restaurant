use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::sql_enum;

sql_enum!(ItemType {
    ReadyMade => "ready_made",
    Cookable => "cookable",
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub category_id: Option<i64>,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: ItemType,
    /// Required for `ReadyMade`, must be `None` for `Cookable` — mirrors
    /// the DB CHECK constraint in 001_initial.sql. `validate()` enforces
    /// this in application code too, so the UI gets a clear message
    /// instead of a raw SQLite constraint error.
    pub purchase_cost: Option<f64>,
    pub selling_price: f64,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Item {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Item {
            id: row.get("id")?,
            category_id: row.get("category_id")?,
            name: row.get("name")?,
            item_type: row.get("type")?,
            purchase_cost: row.get("purchase_cost")?,
            selling_price: row.get("selling_price")?,
            is_active: row.get("is_active")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    /// Mirrors the DB-level CHECK constraint: ready-made items must carry
    /// a purchase cost; cookable items must not (their cost comes from
    /// raw materials, tracked separately).
    pub fn validate(&self) -> AppResult<()> {
        match (self.item_type, self.purchase_cost) {
            (ItemType::ReadyMade, None) => Err(AppError::Validation(
                "ready-made items must have a purchase cost".into(),
            )),
            (ItemType::Cookable, Some(_)) => Err(AppError::Validation(
                "cookable items must not have a purchase cost — cost is tracked via raw materials"
                    .into(),
            )),
            _ => {
                if self.selling_price < 0.0 {
                    return Err(AppError::Validation("selling price cannot be negative".into()));
                }
                if let Some(cost) = self.purchase_cost {
                    if cost < 0.0 {
                        return Err(AppError::Validation("purchase cost cannot be negative".into()));
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_item() -> Item {
        Item {
            id: 1,
            category_id: None,
            name: "Test".into(),
            item_type: ItemType::ReadyMade,
            purchase_cost: Some(1.0),
            selling_price: 2.0,
            is_active: true,
            created_at: "".into(),
            updated_at: "".into(),
        }
    }

    #[test]
    fn ready_made_without_cost_is_rejected() {
        let mut item = base_item();
        item.purchase_cost = None;
        assert!(item.validate().is_err());
    }

    #[test]
    fn cookable_with_cost_is_rejected() {
        let mut item = base_item();
        item.item_type = ItemType::Cookable;
        item.purchase_cost = Some(1.0);
        assert!(item.validate().is_err());
    }

    #[test]
    fn cookable_without_cost_is_valid() {
        let mut item = base_item();
        item.item_type = ItemType::Cookable;
        item.purchase_cost = None;
        assert!(item.validate().is_ok());
    }

    #[test]
    fn ready_made_with_cost_is_valid() {
        assert!(base_item().validate().is_ok());
    }
}
