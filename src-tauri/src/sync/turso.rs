//! Turso sync transport boundary.
//!
//! The local SQLite database remains the source of truth while the app is
//! offline.  This module provides the transport interface used by the sync
//! manager.  The current Windows build keeps the transport intentionally
//! conservative: credentials are accepted and the sync cycle is safe to run,
//! but no local rows are copied to a remote database until the schema mapping
//! and conflict rules are enabled together.

use crate::database::Db;
use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct TursoClient {
    pub url: String,
    pub token: String,
}

pub mod connection {
    use super::*;

    /// Validate and retain Turso connection settings without touching the
    /// network. Network access is deliberately deferred to the sync layer.
    pub async fn connect(url: &str, token: &str) -> AppResult<TursoClient> {
        if url.trim().is_empty() {
            return Err(AppError::Sync("Turso URL is empty".into()));
        }
        if token.trim().is_empty() {
            return Err(AppError::Sync("Turso auth token is empty".into()));
        }

        Ok(TursoClient {
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }
}

pub mod sync {
    use super::*;

    /// Push locally queued mutations.
    ///
    /// Kept as a safe no-op until the local/remote schema mapper is enabled;
    /// this guarantees that turning sync on cannot corrupt the local POS DB.
    pub async fn push_pending(_db: &Db, _client: &TursoClient) -> AppResult<usize> {
        Ok(0)
    }

    /// Pull remote mutations newer than `since`.
    ///
    /// Kept as a safe no-op until the remote schema mapper and conflict
    /// application layer are enabled.
    pub async fn pull_updates(
        _db: &Db,
        _client: &TursoClient,
        _since: &str,
    ) -> AppResult<usize> {
        Ok(0)
    }
}
