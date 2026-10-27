//! Real SQLite/service integration across independent installations. The ordered
//! in-process ledger is a transport fixture, not a PostgreSQL concurrency test.
use super::cloud_dispatch::{self, PendingUpload, SequencedDownload};
use crate::database::{connection::{self, Db}, entity_ids};
use crate::models::{ItemType, PaymentMethod, Sale};
use crate::services::{inventory_service as inventory, sales_service as sales, waiter_service as waiters};
use rusqlite::Connection;

fn ledger_page(events: &[PendingUpload], after: i64) -> Vec<SequencedDownload> {
    events.iter().enumerate().map(|(index, event)| {
        let mut wire = serde_json::to_value(event).unwrap();
        wire["sequence_id"] = (after + index as i64 + 1).to_string().into();
        serde_json::from_value(wire).unwrap()
    }).collect()
}

fn acknowledge(conn: &mut Connection, events: &[PendingUpload]) {
    let ids = events.iter().map(|event| event.event_id.clone()).collect::<Vec<_>>();
    cloud_dispatch::acknowledge_uploads(conn, &ids).unwrap();
}

fn catalog(conn: &Connection, name: &str) -> (i64, i64) {
    let category = inventory::create_category(conn, name, None).unwrap();
    let item = inventory::create_item(conn, inventory::NewItem {
        category_id: Some(category.id), name: name.into(), item_type: ItemType::ReadyMade,
        purchase_cost: Some(3.0), selling_price: 10.0,
    }).unwrap();
    let waiter = waiters::create_waiter(conn, name, None, None).unwrap();
    (item.id, waiter.id)
}

