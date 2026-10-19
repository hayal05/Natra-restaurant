//! Persistent local sales archives. Separate from rolling database backups.
//! Dates supplied by UI are Ethiopian Gregorian business dates (UTC+03:00).
use std::path::{Path, PathBuf};
use std::fs::{self, OpenOptions};
use std::io::Write;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::backup::archive_sales::build_sales_archive;
use crate::database::Db;
use crate::errors::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct ArchiveResult {
    pub batch_id: String,
    pub path: String,
    pub already_exists: bool,
}

pub fn archive_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let path = app.path().app_data_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .join("transaction-archives");
    fs::create_dir_all(&path).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(path)
}

/// Inclusive EAT calendar dates, converted to half-open UTC range.
pub fn eat_window(from: &str, through: &str) -> AppResult<(String, String)> {
    let first = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("invalid start date".into()))?;
    let last = NaiveDate::parse_from_str(through, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("invalid end date".into()))?;
    if first > last {
        return Err(AppError::Validation("start date must not exceed end date".into()));
    }
    let next = last.succ_opt().ok_or_else(|| AppError::Validation("end date overflow".into()))?;
    let offset = FixedOffset::east_opt(3 * 3600).unwrap();
    let start = first.and_hms_opt(0, 0, 0).unwrap();
    let end = next.and_hms_opt(0, 0, 0).unwrap();
    let start_utc = DateTime::<FixedOffset>::from_local(start, offset).with_timezone(&Utc);
    let end_utc = DateTime::<FixedOffset>::from_local(end, offset).with_timezone(&Utc);
    Ok((
        start_utc.format("%Y-%m-%d %H:%M:%S").to_string(),
        end_utc.format("%Y-%m-%d %H:%M:%S").to_string(),
    ))
}

/// Fixed, deterministic names make retries idempotent. No overwrite;
/// pre-existing archive is returned unchanged (manual export can use another range).
pub fn export_range(db: &Db, directory: &Path, from: &str, through: &str) -> AppResult<ArchiveResult> {
    let (start, end) = eat_window(from, through)?;
    fs::create_dir_all(directory).map_err(|e| AppError::Internal(e.to_string()))?;
    let batch_id = format!("EAT-{}-{}", from.replace('-', ""), through.replace('-', ""));
    let final_path = directory.join(format!("natra-sales-{batch_id}.xlsx"));
    if final_path.exists() {
        return Ok(ArchiveResult {
            batch_id, path: final_path.to_string_lossy().into_owned(), already_exists: true,
        });
    }

    let bytes = {
        let conn = db.lock();
        build_sales_archive(&conn, &batch_id, &start, &end)?
    };
    if !bytes.starts_with(b"PK") {
        return Err(AppError::Internal("archive workbook verification failed".into()));
    }
    // Exclusive creation: concurrent writers never replace an existing archive.
    // Incomplete temporary files are ignored on subsequent attempts.
    let temp = directory.join(format!(".natra-sales-{batch_id}-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true)
        .open(&temp).map_err(|e| AppError::Internal(e.to_string()))?;
    let write_result = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = write_result {
        let _ = fs::remove_file(&temp);
        return Err(AppError::Internal(format!("archive write failed: {e}")));
    }
    // Hard link creation is atomic and refuses overwrite, including races.
    let result = match fs::hard_link(&temp, &final_path) {
        Ok(()) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(true),
        Err(e) => Err(AppError::Internal(format!("archive finalize failed: {e}"))),
    };
    let _ = fs::remove_file(&temp);
    let already_exists = result?;
    Ok(ArchiveResult {
        batch_id, path: final_path.to_string_lossy().into_owned(), already_exists,
    })
}

/// Catch up on the latest completed EAT business day while the app is running.
/// Called at startup and periodically. Never exports today's incomplete transactions.
pub fn start_daily(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let eat = FixedOffset::east_opt(3 * 3600).unwrap();
            let yesterday = (Utc::now().with_timezone(&eat) - Duration::days(1))
                .format("%Y-%m-%d").to_string();
            if let Ok(dir) = archive_dir(&app) {
                let db = app.state::<Db>();
                if let Err(err) = export_range(db.inner(), &dir, &yesterday, &yesterday) {
                    eprintln!("[archive] daily export failed: {err}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
