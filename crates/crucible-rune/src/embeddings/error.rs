//! Error types for embedding operations

use thiserror::Error;

/// Result type for embedding operations
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

/// Errors that can occur during embedding operations
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("API error: {message} (status: {status})")]
    ApiError {
        message: String,
        status: u16,
    },

    #[error("Timeout error: operation timed out after {timeout_secs}s")]
    TimeoutError {
        timeout_secs: u64,
    },

    #[error("Rate limit error: {message}")]
    RateLimitError {
        message: String,
        retry_after_seconds: Option<u64>,
    },

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Model '{model}' not available: {message}")]
    ModelNotAvailable {
        model: String,
        message: String,
    },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Too many tokens: {token_count} (max: {max_tokens})")]
    TooManyTokens {
        token_count: usize,
        max_tokens: usize,
    },

    #[error("Embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        expected: usize,
        actual: usize,
    },

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl EmbeddingError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::NetworkError(_) => true,
            Self::TimeoutError { .. } => true,
            Self::RateLimitError { .. } => true,
            Self::ServiceUnavailable(_) => true,
            Self::ApiError { status, .. } => {
                // Retry on 5xx errors and 429 (rate limit)
                *status >= 500 || *status == 429
            }
            _ => false,
        }
    }

    /// Get suggested retry delay in seconds
    pub fn suggested_retry_delay_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimitError { retry_after_seconds, .. } => *retry_after_seconds,
            Self::TimeoutError { timeout_secs } => Some(*timeout_secs),
            Self::NetworkError(_) => Some(1),
            Self::ServiceUnavailable(_) => Some(5),
            Self::ApiError { status, .. } => {
                if *status == 429 {
                    Some(60) // Default retry after for rate limiting
                } else if *status >= 500 {
                    Some(5)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            Self::ConfigError(_) => "configuration",
            Self::NetworkError(_) => "network",
            Self::ApiError { .. } => "api",
            Self::TimeoutError { .. } => "timeout",
            Self::RateLimitError { .. } => "rate_limit",
            Self::InvalidResponse(_) => "invalid_response",
            Self::SerializationError(_) => "serialization",
            Self::IoError(_) => "io",
            Self::ModelNotAvailable { .. } => "model",
            Self::InvalidInput(_) => "input",
            Self::TooManyTokens { .. } => "tokens",
            Self::DimensionMismatch { .. } => "dimension",
            Self::ServiceUnavailable(_) => "service",
            Self::AuthenticationError(_) => "authentication",
            Self::QuotaExceeded(_) => "quota",
            Self::InternalError(_) => "internal",
        }
    }

    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::ConfigError(_) => ErrorSeverity::Error,
            Self::AuthenticationError(_) => ErrorSeverity::Error,
            Self::QuotaExceeded(_) => ErrorSeverity::Warning,
            Self::RateLimitError { .. } => ErrorSeverity::Warning,
            Self::TimeoutError { .. } => ErrorSeverity::Warning,
            Self::ServiceUnavailable(_) => ErrorSeverity::Error,
            Self::ModelNotAvailable { .. } => ErrorSeverity::Error,
            Self::TooManyTokens { .. } => ErrorSeverity::Warning,
            Self::InvalidInput(_) => ErrorSeverity::Warning,
            Self::NetworkError(_) => ErrorSeverity::Error,
            Self::ApiError { status, .. } => {
                if *status >= 500 {
                    ErrorSeverity::Error
                } else if *status == 429 {
                    ErrorSeverity::Warning
                } else {
                    ErrorSeverity::Error
                }
            }
            _ => ErrorSeverity::Error,
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
    Critical = 4,
}

/// Metrics for embedding errors
#[derive(Debug, Clone, Default)]
pub struct EmbeddingErrorMetrics {
    /// Total errors by category
    pub errors_by_category: std::collections::HashMap<String, u64>,
    /// Total errors by severity
    pub errors_by_severity: std::collections::HashMap<ErrorSeverity, u64>,
    /// Retryable errors
    pub retryable_errors: u64,
    /// Non-retryable errors
    pub non_retryable_errors: u64,
    /// Last error timestamp
    pub last_error: Option<chrono::DateTime<chrono::Utc>>,
}

impl EmbeddingErrorMetrics {
    /// Record an error
    pub fn record_error(&mut self, error: &EmbeddingError) {
        let category = error.category().to_string();
        let severity = error.severity();

        *self.errors_by_category.entry(category).or_insert(0) += 1;
        *self.errors_by_severity.entry(severity).or_insert(0) += 1;

        if error.is_retryable() {
            self.retryable_errors += 1;
        } else {
            self.non_retryable_errors += 1;
        }

        self.last_error = Some(chrono::Utc::now());
    }

