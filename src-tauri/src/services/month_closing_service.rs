//! Month-end raw-material valuation preview. No writes: Task 3 owns closing lifecycle.
//! Purchases are filtered on Ethiopia-local calendar boundaries, stored as UTC SQLite strings.
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use crate::database::transactions::with_transaction;
use serde::Serialize;
use crate::errors::{AppError, AppResult};

const EPS: f64 = 0.000001;

#[derive(Debug, Clone, Serialize)]
pub struct MaterialBalance {
    pub raw_material_id: i64,
    pub name: String,
    pub unit: String,
    pub opening_quantity: f64,
    pub opening_value: f64,
    pub purchased_quantity: f64,
    pub purchased_value: f64,
    pub available_quantity: f64,
    pub available_value: f64,
    pub weighted_unit_cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValuedClosing {
    pub raw_material_id: i64,
    pub opening_quantity: f64,
    pub opening_value: f64,
    pub purchased_quantity: f64,
    pub purchased_value: f64,
    pub remaining_quantity: f64,
    pub closing_value: f64,
    pub consumed_value: f64,
}

fn period(month: &str) -> AppResult<(NaiveDate, NaiveDate)> {
    let day = NaiveDate::parse_from_str(month, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("month must be YYYY-MM-01".into()))?;
    if day.day() != 1 { return Err(AppError::Validation("month must be first day".into())); }
    let next = if day.month() == 12 {
        NaiveDate::from_ymd_opt(day.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(day.year(), day.month() + 1, 1)
    }.ok_or_else(|| AppError::Validation("invalid month".into()))?;
    Ok((day, next))
}

fn utc_start(day: NaiveDate) -> String {
    let east = FixedOffset::east_opt(3 * 3600).unwrap();
    east.from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .single().unwrap().with_timezone(&chrono::Utc)
        .format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Reads the immediately preceding *closed* month. If the previous month has
/// a draft closing, refuse to compute a misleading opening stock. The first
/// month in a ledger legitimately starts with zero opening stock.
pub fn preview(conn: &Connection, month: &str) -> AppResult<Vec<MaterialBalance>> {
    let (start, next) = period(month)?;
    let previous = start.pred_opt().ok_or_else(|| AppError::Validation("invalid preceding month".into()))?;
    let previous_key = previous.with_day(1).unwrap().format("%Y-%m-%d").to_string();
    let previous_closing: Option<(i64, String)> = conn.query_row(
        "SELECT id,status FROM raw_material_month_closings WHERE period_month=?1",
        [&previous_key], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional()?;
    if let Some((_, status)) = &previous_closing {
        if status != "closed" {
            return Err(AppError::Validation("previous month must be closed before carry-forward".into()));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT m.id,m.name,m.unit,           COALESCE(l.remaining_quantity,0),COALESCE(l.closing_value,0),           COALESCE(SUM(p.quantity),0),COALESCE(SUM(p.total_cost),0)          FROM raw_materials m          LEFT JOIN raw_material_month_closing_lines l ON l.raw_material_id=m.id AND l.closing_id=?1          LEFT JOIN raw_material_purchases p ON p.raw_material_id=m.id              AND p.purchase_date >= ?2 AND p.purchase_date < ?3          GROUP BY m.id,m.name,m.unit,l.remaining_quantity,l.closing_value ORDER BY m.name"
    )?;
    let prev_id: Option<i64> = previous_closing.map(|(id, _)| id);
    let start_utc = utc_start(start);
    let next_utc = utc_start(next);
    let rows = stmt.query_map(params![prev_id, start_utc, next_utc], |r| {
        let oq: f64 = r.get(3)?;
        let ov: f64 = r.get(4)?;
        let pq: f64 = r.get(5)?;
        let pv: f64 = r.get(6)?;
        let aq = oq + pq;
        let av = ov + pv;
        Ok(MaterialBalance {
            raw_material_id: r.get(0)?, name: r.get(1)?, unit: r.get(2)?,
            opening_quantity: oq, opening_value: ov,
            purchased_quantity: pq, purchased_value: pv,
            available_quantity: aq, available_value: av,
            weighted_unit_cost: if aq > EPS { av / aq } else { 0.0 },
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>,_>>()?)
}

/// Apply user-entered remaining quantity. Computes the value retained and
/// material cost consumed, without counting carry-forward as a new purchase.
pub fn value_remaining(b: &MaterialBalance, remaining: f64) -> AppResult<ValuedClosing> {
    if !remaining.is_finite() || remaining < 0.0 || remaining > b.available_quantity + EPS {
        return Err(AppError::Validation(format!("invalid remaining quantity for {}", b.name)));
    }
    let qty = remaining.min(b.available_quantity);
    let value = if (qty - b.available_quantity).abs() < EPS {
        b.available_value
    } else { qty * b.weighted_unit_cost };
    Ok(ValuedClosing {
        raw_material_id: b.raw_material_id,
        opening_quantity: b.opening_quantity, opening_value: b.opening_value,
        purchased_quantity: b.purchased_quantity, purchased_value: b.purchased_value,
        remaining_quantity: qty, closing_value: value,
        consumed_value: (b.available_value - value).max(0.0),
    })
}


#[derive(Debug, Clone, Serialize)]
pub struct ClosingState {
    pub month: String,
    pub status: String,
    pub lines: Vec<ValuedClosing>,
}

fn latest_closing(conn: &Connection) -> AppResult<Option<(String, String)>> {
    Ok(conn.query_row(
        "SELECT period_month,status FROM raw_material_month_closings ORDER BY period_month DESC LIMIT 1",
        [], |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional()?)
}

fn existing_closing(conn: &Connection, month: &str) -> AppResult<Option<(i64, String)>> {
    Ok(conn.query_row(
        "SELECT id,status FROM raw_material_month_closings WHERE period_month=?1",
        [month], |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional()?)
}

fn check_sequence(conn: &Connection, month: &str) -> AppResult<()> {
    let (start, _) = period(month)?;
    let earliest: Option<String> = conn.query_row(
        "SELECT MIN(period_month) FROM raw_material_month_closings", [], |r| r.get(0)
    )?;
    if let Some(first) = earliest {
        if month < first.as_str() {
            return Err(AppError::Validation("cannot insert a month before the existing closing history".into()));
        }
    }
    // No gaps are allowed once the ledger begins.
    if let Some((latest, _)) = latest_closing(conn)? {
        if month > latest.as_str() {
            let (last, next) = period(&latest)?;
            let _ = last;
            let expected = next.format("%Y-%m-%d").to_string();
            if month != expected {
                return Err(AppError::Validation("close months consecutively without gaps".into()));
            }
        }
    }
    // An immediately preceding month, if recorded, must be closed.
    let previous = start.pred_opt().ok_or_else(|| AppError::Validation("invalid month".into()))?;
    let key = previous.with_day(1).unwrap().format("%Y-%m-%d").to_string();
    if let Some((_, status)) = existing_closing(conn, &key)? {
        if status != "closed" {
            return Err(AppError::Validation("close the preceding month first".into()));
        }
    }
    Ok(())
}

/// Save a full snapshot of manually entered remaining quantities. Changes are
/// atomic: an invalid line cannot partially overwrite an existing draft.
/// No purchase records are added or altered.
pub fn save_draft(
    conn: &mut Connection,
    month: &str,
    remaining: &[(i64, f64)],
) -> AppResult<ClosingState> {
    period(month)?;
    with_transaction(conn, |tx| {
        check_sequence(tx, month)?;
        let existing = existing_closing(tx, month)?;
        if matches!(existing, Some((_, ref status)) if status == "closed") {
            return Err(AppError::Validation("closed month must be explicitly reopened before editing".into()));
        }
        if let Some((latest, _)) = latest_closing(tx)? {
            if latest.as_str() > month {
                return Err(AppError::Validation("cannot edit an earlier period while a later closing exists".into()));
            }
        }
        let balances = preview(tx, month)?;
        let entries: HashMap<i64, f64> = remaining.iter().copied().collect();
        if entries.len() != remaining.len() {
            return Err(AppError::Validation("duplicate raw material in closing".into()));
        }
        let known: HashSet<i64> = balances.iter().map(|b| b.raw_material_id).collect();
        if entries.keys().any(|id| !known.contains(id)) {
            return Err(AppError::Validation("unknown raw material in closing".into()));
        }
        let valued: Vec<ValuedClosing> = balances.iter().filter_map(|b| entries.get(&b.raw_material_id)
            .map(|quantity| value_remaining(b, *quantity))).collect::<AppResult<_>>()?;
        let closing_id = if let Some((id, _)) = existing { id } else {
            tx.execute("INSERT INTO raw_material_month_closings(period_month) VALUES(?1)", [month])?;
            tx.last_insert_rowid()
        };
        tx.execute("DELETE FROM raw_material_month_closing_lines WHERE closing_id=?1", [closing_id])?;
        for b in &balances {
            let v = valued.iter().find(|v| v.raw_material_id == b.raw_material_id);
            tx.execute(
                "INSERT INTO raw_material_month_closing_lines (closing_id,raw_material_id,opening_quantity,opening_value,purchased_quantity,purchased_value,remaining_quantity,closing_value) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![closing_id,b.raw_material_id,b.opening_quantity,b.opening_value,b.purchased_quantity,b.purchased_value,v.map(|x|x.remaining_quantity),v.map(|x|x.closing_value)]
            )?;
        }
        tx.execute("UPDATE raw_material_month_closings SET updated_at=datetime('now') WHERE id=?1", [closing_id])?;
        let result = ClosingState { month: month.into(), status: "draft".into(), lines: valued };
        crate::sync::queue::enqueue_cloud_event(
            tx, "material_month_closings", month, "upsert",
            &serde_json::json!({ "closing": result }),
        )?;
        Ok(result)
    })
}

/// Finalizing requires a completed material inventory and a finished month.
/// The stored closing values become the next month's opening balances through
/// preview(). They are never inserted as purchases.
pub fn close_month(conn: &mut Connection, month: &str) -> AppResult<ClosingState> {
    let (_, next) = period(month)?;
    let today_eat = chrono::Utc::now().with_timezone(&FixedOffset::east_opt(10800).unwrap()).date_naive();
    if next > today_eat {
        return Err(AppError::Validation("cannot finalize a month that has not ended".into()));
    }
    with_transaction(conn, |tx| {
        check_sequence(tx, month)?;
        let (id, status) = existing_closing(tx, month)?
            .ok_or_else(|| AppError::Validation("save a draft before closing".into()))?;
        if status != "draft" {
            return Err(AppError::Validation("month is already closed".into()));
        }
        if let Some((latest, _)) = latest_closing(tx)? {
            if latest != month {
                return Err(AppError::Validation("close the latest month first".into()));
            }
        }
        // Refresh the draft against actual purchases at the time of closing,
        // preventing a stale draft from silently misvaluing late entries.
        let balances = preview(tx, month)?;
        let mut stmt = tx.prepare("SELECT raw_material_id,remaining_quantity FROM raw_material_month_closing_lines WHERE closing_id=?1")?;
        let entries: Vec<(i64, Option<f64>)> = stmt.query_map([id], |r| Ok((r.get(0)?,r.get(1)?)))?
            .collect::<Result<_,_>>()?;
        drop(stmt);
        if entries.len() != balances.len() || entries.iter().any(|(_, qty)| qty.is_none()) {
            return Err(AppError::Validation("all material remaining quantities must be entered".into()));
        }
        let values: HashMap<i64,f64> = entries.into_iter().map(|(id,q)| (id,q.unwrap())).collect();
        let mut computed = Vec::new();
        for b in balances {
            let qty = *values.get(&b.raw_material_id).ok_or_else(|| AppError::Validation("missing material".into()))?;
            let valued = value_remaining(&b, qty)?;
            tx.execute("UPDATE raw_material_month_closing_lines SET opening_quantity=?1,opening_value=?2,purchased_quantity=?3,purchased_value=?4,closing_value=?5 WHERE closing_id=?6 AND raw_material_id=?7",
                params![valued.opening_quantity,valued.opening_value,valued.purchased_quantity,valued.purchased_value,valued.closing_value,id,b.raw_material_id])?;
            computed.push(valued);
        }
        tx.execute("UPDATE raw_material_month_closings SET status='closed',closed_at=datetime('now'),updated_at=datetime('now') WHERE id=?1", [id])?;
        let result = ClosingState { month: month.into(), status: "closed".into(), lines: computed };
        // A closing is unique to its workspace and calendar month. Capture
        // the final valued lines in the same transaction as finalization.
        crate::sync::queue::enqueue_cloud_event(
            tx, "material_month_closings", month, "upsert",
            &serde_json::json!({ "closing": result }),
        )?;
        Ok(result)
    })
}

/// Controlled correction: reopen only the latest closed period and require a
/// written reason. Historical records remain intact; subsequent months cannot
/// silently inherit changed values while locked.
pub fn reopen_month(conn: &mut Connection, month: &str, reason: &str) -> AppResult<()> {
    if reason.trim().len() < 5 {
        return Err(AppError::Validation("a correction reason of at least five characters is required".into()));
    }
    with_transaction(conn, |tx| {
        let (id, status) = existing_closing(tx, month)?
            .ok_or_else(|| AppError::Validation("closing does not exist".into()))?;
        if status != "closed" {
            return Err(AppError::Validation("month is not closed".into()));
        }
        if latest_closing(tx)?.map(|(m,_)| m) != Some(month.to_string()) {
            return Err(AppError::Validation("reopen later months first to preserve carry-forward".into()));
        }
        tx.execute("INSERT INTO raw_material_closing_audit(closing_id,action,reason) VALUES(?1,'reopen',?2)",
            params![id,reason.trim()])?;
        tx.execute("UPDATE raw_material_month_closings SET status='draft',closed_at=NULL,updated_at=datetime('now') WHERE id=?1",[id])?;
        crate::sync::queue::enqueue_cloud_event(
            tx, "material_month_closings", month, "upsert",
            &serde_json::json!({
                "closing": { "month": month, "status": "draft" },
                "action": "reopen",
                "reason": reason.trim(),
            }),
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use crate::services::raw_material_service;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn weighted_average_and_consumption() {
        let c = db();
        let meat = raw_material_service::create_raw_material(&c,"Meat","kg").unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES (?1,10,200,2000,'2026-10-05 00:00:00')", [meat.id]).unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES (?1,10,400,4000,'2026-10-10 00:00:00')", [meat.id]).unwrap();
        let b = preview(&c,"2026-10-01").unwrap().remove(0);
        assert_eq!(b.purchased_quantity,20.0);
        assert_eq!(b.weighted_unit_cost,300.0);
        let result = value_remaining(&b,5.0).unwrap();
        assert_eq!(result.closing_value,1500.0);
        assert_eq!(result.consumed_value,4500.0);
        assert!(value_remaining(&b,21.0).is_err());
    }

    #[test]
    fn following_month_carries_value_without_new_purchase() {
        let c = db();
        let m = raw_material_service::create_raw_material(&c,"Meat","kg").unwrap();
        c.execute("INSERT INTO raw_material_month_closings(period_month,status,closed_at) VALUES('2026-10-01','closed',datetime('now'))", []).unwrap();
        let id = c.last_insert_rowid();
        c.execute("INSERT INTO raw_material_month_closing_lines(closing_id,raw_material_id,purchased_quantity,purchased_value,remaining_quantity,closing_value) VALUES(?1,?2,20,6000,5,1500)", params![id,m.id]).unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES (?1,5,500,2500,'2026-11-05 00:00:00')", [m.id]).unwrap();
        let b = preview(&c,"2026-11-01").unwrap().remove(0);
        assert_eq!(b.opening_quantity,5.0);
        assert_eq!(b.opening_value,1500.0);
        assert_eq!(b.purchased_quantity,5.0);
        assert_eq!(b.purchased_value,2500.0);
        assert_eq!(b.weighted_unit_cost,400.0);
        assert_eq!(value_remaining(&b,4.0).unwrap().consumed_value,2400.0);
    }

    #[test]
    fn save_close_carry_forward_and_reopen() {
        let mut c = db();
        let meat = raw_material_service::create_raw_material(&c,"Meat","kg").unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES(?1,20,300,6000,'2025-10-05 00:00:00')",[meat.id]).unwrap();
        let draft = save_draft(&mut c,"2025-10-01",&[(meat.id,5.0)]).unwrap();
        assert_eq!(draft.status,"draft");
        assert_eq!(draft.lines[0].closing_value,1500.0);
        let done = close_month(&mut c,"2025-10-01").unwrap();
        assert_eq!(done.lines[0].consumed_value,4500.0);
        assert!(close_month(&mut c,"2025-10-01").is_err());
        assert!(save_draft(&mut c,"2025-10-01",&[(meat.id,4.0)]).is_err());
        let november = preview(&c,"2025-11-01").unwrap();
        assert_eq!(november[0].opening_value,1500.0);
        assert_eq!(november[0].opening_quantity,5.0);
        assert!(reopen_month(&mut c,"2025-10-01","").is_err());
        reopen_month(&mut c,"2025-10-01","Correct count").unwrap();
        assert!(preview(&c,"2025-11-01").is_err());
        assert_eq!(save_draft(&mut c,"2025-10-01",&[(meat.id,4.0)]).unwrap().lines[0].closing_value,1200.0);
        assert_eq!(close_month(&mut c,"2025-10-01").unwrap().lines[0].consumed_value,4800.0);
        let count:i64=c.query_row("SELECT COUNT(*) FROM raw_material_closing_audit",[],|r|r.get(0)).unwrap();
        assert_eq!(count,1);
    }

    #[test]
    fn closing_finalization_queues_complete_month_snapshot() {
        let mut conn = db();
        let material = raw_material_service::create_raw_material(&conn, "Rice", "kg").unwrap();
        save_draft(&mut conn, "2025-10-01", &[(material.id, 0.0)]).unwrap();
        close_month(&mut conn, "2025-10-01").unwrap();
        let (identity, payload): (String, String) = conn.query_row(
            "SELECT entity_id,payload FROM cloud_outbox WHERE entity_type='material_month_closings'",
            [], |r| Ok((r.get(0)?,r.get(1)?)),
        ).unwrap();
        assert_eq!(identity, "2025-10-01");
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["closing"]["status"], "closed");
        assert_eq!(event["closing"]["lines"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn closing_finalization_rolls_back_if_outbox_fails() {
        let mut conn = db();
        let material = raw_material_service::create_raw_material(&conn, "Rice", "kg").unwrap();
        save_draft(&mut conn, "2025-10-01", &[(material.id, 0.0)]).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_closing_event BEFORE INSERT ON cloud_outbox
             WHEN NEW.entity_type='material_month_closings'
             BEGIN SELECT RAISE(FAIL, 'outbox failure'); END;"
        ).unwrap();
        assert!(close_month(&mut conn, "2025-10-01").is_err());
        let status: String = conn.query_row(
            "SELECT status FROM raw_material_month_closings WHERE period_month='2025-10-01'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(status, "draft");
    }

    #[test]
    fn draft_and_reopen_are_recorded_in_cloud_outbox() {
        let mut conn = db();
        let material = raw_material_service::create_raw_material(&conn, "Rice", "kg").unwrap();
        save_draft(&mut conn, "2025-10-01", &[(material.id, 0.0)]).unwrap();
        close_month(&mut conn, "2025-10-01").unwrap();
        reopen_month(&mut conn, "2025-10-01", "Count corrected").unwrap();
        let payloads: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT payload FROM cloud_outbox WHERE entity_type='material_month_closings' ORDER BY rowid"
            ).unwrap();
            let rows = stmt.query_map([], |r| r.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(payloads.len(), 3);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payloads[0]).unwrap()["closing"]["status"], "draft");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payloads[2]).unwrap()["action"], "reopen");
    }

    #[test]
    fn draft_and_reopen_roll_back_if_outbox_fails() {
        let mut conn = db();
        let material = raw_material_service::create_raw_material(&conn, "Rice", "kg").unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_closing_event BEFORE INSERT ON cloud_outbox
             WHEN NEW.entity_type='material_month_closings'
             BEGIN SELECT RAISE(FAIL, 'outbox unavailable'); END;"
        ).unwrap();
        assert!(save_draft(&mut conn, "2025-10-01", &[(material.id, 0.0)]).is_err());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM raw_material_month_closings", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
        conn.execute_batch("DROP TRIGGER reject_closing_event;").unwrap();
        save_draft(&mut conn, "2025-10-01", &[(material.id, 0.0)]).unwrap();
        close_month(&mut conn, "2025-10-01").unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_closing_event BEFORE INSERT ON cloud_outbox
             WHEN NEW.entity_type='material_month_closings'
             BEGIN SELECT RAISE(FAIL, 'outbox unavailable'); END;"
        ).unwrap();
        assert!(reopen_month(&mut conn, "2025-10-01", "Count corrected").is_err());
        let status: String = conn.query_row(
            "SELECT status FROM raw_material_month_closings WHERE period_month='2025-10-01'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(status, "closed");
    }

    #[test]
    fn invalid_drafts_roll_back_and_gaps_are_rejected() {
        let mut c = db();
        let meat = raw_material_service::create_raw_material(&c,"Meat","kg").unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES(?1,10,50,500,'2025-10-05 00:00:00')",[meat.id]).unwrap();
        assert!(save_draft(&mut c,"2025-10-01",&[(meat.id,11.0)]).is_err());
        let count:i64=c.query_row("SELECT COUNT(*) FROM raw_material_month_closings",[],|r|r.get(0)).unwrap();
        assert_eq!(count,0);
        save_draft(&mut c,"2025-10-01",&[(meat.id,3.0)]).unwrap();
        assert!(save_draft(&mut c,"2025-12-01",&[(meat.id,0.0)]).is_err());
        assert!(save_draft(&mut c,"2025-09-01",&[(meat.id,0.0)]).is_err());
        close_month(&mut c,"2025-10-01").unwrap();
        save_draft(&mut c,"2025-11-01",&[(meat.id,3.0)]).unwrap();
        close_month(&mut c,"2025-11-01").unwrap();
        assert!(reopen_month(&mut c,"2025-10-01","Correction request").is_err());
    }

    #[test]
    fn east_africa_month_boundary() {
        let c = db();
        let m = raw_material_service::create_raw_material(&c,"Oil","L").unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES(?1,1,10,10,'2026-09-30 21:30:00')",[m.id]).unwrap();
        let b = preview(&c,"2026-10-01").unwrap().remove(0);
        assert_eq!(b.purchased_quantity,1.0);
    }
}
