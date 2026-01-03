//! Error types for SQLite storage

use thiserror::Error;

/// SQLite storage error type
#[derive(Error, Debug)]
pub enum SqliteError {
    /// Database connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Query execution error
    #[error("Query error: {0}")]
    Query(String),

    /// Schema/migration error
    #[error("Schema error: {0}")]
    Schema(String),

    /// Pool error
    #[error("Pool error: {0}")]
    Pool(String),

    /// Entity not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Underlying rusqlite error
    #[error("SQLite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

/// Result type for SQLite operations
pub type SqliteResult<T> = Result<T, SqliteError>;

impl From<SqliteError> for crucible_core::storage::StorageError {
    fn from(err: SqliteError) -> Self {
        match err {
            SqliteError::Connection(msg) => Self::Backend(msg),
            SqliteError::Query(msg) => Self::Backend(msg),
            SqliteError::Schema(msg) => Self::Backend(msg),
            SqliteError::Pool(msg) => Self::Backend(msg),
            SqliteError::NotFound(msg) => Self::Backend(format!("Not found: {}", msg)),
            SqliteError::InvalidOperation(msg) => {
                Self::Backend(format!("Invalid operation: {}", msg))
            }
            SqliteError::Serialization(msg) => Self::Serialization(msg),
            SqliteError::Rusqlite(e) => Self::Backend(e.to_string()),
        }
    }
}
