use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMaterial {
    pub id: i64,
    pub name: String,
    pub unit: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl RawMaterial {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(RawMaterial {
            id: row.get("id")?,
            name: row.get("name")?,
            unit: row.get("unit")?,
            is_active: row.get("is_active")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

/// A single raw-material purchase — an independent cost event, not
/// linked to any specific sale or dish (see 003_raw_materials.sql).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMaterialPurchase {
    pub id: i64,
    pub raw_material_id: i64,
    pub quantity: f64,
    pub unit_cost: f64,
    pub total_cost: f64,
    pub supplier: Option<String>,
    pub purchase_date: String,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl RawMaterialPurchase {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(RawMaterialPurchase {
            id: row.get("id")?,
            raw_material_id: row.get("raw_material_id")?,
            quantity: row.get("quantity")?,
            unit_cost: row.get("unit_cost")?,
            total_cost: row.get("total_cost")?,
            supplier: row.get("supplier")?,
            purchase_date: row.get("purchase_date")?,
            note: row.get("note")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    /// quantity/unit_cost must be positive, and total_cost should agree
    /// with quantity * unit_cost so reports stay trustworthy.
    pub fn validate(&self) -> AppResult<()> {
        if self.quantity <= 0.0 {
            return Err(AppError::Validation("quantity must be greater than zero".into()));
        }
        if self.unit_cost < 0.0 {
            return Err(AppError::Validation("unit cost cannot be negative".into()));
        }
        let expected = self.quantity * self.unit_cost;
        if (self.total_cost - expected).abs() > 0.01 {
            return Err(AppError::Validation(format!(
                "total_cost ({:.2}) does not match quantity * unit_cost ({:.2})",
                self.total_cost, expected
            )));
        }
        Ok(())
    }
}
