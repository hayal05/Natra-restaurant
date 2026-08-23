use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::models::sql_enum;

sql_enum!(PaymentMethod {
    Cash => "cash",
    Card => "card",
    Other => "other",
});

/// A POS transaction header. Totals (quantity/amount/cost/profit) are
/// computed and stored at checkout time from the sale's line items —
/// see `services::sales_service` — not recomputed on the fly, so
/// historical reports stay accurate even if item prices change later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sale {
    pub id: i64,
    pub waiter_id: i64,
    pub user_id: Option<i64>,

    pub total_quantity: i64,
    pub total_amount: f64,
    pub total_cost: f64,
    pub total_profit: f64,

    pub payment_method: PaymentMethod,

    /// Waiter receivable flag: false = still owed to the business.
    pub is_settled: bool,
    pub settled_at: Option<String>,

    /// Reversal (void) flag. A reversed sale is kept for audit history but
    /// excluded from every revenue/cost/profit calculation and from waiter
    /// receivables. See `sales_service::reverse_sale` for the 24-hour rule.
    pub is_reversed: bool,
    pub reversed_at: Option<String>,

    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Sale {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Sale {
            id: row.get("id")?,
            waiter_id: row.get("waiter_id")?,
            user_id: row.get("user_id")?,
            total_quantity: row.get("total_quantity")?,
            total_amount: row.get("total_amount")?,
            total_cost: row.get("total_cost")?,
            total_profit: row.get("total_profit")?,
            payment_method: row.get("payment_method")?,
            is_settled: row.get("is_settled")?,
            settled_at: row.get("settled_at")?,
            is_reversed: row.get("is_reversed")?,
            reversed_at: row.get("reversed_at")?,
            note: row.get("note")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
