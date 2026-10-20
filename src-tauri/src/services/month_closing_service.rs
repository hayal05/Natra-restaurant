//! Month-end raw-material valuation preview. No writes: Task 3 owns closing lifecycle.
//! Purchases are filtered on Ethiopia-local calendar boundaries, stored as UTC SQLite strings.
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
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
    fn east_africa_month_boundary() {
        let c = db();
        let m = raw_material_service::create_raw_material(&c,"Oil","L").unwrap();
        c.execute("INSERT INTO raw_material_purchases(raw_material_id,quantity,unit_cost,total_cost,purchase_date) VALUES(?1,1,10,10,'2026-09-30 21:30:00')",[m.id]).unwrap();
        let b = preview(&c,"2026-10-01").unwrap().remove(0);
        assert_eq!(b.purchased_quantity,1.0);
    }
}
