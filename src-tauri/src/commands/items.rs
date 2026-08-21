//! Tauri commands for the categories & item catalog.

use tauri::State;

use crate::database::Db;
use crate::errors::AppResult;
use crate::models::{Category, Item};
use crate::services::inventory_service::{self, ItemFilter, NewItem};

// ── Categories ──────────────────────────────────────────────

#[tauri::command]
pub fn create_category(
    db: State<Db>,
    name: String,
    description: Option<String>,
) -> AppResult<Category> {
    let conn = db.lock();
    inventory_service::create_category(&conn, &name, description.as_deref())
}

#[tauri::command]
pub fn list_categories(db: State<Db>) -> AppResult<Vec<Category>> {
    let conn = db.lock();
    inventory_service::list_categories(&conn)
}

// ── Items ──────────────────────────────────────────────

#[tauri::command]
pub fn create_item(db: State<Db>, new_item: NewItem) -> AppResult<Item> {
    let conn = db.lock();
    inventory_service::create_item(&conn, new_item)
}

#[tauri::command]
pub fn list_items(db: State<Db>, filter: ItemFilter) -> AppResult<Vec<Item>> {
    let conn = db.lock();
    inventory_service::list_items(&conn, filter)
}

#[tauri::command]
pub fn set_item_active(db: State<Db>, item_id: i64, is_active: bool) -> AppResult<()> {
    let conn = db.lock();
    inventory_service::set_item_active(&conn, item_id, is_active)
}

#[tauri::command]
pub fn update_item_pricing(
    db: State<Db>,
    item_id: i64,
    selling_price: f64,
    purchase_cost: Option<f64>,
) -> AppResult<Item> {
    let conn = db.lock();
    inventory_service::update_item_pricing(&conn, item_id, selling_price, purchase_cost)
}
