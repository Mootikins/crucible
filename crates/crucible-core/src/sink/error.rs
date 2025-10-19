//! Sink error types

use thiserror::Error;

/// Sink operation errors
#[derive(Debug, Error)]
pub enum SinkError {
    /// Write operation failed
    #[error("Write failed: {0}")]
    WriteFailed(String),

    /// Connection error to destination
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Operation timed out
    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    /// Sink is closed and cannot accept writes
    #[error("Sink closed")]
    Closed,

    /// Buffer is full (backpressure)
    #[error("Buffer full: {current}/{max} items")]
    BufferFull {
        /// Current buffer size
        current: usize,
        /// Maximum buffer capacity
        max: usize,
    },

    /// Serialization/encoding error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// General error
    #[error("Sink error: {0}")]
    Other(String),
}

/// Specialized Result type for sink operations
pub type SinkResult<T> = Result<T, SinkError>;

impl SinkError {
    /// Create a write failure error
    pub fn write_failed(msg: impl Into<String>) -> Self {
        Self::WriteFailed(msg.into())
    }

    /// Create a connection error
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::ConnectionError(msg.into())
    }

    /// Create a timeout error
    pub fn timeout(duration: std::time::Duration) -> Self {
        Self::Timeout(duration)
    }

    /// Create a buffer full error
    pub fn buffer_full(current: usize, max: usize) -> Self {
        Self::BufferFull { current, max }
    }

    /// Create a serialization error
    pub fn serialization(msg: impl Into<String>) -> Self {
        Self::SerializationError(msg.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::WriteFailed(_) | Self::ConnectionError(_) | Self::Timeout(_)
        )
    }

    /// Check if this error is fatal (not retryable)
    pub fn is_fatal(&self) -> bool {
        !self.is_retryable()
    }

    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            Self::WriteFailed(_) => "write_failed",
            Self::ConnectionError(_) => "connection",
            Self::Timeout(_) => "timeout",
            Self::Closed => "closed",
            Self::BufferFull { .. } => "buffer_full",
            Self::SerializationError(_) => "serialization",
            Self::ConfigError(_) => "config",
            Self::Other(_) => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_error_retryability() {
        let err = SinkError::write_failed("db error");
        assert!(err.is_retryable());
        assert!(!err.is_fatal());

        let err = SinkError::Closed;
        assert!(err.is_fatal());
        assert!(!err.is_retryable());

        let err = SinkError::timeout(Duration::from_secs(5));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_error_category() {
        let err = SinkError::write_failed("test");
        assert_eq!(err.category(), "write_failed");

        let err = SinkError::buffer_full(100, 100);
        assert_eq!(err.category(), "buffer_full");
    }

    #[test]
    fn test_error_display() {
        let err = SinkError::timeout(Duration::from_secs(5));
        assert_eq!(err.to_string(), "Timeout after 5s");

        let err = SinkError::buffer_full(100, 100);
        assert_eq!(err.to_string(), "Buffer full: 100/100 items");
    }
}
