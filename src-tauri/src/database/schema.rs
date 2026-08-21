//! Shared constants that mirror the SQL CHECK constraints in /migrations.
//!
//! Keeping these as constants (rather than retyping string literals
//! everywhere) means the `models`/`services` layers can't typo a status
//! value that the database would silently reject.

pub mod item_type {
    pub const READY_MADE: &str = "ready_made";
    pub const COOKABLE: &str = "cookable";
}

pub mod user_role {
    pub const ADMIN: &str = "admin";
    pub const STAFF: &str = "staff";
}

pub mod payment_method {
    pub const CASH: &str = "cash";
    pub const CARD: &str = "card";
    pub const OTHER: &str = "other";
}

pub mod sync_status {
    pub const PENDING: &str = "pending";
    pub const SYNCED: &str = "synced";
    pub const FAILED: &str = "failed";
}

pub mod sync_operation {
    pub const INSERT: &str = "insert";
    pub const UPDATE: &str = "update";
    pub const DELETE: &str = "delete";
}
