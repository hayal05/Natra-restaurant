//! Daily local Excel (.xlsx) backup.
//!
//! Independent of `sync` (and works whether or not sync is enabled):
//! once a day, if `settings.backup_enabled` is on, this writes a full
//! snapshot of every business table to `app_data_dir/backups/` as a
//! plain .xlsx file, and prunes anything past the last 30 days. No
//! network, no cloud account — just a file on disk a restaurant owner
//! can open in Excel or hand to their accountant.
//!
//! - `export` builds the workbook itself (one sheet per table).
//! - `manager` decides *when* to run, writes the file, prunes old
//!   ones, and tracks state in `backup_meta` (mirrors `sync_meta`).

pub mod export;
pub mod manager;

pub use manager::{backup_dir_for_app, force_now, run_now, start_background, BackupReport, BackupStatus};
