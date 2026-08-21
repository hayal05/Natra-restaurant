//! Central error type for the application.
//!
//! Every layer (database, services, commands) returns `AppResult<T>`.
//! Tauri commands convert this into a `String` at the boundary since
//! that's what the frontend receives as a rejected promise.

use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("system not initialized")]
    NotInitialized,

    #[error("system already initialized")]
    AlreadyInitialized,

    #[error("sync error: {0}")]
    Sync(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Turso/libSQL errors surface from `sync::turso` — collapsed to a
/// message string, same treatment as every other backend error once it
/// crosses into `AppError`.
impl From<libsql::Error> for AppError {
    fn from(err: libsql::Error) -> Self {
        AppError::Sync(err.to_string())
    }
}

/// Errors from writing the daily backup workbook (see `backup::export`)
/// — collapsed to a message string, same treatment as every other
/// backend error once it crosses into `AppError`.
impl From<rust_xlsxwriter::XlsxError> for AppError {
    fn from(err: rust_xlsxwriter::XlsxError) -> Self {
        AppError::Internal(format!("backup export failed: {err}"))
    }
}

/// Tauri commands must return values that implement `Serialize` for the
/// error case too, so the frontend gets a clean message instead of a
/// generic failure. We serialize AppError as a plain string.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
