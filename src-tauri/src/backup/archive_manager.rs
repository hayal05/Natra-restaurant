//! Offline, non-overwriting permanent XLSX transaction batches.
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use zip::ZipArchive;

use crate::backup::archive_sales::build_sales_archive;
use crate::database::Db;
use crate::errors::{AppError, AppResult};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const REQUIRED_PARTS: &[&str] = &[
    "[Content_Types].xml", "_rels/.rels", "xl/workbook.xml",
    "xl/_rels/workbook.xml.rels",
    "xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml",
    "xl/worksheets/sheet3.xml", "xl/worksheets/sheet4.xml",
    "xl/worksheets/sheet5.xml", "xl/worksheets/sheet6.xml",
];

#[derive(Debug, Serialize)]
pub struct ArchiveResult {
    pub batch_id: String,
    pub path: String,
    pub already_exists: bool,
}

pub fn archive_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let path = app.path().app_data_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?.join("transaction-archives");
    fs::create_dir_all(&path).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(path)
}

pub fn eat_window(from: &str, through: &str) -> AppResult<(String, String)> {
    let first = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("invalid start date".into()))?;
    let last = NaiveDate::parse_from_str(through, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("invalid end date".into()))?;
    if first > last { return Err(AppError::Validation("start date must not exceed end date".into())); }
    let next = last.succ_opt().ok_or_else(|| AppError::Validation("end date overflow".into()))?;
    let offset = FixedOffset::east_opt(10800).unwrap();
    let start = DateTime::<FixedOffset>::from_local(first.and_hms_opt(0, 0, 0).unwrap(), offset).with_timezone(&Utc);
    let end = DateTime::<FixedOffset>::from_local(next.and_hms_opt(0, 0, 0).unwrap(), offset).with_timezone(&Utc);
    Ok((start.format("%Y-%m-%d %H:%M:%S").to_string(),
        end.format("%Y-%m-%d %H:%M:%S").to_string()))
}

/// Validate the OOXML ZIP container, all six worksheet parts and every member CRC.
/// A valid ZIP alone is not proof that amounts reconcile; financial tests are separate.
fn verify_xlsx(bytes: &[u8]) -> AppResult<()> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::Internal(format!("invalid XLSX ZIP: {e}")))?;
    for name in REQUIRED_PARTS {
        zip.by_name(name).map_err(|e| AppError::Internal(format!("missing XLSX part {name}: {e}")))?;
    }
    for i in 0..zip.len() {
        let mut part = zip.by_index(i).map_err(|e| AppError::Internal(e.to_string()))?;
        if part.is_dir() { continue; }
        // Reading to the end makes the ZIP reader check CRC32, catching truncation.
        std::io::copy(&mut part, &mut std::io::sink())
            .map_err(|e| AppError::Internal(format!("corrupt XLSX member: {e}")))?;
    }
    Ok(())
}

