use rusqlite::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Waiter {
    pub id: i64,
    pub full_name: String,
    pub phone: Option<String>,
    pub profile_photo: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Waiter {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Waiter {
            id: row.get("id")?,
            full_name: row.get("full_name")?,
            phone: row.get("phone")?,
            profile_photo: row.get("profile_photo")?,
            is_active: row.get("is_active")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