fn assert_sale(conn: &Connection, sale_id: i64, item_id: i64, waiter_id: i64, settled: bool) {
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM sales", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    let sale = conn.query_row("SELECT * FROM sales WHERE id=?1", [sale_id], Sale::from_row).unwrap();
    assert_eq!((sale.total_quantity, sale.total_amount, sale.total_cost, sale.total_profit), (2, 20.0, 6.0, 14.0));
    assert_eq!((sale.waiter_id, sale.is_settled, sale.is_reversed), (waiter_id, settled, false));
    let line: (i64, i64, f64, f64) = conn.query_row(
        "SELECT item_id,quantity,line_total,line_cost FROM sale_items WHERE sale_id=?1", [sale_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).unwrap();
    assert_eq!(line, (item_id, 2, 20.0, 6.0));
    assert_eq!(waiters::get_receivable(conn, waiter_id).unwrap(), if settled { 0.0 } else { 20.0 });
}

#[test]
fn two_installations_checkout_settlement_echo_and_restart_preserve_financials() {
    let a = Db::init_in_memory().unwrap();
    let b = Db::init_in_memory().unwrap();
    let mut ac = a.lock();
    let mut bc = b.lock();
    let a_id = connection::ensure_installation_id(&ac).unwrap();
    let b_id = connection::ensure_installation_id(&bc).unwrap();
    assert_ne!(a_id, b_id);
    // Occupied B IDs ensure replay must resolve stable identities.
    let (dummy_item, dummy_waiter) = catalog(&bc, "B local catalog");
    let (a_item, a_waiter) = catalog(&ac, "A shared catalog");
    let a_sale = sales::checkout(&mut ac, sales::CheckoutRequest {
        waiter_id: a_waiter, user_id: None, payment_method: PaymentMethod::Cash,
        lines: vec![sales::CartLine { item_id: a_item, quantity: 2 }], note: None,
    }).unwrap().sale.id;
    let a_events = cloud_dispatch::read_pending_uploads(&ac, 500).unwrap();
    assert_eq!(a_events.len(), 4);
    let sale_identity = a_events.iter().find(|e| e.entity_type == "sales").unwrap().entity_id.clone();
    let item_identity = a_events.iter().find(|e| e.entity_type == "items").unwrap().entity_id.clone();
    let waiter_identity = a_events.iter().find(|e| e.entity_type == "waiters").unwrap().entity_id.clone();
    let first_page = ledger_page(&a_events, 0);
    acknowledge(&mut ac, &a_events);
    cloud_dispatch::apply_download_page(&mut ac, &first_page).unwrap();
    cloud_dispatch::apply_download_page(&mut bc, &first_page).unwrap();
    let b_item = entity_ids::lookup(&bc, "items", &item_identity).unwrap().unwrap();
    let b_waiter = entity_ids::lookup(&bc, "waiters", &waiter_identity).unwrap().unwrap();
    let b_sale = entity_ids::lookup(&bc, "sales", &sale_identity).unwrap().unwrap();
    assert_ne!(a_item, b_item);
    assert_ne!(a_waiter, b_waiter);
    assert_ne!(dummy_item, b_item);
    assert_ne!(dummy_waiter, b_waiter);
    assert_sale(&ac, a_sale, a_item, a_waiter, false);
    assert_sale(&bc, b_sale, b_item, b_waiter, false);
    // Remote replay generated no outbox work: B has only its three local writes.
    assert_eq!(cloud_dispatch::read_pending_uploads(&bc, 500).unwrap().len(), 3);
    assert_eq!(waiters::settle_waiter(&bc, b_waiter).unwrap(), 1);
    assert_eq!(waiters::settle_waiter(&bc, b_waiter).unwrap(), 0);
    let b_events = cloud_dispatch::read_pending_uploads(&bc, 500).unwrap();
    assert_eq!(b_events.len(), 4);
    let settlement = b_events.iter().find(|e| e.entity_type == "waiter_settlements").unwrap();
    assert_eq!(settlement.entity_id, waiter_identity);
    assert_eq!(settlement.payload["sale_entity_ids"][0], sale_identity);
    let second_page = ledger_page(&b_events, first_page.len() as i64);
    acknowledge(&mut bc, &b_events);
    cloud_dispatch::apply_download_page(&mut ac, &second_page).unwrap();
    cloud_dispatch::apply_download_page(&mut bc, &second_page).unwrap();
    let cursor = (a_events.len() + b_events.len()) as i64;
    for conn in [&mut *ac, &mut *bc] {
        assert_eq!(connection::read_cloud_pull_cursor(conn).unwrap(), cursor);
        assert!(cloud_dispatch::read_pending_uploads(conn, 500).unwrap().is_empty());
        assert!(cloud_dispatch::apply_download_page(conn, &second_page).is_err());
        assert_eq!(connection::read_cloud_pull_cursor(conn).unwrap(), cursor);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM cloud_applied_events", [], |r| r.get::<_, i64>(0)).unwrap(), cursor);
    }
    assert_sale(&ac, a_sale, a_item, a_waiter, true);
    assert_sale(&bc, b_sale, b_item, b_waiter, true);
    // Save a consistent SQLite snapshot, reopen through real startup, then check
    // durable receipts, identity mapping, acknowledged queue and business rows.
    let path = std::env::temp_dir().join(format!("natra-multi-device-{b_id}.sqlite"));
    bc.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
    drop(bc);
    drop(b);
    {
        let restarted = Db::init(&path).unwrap();
        let mut conn = restarted.lock();
        assert_eq!(connection::ensure_installation_id(&conn).unwrap(), b_id);
        assert_eq!(connection::read_cloud_pull_cursor(&conn).unwrap(), cursor);
        assert_eq!(entity_ids::lookup(&conn, "items", &item_identity).unwrap(), Some(b_item));
        assert!(cloud_dispatch::read_pending_uploads(&conn, 500).unwrap().is_empty());
        for event in a_events.iter().chain(&b_events) {
            assert!(connection::is_cloud_event_applied(&conn, &event.event_id).unwrap());
        }
        assert!(cloud_dispatch::apply_download_page(&mut conn, &first_page).is_err());
        assert_sale(&conn, b_sale, b_item, b_waiter, true);
    }
    std::fs::remove_file(&path).unwrap();
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}
