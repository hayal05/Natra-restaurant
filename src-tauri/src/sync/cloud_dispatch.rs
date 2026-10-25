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
        "sale_reversals" => sales_service::apply_remote_sale_reversal(tx, payload),
        "waiter_settlements" => waiter_service::apply_remote_waiter_settlement(tx, payload),
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
