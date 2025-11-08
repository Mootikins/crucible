//! Storage Error Types
//!
//! Comprehensive error handling for content-addressed storage operations.

use thiserror::Error;

/// Comprehensive error type for storage operations
#[derive(Error, Debug, Clone)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Hash computation error: {0}")]
    HashComputation(String),

    #[error("Invalid hash format: {0}")]
    InvalidHash(String),

    #[error("Block not found: {hash}")]
    BlockNotFound { hash: String },

    #[error("Tree not found: {root_hash}")]
    TreeNotFound { root_hash: String },

    #[error("Storage backend error: {0}")]
    Backend(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Corrupted data detected: {0}")]
    CorruptedData(String),

    #[error("Tree validation failed: {0}")]
    TreeValidation(String),

    #[error("Block size error: {0}")]
    BlockSize(String),

    #[error("Memory allocation error: {0}")]
    MemoryAllocation(String),

    #[error("Concurrent access error: {0}")]
    ConcurrentAccess(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Storage quota exceeded: {used}/{limit} bytes")]
    QuotaExceeded { used: u64, limit: u64 },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Timeout error: operation timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Invalid index: {0}")]
    InvalidIndex(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

impl StorageError {
    /// Create a generic backend error
    pub fn backend<S: Into<String>>(msg: S) -> Self {
        Self::Backend(msg.into())
    }

    /// Create a serialization error
    pub fn serialization<S: Into<String>>(msg: S) -> Self {
        Self::Serialization(msg.into())
    }

    /// Create a deserialization error
    pub fn deserialization<S: Into<String>>(msg: S) -> Self {
        Self::Deserialization(msg.into())
    }

    /// Create a hash computation error
    pub fn hash_computation<S: Into<String>>(msg: S) -> Self {
        Self::HashComputation(msg.into())
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Io(_) | Self::Network(_) | Self::Timeout { .. } => true,
            Self::ConcurrentAccess(_) => true,
            _ => false,
        }
    }

    /// Check if the error indicates data corruption
    pub fn is_corruption(&self) -> bool {
        matches!(
            self,
            Self::CorruptedData(_) | Self::TreeValidation(_) | Self::HashComputation(_)
        )
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        assert!(StorageError::Io("timeout".to_string()).is_retryable());

        assert!(StorageError::Network("connection failed".to_string()).is_retryable());

        assert!(!StorageError::BlockNotFound {
            hash: "abc123".to_string()
        }
        .is_retryable());
    }

    #[test]
    fn test_error_corruption() {
        assert!(StorageError::CorruptedData("invalid checksum".to_string()).is_corruption());

        assert!(StorageError::TreeValidation("invalid hash".to_string()).is_corruption());

        assert!(!StorageError::Io("not found".to_string()).is_corruption());
    }
}