pub fn export_range(db: &Db, directory: &Path, from: &str, through: &str) -> AppResult<ArchiveResult> {
    let (start, end) = eat_window(from, through)?;
    fs::create_dir_all(directory).map_err(|e| AppError::Internal(e.to_string()))?;
    let batch_id = format!("EAT-{}-{}", from.replace('-', ""), through.replace('-', ""));
    let final_path = directory.join(format!("natra-sales-{batch_id}.xlsx"));
    if final_path.exists() {
        // Never report a corrupted existing archive as a successful batch.
        verify_xlsx(&fs::read(&final_path).map_err(|e| AppError::Internal(e.to_string()))?)?;
        return Ok(ArchiveResult { batch_id, path: final_path.to_string_lossy().into_owned(), already_exists: true });
    }
    let bytes = {
        let conn = db.lock();
        build_sales_archive(&conn, &batch_id, &start, &end)?
    };
    verify_xlsx(&bytes)?;

    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = directory.join(format!(".natra-sales-{batch_id}-{}-{seq}.tmp", std::process::id()));
    let result = (|| -> AppResult<bool> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)
            .map_err(|e| AppError::Internal(format!("cannot create archive temp: {e}")))?;
        file.write_all(&bytes).and_then(|_| file.sync_all())
            .map_err(|e| AppError::Internal(format!("cannot write archive: {e}")))?;
        drop(file);
        verify_xlsx(&fs::read(&temp).map_err(|e| AppError::Internal(e.to_string()))?)?;
        match fs::hard_link(&temp, &final_path) {
            Ok(()) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                verify_xlsx(&fs::read(&final_path).map_err(|e| AppError::Internal(e.to_string()))?)?;
                Ok(true)
            }
            Err(e) => Err(AppError::Internal(format!("cannot finalize archive: {e}"))),
        }
    })();
    // Only remove this invocation's own temp file. Never remove unknown files
    // from a previous process, which could still be exporting concurrently.
    let _ = fs::remove_file(&temp);
    let already_exists = result?;
    Ok(ArchiveResult { batch_id, path: final_path.to_string_lossy().into_owned(), already_exists })
}

/// Return the earliest transaction date in EAT. Empty restaurants still archive
/// the latest completed day, without generating years of empty history.
fn earliest_day(db: &Db, yesterday: NaiveDate) -> AppResult<NaiveDate> {
    let conn = db.lock();
    let first: Option<String> = conn.query_row(
        "SELECT MIN(created_at) FROM sales", [], |r| r.get(0))?;
    let Some(utc) = first else { return Ok(yesterday); };
    let naive = chrono::NaiveDateTime::parse_from_str(&utc, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| AppError::Internal(format!("invalid source timestamp: {e}")))?;
    let east = FixedOffset::east_opt(10800).unwrap();
    let date = naive.and_utc().with_timezone(&east).date_naive();
    Ok(date.min(yesterday))
}

/// Find every missing daily archive, limited per invocation to avoid blocking
/// the offline POS on large histories. Each hourly pass continues the backlog.
fn catch_up(db: &Db, dir: &Path, today: NaiveDate, max_batches: usize) -> AppResult<usize> {
    let yesterday = today.pred_opt().ok_or_else(|| AppError::Internal("invalid date".into()))?;
    let mut date = earliest_day(db, yesterday)?;
    let mut created = 0;
    while date <= yesterday && created < max_batches {
        let day = date.format("%Y-%m-%d").to_string();
        let id = format!("EAT-{}-{}", day.replace('-', ""), day.replace('-', ""));
        let path = dir.join(format!("natra-sales-{id}.xlsx"));
        if !path.exists() {
            export_range(db, dir, &day, &day)?;
            created += 1;
        } else {
            // Existing files may have been tampered with; surface errors.
            verify_xlsx(&fs::read(&path).map_err(|e| AppError::Internal(e.to_string()))?)?;
        }
        date = date.succ_opt().ok_or_else(|| AppError::Internal("date overflow".into()))?;
    }
    Ok(created)
}

