//! Conflict resolution for pulled rows.
//!
//! Every syncable table already carries an `updated_at` column (default
//! `datetime('now')`, format `YYYY-MM-DD HH:MM:SS`), which is exactly
//! what last-write-wins needs and nothing more — no per-row version
//! numbers or vector clocks to maintain. Because the timestamp format
//! is fixed-width, plain string comparison sorts chronologically, so
//! no parsing is needed here.
//!
//! This intentionally does not try to be clever about merging fields
//! from both sides. Restaurant records (a waiter's name, an item's
//! price) are edited by one person at a time in practice; the rare
//! true collision is resolved wholesale in favor of whichever side
//! wrote most recently, and the losing edit is simply gone. That's a
//! deliberate trade-off for simplicity over a multi-device editing
//! system.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepLocal,
    KeepRemote,
}

/// Decide which side wins for a single row.
///
/// Ties (identical timestamps — most likely two devices pulling the
/// same unmodified row) favor the remote copy: it's the copy every
/// other device will converge on next, so preferring it keeps devices
/// from drifting apart over repeated no-op syncs.
pub fn resolve(local_updated_at: &str, remote_updated_at: &str) -> Resolution {
    match local_updated_at.cmp(remote_updated_at) {
        Ordering::Greater => Resolution::KeepLocal,
        Ordering::Less | Ordering::Equal => Resolution::KeepRemote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_local_edit_wins() {
        let res = resolve("2026-08-20 10:05:00", "2026-08-20 10:00:00");
        assert_eq!(res, Resolution::KeepLocal);
    }

    #[test]
    fn newer_remote_edit_wins() {
        let res = resolve("2026-08-20 10:00:00", "2026-08-20 10:05:00");
        assert_eq!(res, Resolution::KeepRemote);
    }

    #[test]
    fn tie_favors_remote() {
        let res = resolve("2026-08-20 10:00:00", "2026-08-20 10:00:00");
        assert_eq!(res, Resolution::KeepRemote);
    }

    #[test]
    fn fixed_width_timestamps_sort_correctly_across_date_boundaries() {
        // Would break under naive lexical comparison of non-padded
        // formats (e.g. "9:00" vs "10:00") — confirms the padded
        // `datetime('now')` format sorts correctly.
        let res = resolve("2026-08-09 23:59:00", "2026-08-10 00:01:00");
        assert_eq!(res, Resolution::KeepRemote);
    }
}
