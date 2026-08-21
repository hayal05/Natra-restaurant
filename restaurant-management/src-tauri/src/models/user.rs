use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::models::sql_enum;

sql_enum!(UserRole {
    Admin => "admin",
    Staff => "staff",
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    /// Never sent to the frontend on its own — commands should strip this
    /// before returning a User over IPC (see `commands::auth`).
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub full_name: String,
    pub role: UserRole,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(User {
            id: row.get("id")?,
            username: row.get("username")?,
            password_hash: row.get("password_hash")?,
            full_name: row.get("full_name")?,
            role: row.get("role")?,
            is_active: row.get("is_active")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