    /// Get total error count
    pub fn total_errors(&self) -> u64 {
        self.errors_by_category.values().sum()
    }

    /// Get error rate (errors per minute)
    pub fn error_rate_per_minute(&self) -> f64 {
        if let Some(last_error) = self.last_error {
            let now = chrono::Utc::now();
            let minutes_elapsed = (now - last_error).num_minutes().max(1) as f64;
            self.total_errors() as f64 / minutes_elapsed
        } else {
            0.0
        }
    }

    /// Get retryable error rate
    pub fn retryable_error_rate(&self) -> f64 {
        let total = self.total_errors();
        if total > 0 {
            self.retryable_errors as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Reset metrics
    pub fn reset(&mut self) {
        self.errors_by_category.clear();
        self.errors_by_severity.clear();
        self.retryable_errors = 0;
        self.non_retryable_errors = 0;
        self.last_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        let network_error = EmbeddingError::NetworkError(
            reqwest::Error::from(reqwest::ErrorKind::Request)
        );
        assert!(network_error.is_retryable());

        let timeout_error = EmbeddingError::TimeoutError { timeout_secs: 30 };
        assert!(timeout_error.is_retryable());

        let config_error = EmbeddingError::ConfigError("test".to_string());
        assert!(!config_error.is_retryable());
    }

    #[test]
    fn test_suggested_retry_delay() {
        let rate_limit_error = EmbeddingError::RateLimitError {
            message: "Rate limited".to_string(),
            retry_after_seconds: Some(120),
        };
        assert_eq!(rate_limit_error.suggested_retry_delay_secs(), Some(120));

        let timeout_error = EmbeddingError::TimeoutError { timeout_secs: 30 };
        assert_eq!(timeout_error.suggested_retry_delay_secs(), Some(30));

        let config_error = EmbeddingError::ConfigError("test".to_string());
        assert_eq!(config_error.suggested_retry_delay_secs(), None);
    }

    #[test]
    fn test_error_category() {
        let config_error = EmbeddingError::ConfigError("test".to_string());
        assert_eq!(config_error.category(), "configuration");

        let network_error = EmbeddingError::NetworkError(
            reqwest::Error::from(reqwest::ErrorKind::Request)
        );
        assert_eq!(network_error.category(), "network");
    }

    #[test]
    fn test_error_severity() {
        let config_error = EmbeddingError::ConfigError("test".to_string());
        assert_eq!(config_error.severity(), ErrorSeverity::Error);

        let rate_limit_error = EmbeddingError::RateLimitError {
            message: "Rate limited".to_string(),
            retry_after_seconds: None,
        };
        assert_eq!(rate_limit_error.severity(), ErrorSeverity::Warning);
    }

    #[test]
    fn test_error_metrics() {
        let mut metrics = EmbeddingErrorMetrics::default();

        let config_error = EmbeddingError::ConfigError("test".to_string());
        metrics.record_error(&config_error);

        assert_eq!(metrics.total_errors(), 1);
        assert_eq!(metrics.errors_by_category.get("configuration"), Some(&1));
        assert_eq!(metrics.errors_by_severity.get(&ErrorSeverity::Error), Some(&1));
        assert_eq!(metrics.non_retryable_errors, 1);
        assert_eq!(metrics.retryable_errors, 0);

        let network_error = EmbeddingError::NetworkError(
            reqwest::Error::from(reqwest::ErrorKind::Request)
        );
        metrics.record_error(&network_error);

        assert_eq!(metrics.total_errors(), 2);
        assert_eq!(metrics.retryable_errors, 1);
        assert_eq!(metrics.retryable_error_rate(), 0.5);
    }

    #[test]
    fn test_api_error_retryable() {
        // 5xx errors should be retryable
        let server_error = EmbeddingError::ApiError {
            message: "Internal server error".to_string(),
            status: 500,
        };
        assert!(server_error.is_retryable());

        // 429 should be retryable
        let rate_limit = EmbeddingError::ApiError {
            message: "Rate limited".to_string(),
            status: 429,
        };
        assert!(rate_limit.is_retryable());

        // 4xx errors (except 429) should not be retryable
        let client_error = EmbeddingError::ApiError {
            message: "Bad request".to_string(),
            status: 400,
        };
        assert!(!client_error.is_retryable());
    }
}