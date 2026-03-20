//! Shared library error types.

use std::path::PathBuf;

use thiserror::Error;

/// Result alias for Patchworks library code.
pub type Result<T> = std::result::Result<T, PatchworksError>;

/// Errors returned by Patchworks backend and UI coordination code.
#[derive(Debug, Error)]
pub enum PatchworksError {
    /// Wrapper for filesystem I/O failures.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Wrapper for SQLite failures.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A requested table was not found in the inspected database.
    #[error("table `{table}` was not found in {path}")]
    MissingTable { table: String, path: PathBuf },
    /// The requested operation requires a database path.
    #[error("a database path is required for this operation")]
    MissingDatabasePath,
    /// The requested operation could not be completed because the data was inconsistent.
    #[error("{0}")]
    InvalidState(String),
}
