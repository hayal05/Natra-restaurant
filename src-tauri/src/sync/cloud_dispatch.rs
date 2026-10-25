//! Cloud pull event routing only. Network transfer is deliberately not enabled.
//! Handlers run inside apply_cloud_event_batch's receipt/cursor transaction.
use rusqlite::Transaction;
use serde::Deserialize;
use crate::errors::{AppError, AppResult};
use crate::services::{
    financial_service, inventory_service, month_closing_service,
    raw_material_service, sales_service, waiter_service,
};

#[derive(Debug, Deserialize)]
pub(crate) struct DownloadedEvent {
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub payload: serde_json::Value,
}

/// Reject unsupported events rather than advancing the cursor without applying them.
#[allow(dead_code)]
pub(crate) fn dispatch(tx: &Transaction<'_>, event: &DownloadedEvent) -> AppResult<()> {
    if event.entity_id.trim().is_empty() || event.operation != "upsert" {
        return Err(AppError::Validation("invalid cloud event identity or operation".into()));
    }
    let payload = &event.payload;
    match event.entity_type.as_str() {
        "categories" => inventory_service::apply_remote_category(tx, &event.entity_id, payload),
        "items" => inventory_service::apply_remote_item(tx, &event.entity_id, payload),
        "waiters" => waiter_service::apply_remote_waiter(tx, &event.entity_id, payload),
        "raw_materials" => raw_material_service::apply_remote_raw_material(tx, &event.entity_id, payload),
        "raw_material_purchases" => raw_material_service::apply_remote_purchase(tx, &event.entity_id, payload),
        "expenses" => financial_service::apply_remote_expense(tx, &event.entity_id, payload),
        "sales" => sales_service::apply_remote_sale(tx, &event.entity_id, payload),
        "sale_reversals" => {
            if payload.get("sale_entity_id").and_then(|v| v.as_str())
                != Some(event.entity_id.as_str()) {
                return Err(AppError::Validation("reversal identity does not match cloud event".into()));
            }
            sales_service::apply_remote_sale_reversal(tx, payload)
        }
        "waiter_settlements" => {
            if payload.get("waiter_entity_id").and_then(|v| v.as_str())
                != Some(event.entity_id.as_str()) {
                return Err(AppError::Validation("settlement identity does not match cloud event".into()));
            }
            waiter_service::apply_remote_waiter_settlement(tx, payload)
        },
        "material_month_closings" => {
            if payload.get("action").and_then(|v| v.as_str()) == Some("reopen") {
                return month_closing_service::apply_remote_closing_reopen(tx, payload);
            }
            let closing = payload.get("closing").ok_or_else(|| {
                AppError::Validation("missing month closing payload".into())
            })?;
            let month = closing.get("month").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("missing closing month".into()))?;
            let existing = tx.query_row(
                "SELECT status FROM raw_material_month_closings WHERE period_month=?1",
                [month], |row| row.get::<_, String>(0),
            ).optional()?;
            match (existing.as_deref(), closing.get("status").and_then(|v| v.as_str())) {
                (None, Some("draft" | "closed")) =>
                    month_closing_service::apply_remote_initial_closing(tx, payload),
                (Some("draft"), Some("draft")) =>
                    month_closing_service::apply_remote_closing_draft_revision(tx, payload),
                (Some("draft"), Some("closed")) =>
                    month_closing_service::apply_remote_closing_finalization(tx, payload),
                _ => Err(AppError::Validation("invalid month closing transition".into())),
            }
        }
        _ => Err(AppError::Validation("unsupported cloud event type".into())),
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{connection, migrations};
    use rusqlite::Connection;

    #[test]
    fn financial_updates_reject_mismatched_cloud_target_before_mutation() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        for (entity_type,payload) in [
            ("sale_reversals",serde_json::json!({"sale_entity_id":"another:sale"})),
            ("waiter_settlements",serde_json::json!({"waiter_entity_id":"another:waiter"})),
        ] {
            let event=DownloadedEvent {
                entity_type:entity_type.into(),entity_id:"remote:target".into(),
                operation:"upsert".into(),payload,
            };
            assert!(connection::apply_cloud_event_batch(
                &mut conn,&[("bad-target-event",1)],|tx,_,_|dispatch(tx,&event)
            ).is_err());
            assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),0);
            assert!(!connection::is_cloud_event_applied(&conn,"bad-target-event").unwrap());
        }
    }

    #[test]
    fn downloaded_sale_maps_waiter_and_item_and_keeps_financial_totals() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let category = DownloadedEvent {
            entity_type:"categories".into(), entity_id:"other:category".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"name":"Imported Drinks","description":null}),
        };
        let item = DownloadedEvent {
            entity_type:"items".into(),entity_id:"other:item".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "category_entity_id":"other:category",
                "item":{"id":51,"category_id":51,"name":"Tea",
                    "type":"ready_made","purchase_cost":2.0,"selling_price":5.0,
                    "is_active":true,"created_at":"2025-01-01 00:00:00",
                    "updated_at":"2025-01-01 00:00:00"}
            }),
        };
        let waiter = DownloadedEvent {
            entity_type:"waiters".into(),entity_id:"other:waiter".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"waiter":{
                "id":71,"full_name":"Imported Waiter","phone":null,
                "profile_photo":null,"is_active":true,
                "created_at":"2025-01-01 00:00:00",
                "updated_at":"2025-01-01 00:00:00"
            }}),
        };
        let sale = DownloadedEvent {
            entity_type:"sales".into(),entity_id:"other:sale".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "origin_installation_id":"other",
                "waiter_entity_id":"other:waiter",
                "item_entity_ids":["other:item"],
                "sale":{"id":93,"waiter_id":71,"user_id":null,
                    "total_quantity":2,"total_amount":10.0,
                    "total_cost":4.0,"total_profit":6.0,
                    "payment_method":"cash","is_settled":false,"settled_at":null,
                    "is_reversed":false,"reversed_at":null,"note":null,
                    "created_at":"2025-01-01 00:00:00",
                    "updated_at":"2025-01-01 00:00:00"},
                "items":[{"id":94,"sale_id":93,"item_id":51,
                    "item_name":"Tea","item_type":"ready_made","quantity":2,
                    "unit_price":5.0,"unit_cost":2.0,
                    "line_total":10.0,"line_cost":4.0,
                    "created_at":"2025-01-01 00:00:00"}]
            }),
        };
        let events=[&category,&item,&waiter,&sale];
        connection::apply_cloud_event_batch(
            &mut conn,
            &[("remote-cat",1),("remote-item",2),("remote-waiter",3),("remote-sale",4)],
            |tx,_,seq|dispatch(tx,events[(seq-1) as usize]),
        ).unwrap();
        let waiter_id = crate::database::entity_ids::lookup(
            &conn,"waiters","other:waiter").unwrap().unwrap();
        let item_id = crate::database::entity_ids::lookup(
            &conn,"items","other:item").unwrap().unwrap();
        let sale_id = crate::database::entity_ids::lookup(
            &conn,"sales","other:sale").unwrap().unwrap();
        let (linked_waiter,total,profit):(i64,f64,f64)=conn.query_row(
            "SELECT waiter_id,total_amount,total_profit FROM sales WHERE id=?1",
            [sale_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).unwrap();
        assert_eq!((linked_waiter,total,profit),(waiter_id,10.0,6.0));
        let (linked_item,qty,line_total):(i64,i64,f64)=conn.query_row(
            "SELECT item_id,quantity,line_total FROM sale_items WHERE sale_id=?1",
            [sale_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).unwrap();
        assert_eq!((linked_item,qty,line_total),(item_id,2,10.0));
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),4);
        assert!(connection::is_cloud_event_applied(&conn,"remote-sale").unwrap());
        // A distinct cloud event ID must not duplicate the same financial sale.
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("duplicate-sale-event",5)],
            |tx,_,_|dispatch(tx,&sale),
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),4);
        assert!(!connection::is_cloud_event_applied(&conn,"duplicate-sale-event").unwrap());
        let sale_count:i64=conn.query_row(
            "SELECT COUNT(*) FROM sales",[],|r|r.get(0),
        ).unwrap();
        let line_count:i64=conn.query_row(
            "SELECT COUNT(*) FROM sale_items",[],|r|r.get(0),
        ).unwrap();
        assert_eq!((sale_count,line_count),(1,1));
    }

    #[test]
    fn oversized_remote_sale_quantities_cannot_overflow_or_advance_cursor() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let waiter = DownloadedEvent {
            entity_type:"waiters".into(),entity_id:"remote:overflow-waiter".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"waiter":{
                "id":71,"full_name":"Overflow Test","phone":null,"profile_photo":null,
                "is_active":true,"created_at":"2025-01-01 00:00:00",
                "updated_at":"2025-01-01 00:00:00"}}),
        };
        let category = DownloadedEvent {
            entity_type:"categories".into(),entity_id:"remote:overflow-category".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"name":"Overflow Category","description":null}),
        };
        let item = DownloadedEvent {
            entity_type:"items".into(),entity_id:"remote:overflow-item".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "category_entity_id":"remote:overflow-category",
                "item":{"id":88,"category_id":88,"name":"Test Item",
                    "type":"ready_made","purchase_cost":0.0,"selling_price":0.0,
                    "is_active":true,"created_at":"2025-01-01 00:00:00",
                    "updated_at":"2025-01-01 00:00:00"}
            }),
        };
        let dependencies=[&waiter,&category,&item];
        connection::apply_cloud_event_batch(
            &mut conn,&[("overflow-waiter",1),("overflow-cat",2),("overflow-item",3)],
            |tx,_,seq|dispatch(tx,dependencies[(seq-1) as usize]),
        ).unwrap();
        let huge = i64::MAX;
        let sale = DownloadedEvent {
            entity_type:"sales".into(),entity_id:"remote:overflow-sale".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "origin_installation_id":"remote",
                "waiter_entity_id":"remote:overflow-waiter",
                "item_entity_ids":["remote:overflow-item","remote:overflow-item"],
                "sale":{"id":99,"waiter_id":71,"user_id":null,
                    "total_quantity":huge,"total_amount":0.0,"total_cost":0.0,
                    "total_profit":0.0,"payment_method":"cash","is_settled":false,
                    "settled_at":null,"is_reversed":false,"reversed_at":null,"note":null,
                    "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"},
                "items":[
                    {"id":1,"sale_id":99,"item_id":88,"item_name":"Test Item",
                        "item_type":"ready_made","quantity":huge,"unit_price":0.0,
                        "unit_cost":0.0,"line_total":0.0,"line_cost":0.0,
                        "created_at":"2025-01-01 00:00:00"},
                    {"id":2,"sale_id":99,"item_id":88,"item_name":"Test Item",
                        "item_type":"ready_made","quantity":1,"unit_price":0.0,
                        "unit_cost":0.0,"line_total":0.0,"line_cost":0.0,
                        "created_at":"2025-01-01 00:00:00"}]
            }),
        };
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("overflow-sale-event",4)],|tx,_,_|dispatch(tx,&sale)
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),3);
        assert!(!connection::is_cloud_event_applied(&conn,"overflow-sale-event").unwrap());
        assert_eq!(crate::database::entity_ids::lookup(
            &conn,"sales","remote:overflow-sale").unwrap(),None);
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM sales",[],|r|r.get::<_,i64>(0)
        ).unwrap(),0);
    }

    #[test]
    fn negative_remote_sale_values_roll_back_with_cloud_receipt() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let sale = DownloadedEvent {
            entity_type:"sales".into(),entity_id:"remote:negative-sale".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "origin_installation_id":"remote",
                "waiter_entity_id":"remote:waiter",
                "item_entity_ids":["remote:item"],
                "sale":{"id":42,"waiter_id":99,"user_id":null,
                    "total_quantity":1,"total_amount":-5.0,"total_cost":2.0,
                    "total_profit":-7.0,"payment_method":"cash","is_settled":false,
                    "settled_at":null,"is_reversed":false,"reversed_at":null,"note":null,
                    "created_at":"2025-01-01 00:00:00",
                    "updated_at":"2025-01-01 00:00:00"},
                "items":[{"id":77,"sale_id":42,"item_id":99,"item_name":"Tea",
                    "item_type":"ready_made","quantity":1,"unit_price":-5.0,
                    "unit_cost":2.0,"line_total":-5.0,"line_cost":2.0,
                    "created_at":"2025-01-01 00:00:00"}]
            }),
        };
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("negative-sale-event",1)],
            |tx,_,_|dispatch(tx,&sale)
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),0);
        assert!(!connection::is_cloud_event_applied(&conn,"negative-sale-event").unwrap());
        assert_eq!(crate::database::entity_ids::lookup(
            &conn,"sales","remote:negative-sale").unwrap(),None);
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM sales",[],|r|r.get::<_,i64>(0)
        ).unwrap(),0);
    }

    #[test]
    fn downloaded_sale_requires_mapped_waiter_and_items_and_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let waiter = DownloadedEvent {
            entity_type:"waiters".into(),entity_id:"remote:waiter".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"waiter":{
                "id":99,"full_name":"Remote Server","phone":null,"profile_photo":null,
                "is_active":true,"created_at":"2025-01-01 00:00:00",
                "updated_at":"2025-01-01 00:00:00"
            }}),
        };
        let sale = DownloadedEvent {
            entity_type:"sales".into(),entity_id:"remote:sale".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "origin_installation_id":"remote","waiter_entity_id":"remote:waiter",
                "item_entity_ids":["missing:item"],
                "sale":{"id":42,"waiter_id":99,"user_id":null,
                    "total_quantity":1,"total_amount":5.0,"total_cost":2.0,
                    "total_profit":3.0,"payment_method":"cash","is_settled":false,
                    "settled_at":null,"is_reversed":false,"reversed_at":null,"note":null,
                    "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"},
                "items":[{"id":77,"sale_id":42,"item_id":99,"item_name":"Tea",
                    "item_type":"ready_made","quantity":1,"unit_price":5.0,
                    "unit_cost":2.0,"line_total":5.0,"line_cost":2.0,
                    "created_at":"2025-01-01 00:00:00"}]
            }),
        };
        let batch=[&waiter,&sale];
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("remote-waiter-event",1),("remote-sale-event",2)],
            |tx,_,seq|dispatch(tx,batch[(seq-1) as usize]),
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),0);
        assert_eq!(crate::database::entity_ids::lookup(&conn,"waiters","remote:waiter").unwrap(),None);
        assert!(!connection::is_cloud_event_applied(&conn,"remote-waiter-event").unwrap());
        let sales:i64=conn.query_row("SELECT COUNT(*) FROM sales",[],|r|r.get(0)).unwrap();
        assert_eq!(sales,0);
    }

    #[test]
    fn financial_purchase_and_expense_batch_commits_or_rolls_back_together() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let material = DownloadedEvent {
            entity_type:"raw_materials".into(),entity_id:"device-b:material".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"material":{
                "id":91,"name":"Imported Beans","unit":"kg","is_active":true,
                "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"
            }}),
        };
        connection::apply_cloud_event_batch(
            &mut conn,&[("material-event",1)],|tx,_,_|dispatch(tx,&material)
        ).unwrap();
        let purchase = DownloadedEvent {
            entity_type:"raw_material_purchases".into(),entity_id:"device-b:purchase".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "raw_material_entity_id":"device-b:material",
                "purchase":{"id":12,"raw_material_id":91,"quantity":3.0,
                    "unit_cost":4.0,"total_cost":12.0,"supplier":null,
                    "purchase_date":"2025-01-02 00:00:00","note":null,
                    "created_at":"2025-01-02 00:00:00","updated_at":"2025-01-02 00:00:00"}
            }),
        };
        let expense = DownloadedEvent {
            entity_type:"expenses".into(),entity_id:"device-b:expense".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"expense":{
                "id":23,"category":"Rent","description":null,"amount":25.0,
                "expense_date":"2025-01-02 00:00:00","note":null,
                "created_at":"2025-01-02 00:00:00","updated_at":"2025-01-02 00:00:00"
            }}),
        };
        let events = [&purchase,&expense];
        connection::apply_cloud_event_batch(
            &mut conn,&[("purchase-event",2),("expense-event",3)],
            |tx,_,seq|dispatch(tx,events[(seq-2) as usize]),
        ).unwrap();
        let purchase_total: f64 = conn.query_row(
            "SELECT SUM(total_cost) FROM raw_material_purchases",[],|r|r.get(0)
        ).unwrap();
        let expense_total: f64 = conn.query_row(
            "SELECT SUM(amount) FROM expenses",[],|r|r.get(0)
        ).unwrap();
        assert_eq!((purchase_total,expense_total),(12.0,25.0));
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),3);

        // A new receipt for an already imported purchase must not double
        // count material cost or move the download cursor.
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("duplicate-purchase-receipt",4)],
            |tx,_,_|dispatch(tx,&purchase),
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),3);
        assert!(!connection::is_cloud_event_applied(
            &conn,"duplicate-purchase-receipt").unwrap());
        assert_eq!(conn.query_row(
            "SELECT SUM(total_cost) FROM raw_material_purchases",
            [],|r|r.get::<_,f64>(0),
        ).unwrap(),12.0);

        // A distinct cloud receipt cannot import the same expense twice.
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("duplicate-expense-receipt",4)],
            |tx,_,_|dispatch(tx,&expense),
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),3);
        assert!(!connection::is_cloud_event_applied(
            &conn,"duplicate-expense-receipt").unwrap());
        assert_eq!(conn.query_row(
            "SELECT SUM(amount) FROM expenses",[],|r|r.get::<_,f64>(0),
        ).unwrap(),25.0);

        let bad_purchase = DownloadedEvent {
            entity_type:"raw_material_purchases".into(),entity_id:"device-c:purchase".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "raw_material_entity_id":"device-b:material",
                "purchase":{"id":56,"raw_material_id":91,"quantity":2.0,
                    "unit_cost":6.0,"total_cost":999.0,"supplier":null,
                    "purchase_date":"2025-01-02 00:00:00","note":null,
                    "created_at":"2025-01-02 00:00:00","updated_at":"2025-01-02 00:00:00"}
            }),
        };
        let another_expense = DownloadedEvent {
            entity_type:"expenses".into(),entity_id:"device-c:expense".into(),
            operation:"upsert".into(),payload:expense.payload.clone(),
        };
        let failed = [&another_expense,&bad_purchase];
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("later-expense-event",4),("invalid-purchase-event",5)],
            |tx,_,seq|dispatch(tx,failed[(seq-4) as usize]),
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),3);
        assert!(!connection::is_cloud_event_applied(&conn,"later-expense-event").unwrap());
        assert_eq!(crate::database::entity_ids::lookup(
            &conn,"expenses","device-c:expense").unwrap(),None);
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM raw_material_purchases",[],|r|r.get::<_,i64>(0)
        ).unwrap(),1);
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM expenses",[],|r|r.get::<_,i64>(0)
        ).unwrap(),1);
    }

    #[test]
    fn linked_category_and_item_commit_together_or_roll_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let category = DownloadedEvent {
            entity_type:"categories".into(),entity_id:"device-b:category".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"name":"Snacks","description":null}),
        };
        let item = DownloadedEvent {
            entity_type:"items".into(),entity_id:"device-b:item".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "category_entity_id":"device-b:category",
                "item":{"id":99,"category_id":99,"name":"Biscuit","type":"ready_made",
                    "purchase_cost":2.0,"selling_price":5.0,"is_active":true,
                    "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"}
            }),
        };
        let events = [&category,&item];
        connection::apply_cloud_event_batch(
            &mut conn,&[("category-event",1),("item-event",2)],
            |tx,_,seq|dispatch(tx,events[(seq-1) as usize]),
        ).unwrap();
        let category_id = crate::database::entity_ids::lookup(
            &conn,"categories","device-b:category").unwrap().unwrap();
        let item_id = crate::database::entity_ids::lookup(
            &conn,"items","device-b:item").unwrap().unwrap();
        let linked: i64 = conn.query_row(
            "SELECT category_id FROM items WHERE id=?1",[item_id],|r|r.get(0),
        ).unwrap();
        assert_eq!(linked,category_id);
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),2);

        let another_category = DownloadedEvent {
            entity_type:"categories".into(),entity_id:"device-c:category".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({"name":"Other","description":null}),
        };
        let invalid_item = DownloadedEvent {
            entity_type:"items".into(),entity_id:"device-c:item".into(),
            operation:"upsert".into(),
            payload:serde_json::json!({
                "category_entity_id":"missing:category",
                "item":{"id":5,"category_id":5,"name":"Failed","type":"ready_made",
                    "purchase_cost":1.0,"selling_price":3.0,"is_active":true,
                    "created_at":"2025-01-01 00:00:00","updated_at":"2025-01-01 00:00:00"}
            }),
        };
        let failed = [&another_category,&invalid_item];
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("category-event-3",3),("item-event-4",4)],
            |tx,_,seq|dispatch(tx,failed[(seq-3) as usize]),
        ).is_err());
        assert_eq!(crate::database::entity_ids::lookup(
            &conn,"categories","device-c:category").unwrap(),None);
        assert!(!connection::is_cloud_event_applied(&conn,"category-event-3").unwrap());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),2);
    }

    #[test]
    fn dispatch_categories_in_batch_and_reject_unknown_events_atomically() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        let good = DownloadedEvent {
            entity_type: "categories".into(), entity_id: "remote:42".into(),
            operation: "upsert".into(),
            payload: serde_json::json!({"name":"Remote category","description":null}),
        };
        connection::apply_cloud_event_batch(&mut conn,&[("event-1",1)],|tx,_,_|dispatch(tx,&good)).unwrap();
        assert!(crate::database::entity_ids::lookup(&conn,"categories","remote:42").unwrap().is_some());
        let bad = DownloadedEvent {
            entity_type: "unrecognized".into(),entity_id:"remote:1".into(),
            operation:"upsert".into(),payload:serde_json::json!({}),
        };
        assert!(connection::apply_cloud_event_batch(
            &mut conn,&[("event-2",2)],|tx,_,_|dispatch(tx,&bad)
        ).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(),1);
    }
}
