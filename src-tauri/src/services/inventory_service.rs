//! Categories and items catalog.
//!
//! Enforces the ready-made vs. cookable rule at the application layer
//! (via `NewItem::validate`) before it ever reaches the database, so
//! callers get a clear validation message instead of a raw SQL error.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::{Category, Item, ItemType};

// ── Categories ──────────────────────────────────────────────

pub fn create_category(conn: &Connection, name: &str, description: Option<&str>) -> AppResult<Category> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("category name is required".into()));
    }
    // Keep the category and its upload event in one atomic SQLite commit.
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO categories (name, description) VALUES (?1, ?2)",
        rusqlite::params![name, description],
    )?;
    let id = tx.last_insert_rowid();
    let category = tx.query_row(
        "SELECT * FROM categories WHERE id = ?1", [id], Category::from_row
    )?;
    let installation_id = crate::database::connection::ensure_installation_id(&tx)?;
    let entity_id = format!("{installation_id}:{id}");
    crate::database::entity_ids::register(&tx, "categories", &entity_id, id)?;
    crate::sync::queue::enqueue_cloud_event(
        &tx, "categories", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "name": name,
            "description": description,
        }),
    )?;
    tx.commit()?;
    Ok(category)
}

/// Import a category by cloud identity inside the caller's cloud batch
/// transaction. Remote SQLite row IDs must never be used locally.
#[allow(dead_code)]
pub(crate) fn apply_remote_category(
    tx: &rusqlite::Transaction<'_>,
    entity_id: &str,
    payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote category".into());
    let name = payload.get("name").and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty()).ok_or_else(invalid)?;
    let description = match payload.get("description") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(value.as_str().ok_or_else(invalid)?),
    };
    if entity_id.trim().is_empty() { return Err(invalid()); }
    if crate::database::entity_ids::lookup(tx, "categories", entity_id)?.is_some() {
        return Err(AppError::Validation("category identity already imported".into()));
    }
    tx.execute(
        "INSERT INTO categories(name,description) VALUES(?1,?2)",
        rusqlite::params![name,description],
    )?;
    crate::database::entity_ids::register(tx, "categories", entity_id, tx.last_insert_rowid())?;
    Ok(())
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

    let tx = conn.unchecked_transaction()?;
    tx.execute(
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
    let id = tx.last_insert_rowid();
    let item = tx.query_row("SELECT * FROM items WHERE id = ?1", [id], Item::from_row)?;
    let installation_id = crate::database::connection::ensure_installation_id(&tx)?;
    let entity_id = format!("{installation_id}:{id}");
    crate::database::entity_ids::register(&tx, "items", &entity_id, id)?;
    let category_entity_id = if let Some(category_id) = item.category_id {
        let existing: Option<String> = tx.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='categories' AND local_id=?1",
            [category_id], |r| r.get(0),
        ).optional()?;
        let category_cloud_id = existing.unwrap_or_else(|| format!("{installation_id}:{category_id}"));
        crate::database::entity_ids::register(&tx, "categories", &category_cloud_id, category_id)?;
        Some(category_cloud_id)
    } else { None };
    crate::sync::queue::enqueue_cloud_event(
        &tx, "items", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": id,
            "category_entity_id": category_entity_id,
            "item": item,
        }),
    )?;
    tx.commit()?;
    Ok(item)
}