pub fn start_daily(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let eat = FixedOffset::east_opt(10800).unwrap();
            let today = Utc::now().with_timezone(&eat).date_naive();
            if let Ok(dir) = archive_dir(&app) {
                let db = app.state::<Db>();
                if let Err(e) = catch_up(db.inner(), &dir, today, 30) {
                    eprintln!("[archive] catch-up failed: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
}


#[derive(Debug, Serialize)]
pub struct ArchiveEntry {
    pub filename: String,
    pub batch_id: String,
    pub size_bytes: u64,
    pub valid: bool,
}

/// Directory enumeration never trusts a filename from the UI.
pub fn list_archives(directory: &Path) -> AppResult<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(directory).map_err(|e| AppError::Internal(e.to_string()))? {
        let item = item.map_err(|e| AppError::Internal(e.to_string()))?;
        let path = item.path();
        if !path.is_file() { continue; }
        let Some(filename) = path.file_name().and_then(|v| v.to_str()) else { continue };
        if !filename.starts_with("natra-sales-EAT-") || !filename.ends_with(".xlsx") { continue; }
        let batch_id = filename.trim_start_matches("natra-sales-").trim_end_matches(".xlsx").to_string();
        let metadata = fs::metadata(&path).map_err(|e| AppError::Internal(e.to_string()))?;
        let valid = fs::read(&path).map(|v| verify_xlsx(&v).is_ok()).unwrap_or(false);
        entries.push(ArchiveEntry { filename: filename.to_string(), batch_id, size_bytes: metadata.len(), valid });
    }
    entries.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;
    use rusqlite::Connection;

    fn test_db() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        migrations::run(&conn).unwrap();
        Db(std::sync::Mutex::new(conn))
    }
    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("natra-archive-test-{}-{}",
            std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn eat_midnight_window() {
        let (start, end) = eat_window("2026-10-19", "2026-10-19").unwrap();
        assert_eq!(start, "2026-10-18 21:00:00");
        assert_eq!(end, "2026-10-19 21:00:00");
    }
    #[test]
    fn reversed_window_is_invalid() {
        assert!(eat_window("2026-10-20", "2026-10-19").is_err());
    }
    #[test]
    fn detects_corrupted_workbook() {
        assert!(verify_xlsx(b"PKnot a full ZIP").is_err());
    }
    #[test]
    fn exports_and_validates_six_sheets() {
        let db = test_db();
        let dir = test_dir();
        let result = export_range(&db, &dir, "2026-10-19", "2026-10-19").unwrap();
        verify_xlsx(&fs::read(&result.path).unwrap()).unwrap();
        let again = export_range(&db, &dir, "2026-10-19", "2026-10-19").unwrap();
        assert!(again.already_exists);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refuses_corrupted_existing_batch() {
        let db = test_db();
        let dir = test_dir();
        let result = export_range(&db, &dir, "2026-10-19", "2026-10-19").unwrap();
        fs::write(&result.path, b"corrupt").unwrap();
        assert!(export_range(&db, &dir, "2026-10-19", "2026-10-19").is_err());
        assert_eq!(fs::read(&result.path).unwrap(), b"corrupt");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recovers_after_orphaned_temp_file() {
        let db = test_db();
        let dir = test_dir();
        let orphan = dir.join(".natra-sales-EAT-20261019-20261019-old.tmp");
        fs::write(&orphan, b"incomplete").unwrap();
        let result = export_range(&db, &dir, "2026-10-19", "2026-10-19").unwrap();
        assert!(Path::new(&result.path).exists());
        assert_eq!(fs::read(&orphan).unwrap(), b"incomplete");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fills_multiple_missing_days_and_does_not_reexport() {
        let db = test_db();
        {
            let conn = db.lock();
            conn.execute("INSERT INTO waiters (full_name) VALUES ('Abebe Kebede')", []).unwrap();
            conn.execute("INSERT INTO sales (waiter_id, created_at) VALUES (1, '2026-10-16 10:00:00')", []).unwrap();
        }
        let dir = test_dir();
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        assert_eq!(catch_up(&db, &dir, today, 30).unwrap(), 4);
        assert_eq!(catch_up(&db, &dir, today, 30).unwrap(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn resumes_limited_backlog() {
        let db = test_db();
        {
            let conn = db.lock();
            conn.execute("INSERT INTO waiters (full_name) VALUES ('Abebe Kebede')", []).unwrap();
            conn.execute("INSERT INTO sales (waiter_id, created_at) VALUES (1, '2026-10-16 10:00:00')", []).unwrap();
        }
        let dir = test_dir();
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        assert_eq!(catch_up(&db, &dir, today, 2).unwrap(), 2);
        assert_eq!(catch_up(&db, &dir, today, 2).unwrap(), 2);
        assert_eq!(catch_up(&db, &dir, today, 2).unwrap(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
