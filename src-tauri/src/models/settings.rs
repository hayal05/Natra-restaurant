use rusqlite::Row;
use serde::{Deserialize, Serialize};

/// Single-row application settings (id is always 1 — enforced by a DB
/// CHECK constraint). Includes first-run/init state and optional Turso
/// sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub id: i64,
    pub restaurant_name: String,
    pub currency: String,
    pub is_initialized: bool,
    pub sync_enabled: bool,
    pub backup_enabled: bool,
    /// Never sent to the frontend — sync credentials stay backend-side.
    #[serde(skip_serializing)]
    pub turso_url: Option<String>,
    #[serde(skip_serializing)]
    pub turso_auth_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Settings {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Settings {
            id: row.get("id")?,
            restaurant_name: row.get("restaurant_name")?,
            currency: row.get("currency")?,
            is_initialized: row.get("is_initialized")?,
            sync_enabled: row.get("sync_enabled")?,
            backup_enabled: row.get("backup_enabled")?,
            turso_url: row.get("turso_url")?,
            turso_auth_token: row.get("turso_auth_token")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
