//! Error types for embedding operations

use thiserror::Error;

/// Result type for embedding operations
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

/// Errors that can occur during embedding operations
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// HTTP client error
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// Invalid API response
    #[error("Invalid API response: {0}")]
    InvalidResponse(String),

    /// Authentication error
    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimitExceeded { retry_after_secs: u64 },

    /// Provider-specific error
    #[error("Provider error: {provider}: {message}")]
    ProviderError { provider: String, message: String },

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Timeout error
    #[error("Request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    /// Circuit breaker open
    #[error("Circuit breaker open, too many failures")]
    CircuitBreakerOpen,

    /// Invalid embedding dimensions
    #[error("Invalid embedding dimensions: expected {expected}, got {actual}")]
    InvalidDimensions { expected: usize, actual: usize },

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Model discovery not supported by this provider
    #[error("Model discovery not supported by provider: {0}")]
    ModelDiscoveryNotSupported(String),

    /// Model not found
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Invalid model metadata
    #[error("Invalid model metadata: {0}")]
    InvalidModelMetadata(String),

    /// Generic error
    #[error("Embedding error: {0}")]
    Other(String),
}

impl EmbeddingError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            // Retryable errors
            EmbeddingError::HttpError(e) => {
                // Network errors and 5xx server errors are retryable
                e.is_timeout() || e.is_connect() || e.is_request()
                    || e.status().map(|s| s.is_server_error()).unwrap_or(false)
            }
            EmbeddingError::Timeout { .. } => true,
            EmbeddingError::RateLimitExceeded { .. } => true,
            
            // Non-retryable errors
            EmbeddingError::AuthenticationError(_) => false,
            EmbeddingError::ConfigError(_) => false,
            EmbeddingError::CircuitBreakerOpen => false,
            EmbeddingError::InvalidDimensions { .. } => false,
            EmbeddingError::InvalidResponse(_) => false,
            EmbeddingError::ProviderError { .. } => false,
            EmbeddingError::SerializationError(_) => false,
            EmbeddingError::ModelDiscoveryNotSupported(_) => false,
            EmbeddingError::ModelNotFound(_) => false,
            EmbeddingError::InvalidModelMetadata(_) => false,
            EmbeddingError::Other(_) => false,
        }
    }

    /// Get the recommended retry delay in seconds
    pub fn retry_delay_secs(&self) -> Option<u64> {
        match self {
            EmbeddingError::RateLimitExceeded { retry_after_secs } => Some(*retry_after_secs),
            EmbeddingError::HttpError(_) => Some(1), // Start with 1 second
            EmbeddingError::Timeout { .. } => Some(2),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        let timeout_err = EmbeddingError::Timeout { timeout_secs: 30 };
        assert!(timeout_err.is_retryable());

        let auth_err = EmbeddingError::AuthenticationError("Invalid key".to_string());
        assert!(!auth_err.is_retryable());

        let rate_limit = EmbeddingError::RateLimitExceeded { retry_after_secs: 60 };
        assert!(rate_limit.is_retryable());
        assert_eq!(rate_limit.retry_delay_secs(), Some(60));
    }

    #[test]
    fn test_error_display() {
        let err = EmbeddingError::InvalidDimensions { expected: 1536, actual: 768 };
        assert!(err.to_string().contains("expected 1536"));
        assert!(err.to_string().contains("got 768"));
    }
}
