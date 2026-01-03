//! LanceDB Error Types
//!
//! Provides error handling for LanceDB operations with conversion
//! to crucible-core StorageError.

use thiserror::Error;

/// Error type for LanceDB operations
#[derive(Error, Debug)]
pub enum LanceError {
    /// Database connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Table operation error
    #[error("Table error: {0}")]
    Table(String),

    /// Query execution error
    #[error("Query error: {0}")]
    Query(String),

    /// Schema error
    #[error("Schema error: {0}")]
    Schema(String),

    /// Data conversion error
    #[error("Conversion error: {0}")]
    Conversion(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Arrow error
    #[error("Arrow error: {0}")]
    Arrow(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(String),
}

/// Result type for LanceDB operations
pub type LanceResult<T> = Result<T, LanceError>;

impl From<lancedb::Error> for LanceError {
    fn from(err: lancedb::Error) -> Self {
        // LanceDB errors are typically string-based
        LanceError::Table(err.to_string())
    }
}

impl From<arrow_schema::ArrowError> for LanceError {
    fn from(err: arrow_schema::ArrowError) -> Self {
        LanceError::Arrow(err.to_string())
    }
}

impl From<serde_json::Error> for LanceError {
    fn from(err: serde_json::Error) -> Self {
        LanceError::Serialization(err.to_string())
    }
}

impl From<std::io::Error> for LanceError {
    fn from(err: std::io::Error) -> Self {
        LanceError::Io(err.to_string())
    }
}

impl From<LanceError> for crucible_core::storage::StorageError {
    fn from(err: LanceError) -> Self {
        crucible_core::storage::StorageError::Backend(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lance_error_display() {
        let err = LanceError::Connection("failed to connect".to_string());
        assert!(err.to_string().contains("Connection error"));
        assert!(err.to_string().contains("failed to connect"));
    }

    #[test]
    fn test_lance_error_to_storage_error() {
        let lance_err = LanceError::Table("table not found".to_string());
        let storage_err: crucible_core::storage::StorageError = lance_err.into();
        assert!(storage_err.to_string().contains("table not found"));
    }
}
