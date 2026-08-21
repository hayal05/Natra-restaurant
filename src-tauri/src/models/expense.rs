use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

/// An "other" operating expense (rent, utilities, salaries, etc.),
/// recorded independently of sales and raw materials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: i64,
    pub category: String,
    pub description: Option<String>,
    pub amount: f64,
    pub expense_date: String,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Expense {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Expense {
            id: row.get("id")?,
            category: row.get("category")?,
            description: row.get("description")?,
            amount: row.get("amount")?,
            expense_date: row.get("expense_date")?,
            note: row.get("note")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.category.trim().is_empty() {
            return Err(AppError::Validation("expense category is required".into()));
        }
        if self.amount <= 0.0 {
            return Err(AppError::Validation("expense amount must be greater than zero".into()));
        }
        Ok(())
    }
}