/// Import a menu item using stable category identity, not sender-side IDs.
#[allow(dead_code)]
pub(crate) fn apply_remote_item(
    tx: &rusqlite::Transaction<'_>, entity_id: &str, payload: &serde_json::Value,
) -> AppResult<()> {
    let invalid = || AppError::Validation("invalid remote menu item".into());
    let item: Item = serde_json::from_value(
        payload.get("item").cloned().ok_or_else(invalid)?
    ).map_err(|_| invalid())?;
    item.validate()?;
    if entity_id.trim().is_empty() || item.name.trim().is_empty()
        || !item.selling_price.is_finite()
        || item.purchase_cost.is_some_and(|cost| !cost.is_finite()) {
        return Err(invalid());
    }
    // The cloud sequence controls replay order; mapped catalog identities
    // may receive subsequent price, activation, and category changes.
    let existing_id = crate::database::entity_ids::lookup(tx, "items", entity_id)?;
    let category_id = match payload.get("category_entity_id") {
        None | Some(serde_json::Value::Null) if item.category_id.is_none() => None,
        Some(serde_json::Value::String(cloud_id)) if item.category_id.is_some() => {
            Some(crate::database::entity_ids::lookup(tx, "categories", cloud_id)?
                .ok_or_else(|| AppError::Validation("unknown cloud category for menu item".into()))?)
        },
        _ => return Err(invalid()),
    };
    if let Some(local_id) = existing_id {
        // Never replace local SQLite identity or historical transaction lines.
        let changed = tx.execute(
            "UPDATE items SET category_id=?1,name=?2,type=?3,purchase_cost=?4,
             selling_price=?5,is_active=?6,updated_at=?7 WHERE id=?8",
            rusqlite::params![category_id,item.name,item.item_type,item.purchase_cost,
                item.selling_price,item.is_active,item.updated_at,local_id],
        )?;
        if changed != 1 { return Err(invalid()); }
    } else {
        tx.execute(
            "INSERT INTO items(category_id,name,type,purchase_cost,selling_price,is_active,
             created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![category_id,item.name,item.item_type,item.purchase_cost,
                item.selling_price,item.is_active,item.created_at,item.updated_at],
        )?;
        crate::database::entity_ids::register(tx, "items", entity_id, tx.last_insert_rowid())?;
    }
    Ok(())
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

// Shared by activation and pricing changes; both save a complete item snapshot.
fn enqueue_item_update(tx: &rusqlite::Transaction<'_>, item_id: i64) -> AppResult<Item> {
    let item = tx.query_row("SELECT * FROM items WHERE id = ?1", [item_id], Item::from_row)?;
    let installation_id = crate::database::connection::ensure_installation_id(tx)?;
    // Imported items retain their original global identity when edited here.
    // Existing local items without a mapping get one on their first edit.
    let entity_id: Option<String> = tx.query_row(
        "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='items' AND local_id=?1",
        [item_id], |row| row.get(0),
    ).optional()?;
    let entity_id = match entity_id {
        Some(id) => id,
        None => {
            let id = format!("{installation_id}:{item_id}");
            crate::database::entity_ids::register(tx, "items", &id, item_id)?;
            id
        }
    };
    crate::sync::queue::enqueue_cloud_event(
        tx, "items", &entity_id, "upsert",
        &serde_json::json!({
            "origin_installation_id": installation_id,
            "local_id": item_id,
            "item": item,
        }),
    )?;
    Ok(item)
}

pub fn set_item_active(conn: &Connection, item_id: i64, is_active: bool) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute(
        "UPDATE items SET is_active = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![is_active, item_id],
    )?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("item {item_id}")));
    }
    enqueue_item_update(&tx, item_id)?;
    tx.commit()?;
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

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE items SET selling_price = ?1, purchase_cost = ?2, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![selling_price, purchase_cost, item_id],
    )?;
    let item = enqueue_item_update(&tx, item_id)?;
    tx.commit()?;
    Ok(item)
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
    fn remote_category_uses_own_local_id_and_rejects_duplicate() {
        let mut conn = db();
        let payload = serde_json::json!({
            "origin_installation_id":"other","local_id":999,
            "name":"Imported soups","description":"Lunch"
        });
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-category-1",1)],
            |tx,_,_| apply_remote_category(tx,"other:999",&payload),
        ).unwrap();
        let local = crate::database::entity_ids::lookup(&conn,"categories","other:999")
            .unwrap().unwrap();
        assert_ne!(local,999);
        assert!(crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-category-2",2)],
            |tx,_,_| apply_remote_category(tx,"other:999",&payload),
        ).is_err());
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),1);
    }

    #[test]
    fn remote_item_revision_reuses_local_row_and_changes_price() {
        let mut conn = db();
        let original = create_item(&conn, NewItem {
            category_id: None, name: "Imported tea".into(),
            item_type: ItemType::ReadyMade,
            purchase_cost: Some(2.0), selling_price: 5.0,
        }).unwrap();
        let cloud_id: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='items' AND local_id=?1",
            [original.id], |r| r.get(0),
        ).unwrap();
        let mut changed = serde_json::to_value(original.clone()).unwrap();
        changed["selling_price"] = serde_json::json!(7.0);
        changed["is_active"] = serde_json::json!(false);
        let payload = serde_json::json!({"item": changed, "category_entity_id": null});
        crate::database::connection::apply_cloud_event_batch(
            &mut conn, &[("remote-item-update",1)],
            |tx,_,_| apply_remote_item(tx, &cloud_id, &payload),
        ).unwrap();
        let (price, active): (f64, bool) = conn.query_row(
            "SELECT selling_price,is_active FROM items WHERE id=?1", [original.id],
            |r| Ok((r.get(0)?,r.get(1)?)),
        ).unwrap();
        assert_eq!(price, 7.0);
        assert!(!active);
        assert_eq!(crate::database::entity_ids::lookup(&conn, "items", &cloud_id).unwrap(), Some(original.id));
    }

    #[test]
    fn remote_menu_item_resolves_category_and_replays_updates() {
        let mut conn = db();
        let category = create_category(&conn,"Drinks",None).unwrap();
        let cloud_category: String = conn.query_row(
            "SELECT entity_id FROM cloud_entity_ids WHERE entity_type='categories'",
            [], |r|r.get(0),
        ).unwrap();
        let item = create_item(&conn,NewItem{
            category_id:Some(category.id),name:"Tea".into(),item_type:ItemType::ReadyMade,
            purchase_cost:Some(2.0),selling_price:5.0
        }).unwrap();
        let payload=serde_json::json!({"item":item,"category_entity_id":cloud_category});
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-item-1",1)],
            |tx,_,_|apply_remote_item(tx,"other-device:item",&payload),
        ).unwrap();
        let local=crate::database::entity_ids::lookup(&conn,"items","other-device:item")
            .unwrap().unwrap();
        let mapped_category:i64=conn.query_row(
            "SELECT category_id FROM items WHERE id=?1",[local],|r|r.get(0)
        ).unwrap();
        assert_eq!(mapped_category,category.id);
        crate::database::connection::apply_cloud_event_batch(
            &mut conn,&[("remote-item-2",2)],
            |tx,_,_|apply_remote_item(tx,"other-device:item",&payload),
        ).unwrap();
        assert_eq!(crate::database::connection::read_cloud_pull_cursor(&conn).unwrap(),2);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(count,2);
    }

    #[test]
    fn category_creation_records_cloud_event() {
        let conn = db();
        let category = create_category(&conn, "Hot drinks", Some("Coffee and tea")).unwrap();
        let (entity_id, payload): (String, String) = conn.query_row(
            "SELECT entity_id,payload FROM cloud_outbox WHERE entity_type='categories'",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert!(entity_id.ends_with(&format!(":{}", category.id)));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap()["name"], "Hot drinks");
    }

    #[test]
    fn category_creation_registers_stable_cloud_identity() {
        let conn = db();
        let category = create_category(&conn, "Coffee", None).unwrap();
        let entity_id: String = conn.query_row(
            "SELECT entity_id FROM cloud_outbox WHERE entity_type='categories'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(
            crate::database::entity_ids::lookup(&conn, "categories", &entity_id).unwrap(),
            Some(category.id),
        );
    }

    #[test]
    fn category_creation_rolls_back_if_outbox_fails() {
        let conn = db();
        conn.execute_batch(
            "CREATE TRIGGER block_category_event BEFORE INSERT ON cloud_outbox
             BEGIN SELECT RAISE(FAIL, 'simulated outbox failure'); END;"
        ).unwrap();
        assert!(create_category(&conn, "Should not persist", None).is_err());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM categories WHERE name='Should not persist'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
        let mapping_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cloud_entity_ids WHERE entity_type='categories'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(mapping_count, 0);
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
    fn item_creation_records_cloud_event() {
        let conn = db();
        let item = create_item(&conn, NewItem {
            category_id: None, name: "Coffee".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(10.0), selling_price: 20.0,
        }).unwrap();
        let (entity_id, payload): (String, String) = conn.query_row(
            "SELECT entity_id, payload FROM cloud_outbox WHERE entity_type='items'",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert!(entity_id.ends_with(&format!(":{}", item.id)));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap()["item"]["name"], "Coffee");
    }

    #[test]
    fn item_creation_preserves_remote_category_identity() {
        let conn = db();
        let category = create_category(&conn, "Drinks", None).unwrap();
        conn.execute(
            "UPDATE cloud_entity_ids SET entity_id='remote:category'
             WHERE entity_type='categories' AND local_id=?1", [category.id],
        ).unwrap();
        create_item(&conn, NewItem {
            category_id: Some(category.id), name: "Tea".into(),
            item_type: ItemType::ReadyMade, purchase_cost: Some(2.0), selling_price: 5.0,
        }).unwrap();
        let payload: String = conn.query_row(
            "SELECT payload FROM cloud_outbox WHERE entity_type='items'", [], |r| r.get(0),
        ).unwrap();
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["category_entity_id"], "remote:category");
    }

    #[test]
    fn item_edits_keep_the_registered_cloud_identity() {
        let conn = db();
        let item = create_item(&conn, NewItem {
            category_id: None, name: "Tea".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(2.0), selling_price: 5.0,
        }).unwrap();
        // Simulate an item that originally came from a different installation.
        conn.execute(
            "UPDATE cloud_entity_ids SET entity_id='remote-device:42'
             WHERE entity_type='items' AND local_id=?1", [item.id],
        ).unwrap();
        set_item_active(&conn, item.id, false).unwrap();
        update_item_pricing(&conn, item.id, 6.0, Some(2.0)).unwrap();
        let mut stmt = conn.prepare(
            "SELECT entity_id FROM cloud_outbox WHERE entity_type='items' ORDER BY rowid DESC LIMIT 2"
        ).unwrap();
        let ids: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap()
            .map(|r| r.unwrap()).collect();
        assert_eq!(ids, vec!["remote-device:42", "remote-device:42"]);
    }

    #[test]
    fn item_creation_rolls_back_if_outbox_fails() {
        let conn = db();
        conn.execute_batch(
            "CREATE TRIGGER reject_item_event BEFORE INSERT ON cloud_outbox
             BEGIN SELECT RAISE(FAIL, 'outbox unavailable'); END;"
        ).unwrap();
        assert!(create_item(&conn, NewItem {
            category_id: None, name: "Unsaved".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(1.0), selling_price: 2.0,
        }).is_err());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items WHERE name='Unsaved'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn item_activation_and_pricing_enqueue_snapshots() {
        let conn = db();
        let item = create_item(&conn, NewItem {
            category_id: None, name: "Tea".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(3.0), selling_price: 7.0,
        }).unwrap();
        set_item_active(&conn, item.id, false).unwrap();
        update_item_pricing(&conn, item.id, 8.0, Some(3.0)).unwrap();
        let mut stmt = conn.prepare(
            "SELECT payload FROM cloud_outbox WHERE entity_type='items' ORDER BY rowid"
        ).unwrap();
        let payloads: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap()
            .map(|r| r.unwrap()).collect();
        assert_eq!(payloads.len(), 3);
        let inactive: serde_json::Value = serde_json::from_str(&payloads[1]).unwrap();
        let priced: serde_json::Value = serde_json::from_str(&payloads[2]).unwrap();
        assert_eq!(inactive["item"]["is_active"], false);
        assert_eq!(priced["item"]["selling_price"], 8.0);
    }

    #[test]
    fn item_update_rolls_back_when_outbox_fails() {
        let conn = db();
        let item = create_item(&conn, NewItem {
            category_id: None, name: "Tea".into(), item_type: ItemType::ReadyMade,
            purchase_cost: Some(3.0), selling_price: 7.0,
        }).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER stop_item_updates BEFORE INSERT ON cloud_outbox
             BEGIN SELECT RAISE(FAIL, 'outbox failure'); END;"
        ).unwrap();
        assert!(set_item_active(&conn, item.id, false).is_err());
        assert!(update_item_pricing(&conn, item.id, 8.0, Some(3.0)).is_err());
        let saved = conn.query_row("SELECT * FROM items WHERE id=?1", [item.id], Item::from_row).unwrap();
        assert!(saved.is_active);
        assert_eq!(saved.selling_price, 7.0);
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
