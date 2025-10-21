//! Error handling for the Crucible Rune system
//!
//! This module provides comprehensive error types and handling for all Rune operations.
//! It uses thiserror for clean error definitions and provides proper error propagation
//! with context and chaining throughout the crate.

use thiserror::Error;
use std::sync::Arc;

/// Main error type for the Rune system
#[derive(Error, Debug, Clone)]
pub enum RuneError {
    /// Compilation errors
    #[error("Compilation failed: {message}")]
    CompilationError {
        message: String,
        #[source]
        source: Option<rune::compile::Error>,
    },

    /// Runtime errors
    #[error("Runtime error: {message}")]
    RuntimeError {
        message: String,
        #[source]
        source: Option<rune::runtime::VmError>,
    },

    /// Tool discovery errors
    #[error("Tool discovery failed: {message}")]
    DiscoveryError {
        message: String,
        path: Option<std::path::PathBuf>,
    },

    /// Tool loading errors
    #[error("Tool loading failed: {tool_name}")]
    LoadingError {
        tool_name: String,
        #[source]
        source: anyhow::Error,
    },

    /// Tool execution errors
    #[error("Tool execution failed: {tool_name}")]
    ExecutionError {
        tool_name: String,
        execution_id: Option<String>,
        #[source]
        source: anyhow::Error,
    },

    /// Validation errors
    #[error("Validation failed: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
        value: Option<serde_json::Value>,
    },

    /// Schema errors
    #[error("Schema error: {message}")]
    SchemaError {
        message: String,
        schema_path: Option<String>,
    },

    /// Hot reload errors
    #[error("Hot reload failed: {message}")]
    HotReloadError {
        message: String,
        file_path: Option<std::path::PathBuf>,
    },

    /// Context errors
    #[error("Context error: {message}")]
    ContextError {
        message: String,
        context_type: Option<String>,
    },

    /// Registry errors
    #[error("Registry error: {message}")]
    RegistryError {
        message: String,
        operation: Option<String>,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
        field: Option<String>,
        value: Option<String>,
    },

    /// Security errors
    #[error("Security error: {message}")]
    SecurityError {
        message: String,
        violation_type: Option<String>,
    },

    /// Timeout errors
    #[error("Operation timed out: {message}")]
    TimeoutError {
        message: String,
        timeout_ms: u64,
        elapsed_ms: u64,
    },

    /// Resource errors
    #[error("Resource error: {message}")]
    ResourceError {
        message: String,
        resource_type: Option<String>,
    },

    /// I/O errors
    #[error("I/O error: {message}")]
    IoError {
        message: String,
        path: Option<std::path::PathBuf>,
        #[source]
        source: std::io::Error,
    },

    /// Serialization errors
    #[error("Serialization error: {message}")]
    SerializationError {
        message: String,
        format: Option<String>,
        #[source]
        source: serde_json::Error,
    },

    /// Database errors
    #[error("Database error: {message}")]
    DatabaseError {
        message: String,
        operation: Option<String>,
        #[source]
        source: anyhow::Error,
    },

    /// Embedding errors
    #[error("Embedding error: {message}")]
    EmbeddingError {
        message: String,
        provider: Option<String>,
        #[source]
        source: anyhow::Error,
    },

    /// Service errors
    #[error("Service error: {message}")]
    ServiceError {
        message: String,
        service_name: Option<String>,
        operation: Option<String>,
    },

    /// Generic errors
    #[error("Error: {message}")]
    GenericError {
        message: String,
        #[source]
        source: Option<anyhow::Error>,
    },

    /// Network errors
    #[error("Network error: {message}")]
    NetworkError {
        message: String,
        url: Option<String>,
        status_code: Option<u16>,
        #[source]
        source: Option<reqwest::Error>,
    },

    /// Authentication errors
    #[error("Authentication error: {message}")]
    AuthenticationError {
        message: String,
        provider: Option<String>,
        reason: Option<String>,
    },

    /// Rate limiting errors
    #[error("Rate limit exceeded: {message}")]
    RateLimitError {
        message: String,
        provider: Option<String>,
        retry_after_ms: Option<u64>,
    },

    /// Quota exceeded errors
    #[error("Quota exceeded: {message}")]
    QuotaError {
        message: String,
        quota_type: Option<String>,
        current_usage: Option<u64>,
        limit: Option<u64>,
    },

    /// Provider-specific errors
    #[error("Provider error: {provider} - {message}")]
    ProviderError {
        provider: String,
        message: String,
        error_code: Option<String>,
        #[source]
        source: Option<anyhow::Error>,
    },

    /// Parsing errors
    #[error("Parsing error: {message}")]
    ParseError {
        message: String,
        input_type: Option<String>,
        #[source]
        source: Option<anyhow::Error>,
    },

    /// Concurrent operation errors
    #[error("Concurrent operation error: {message}")]
    ConcurrencyError {
        message: String,
        operation: Option<String>,
        resource_id: Option<String>,
    },
}

impl RuneError {
    /// Get error category for logging and metrics
    pub fn category(&self) -> &'static str {
        match self {
            Self::CompilationError { .. } => "compilation",
            Self::RuntimeError { .. } => "runtime",
            Self::DiscoveryError { .. } => "discovery",
            Self::LoadingError { .. } => "loading",
            Self::ExecutionError { .. } => "execution",
            Self::ValidationError { .. } => "validation",
            Self::SchemaError { .. } => "schema",
            Self::HotReloadError { .. } => "hot_reload",
            Self::ContextError { .. } => "context",
            Self::RegistryError { .. } => "registry",
            Self::ConfigurationError { .. } => "configuration",
            Self::SecurityError { .. } => "security",
            Self::TimeoutError { .. } => "timeout",
            Self::ResourceError { .. } => "resource",
            Self::IoError { .. } => "io",
            Self::SerializationError { .. } => "serialization",
            Self::DatabaseError { .. } => "database",
            Self::EmbeddingError { .. } => "embedding",
            Self::ServiceError { .. } => "service",
            Self::GenericError { .. } => "generic",
            Self::NetworkError { .. } => "network",
            Self::AuthenticationError { .. } => "authentication",
            Self::RateLimitError { .. } => "rate_limit",
            Self::QuotaError { .. } => "quota",
            Self::ProviderError { .. } => "provider",
            Self::ParseError { .. } => "parse",
            Self::ConcurrencyError { .. } => "concurrency",
        }
    }

    /// Get error severity for alerting
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::CompilationError { .. } => ErrorSeverity::Error,
            Self::RuntimeError { .. } => ErrorSeverity::Error,
            Self::SecurityError { .. } => ErrorSeverity::Critical,
            Self::ValidationError { .. } => ErrorSeverity::Warning,
            Self::TimeoutError { .. } => ErrorSeverity::Warning,
            Self::ConfigurationError { .. } => ErrorSeverity::Error,
            Self::DatabaseError { .. } => ErrorSeverity::Error,
            Self::EmbeddingError { .. } => ErrorSeverity::Warning,
            Self::ServiceError { .. } => ErrorSeverity::Error,
            Self::AuthenticationError { .. } => ErrorSeverity::Error,
            Self::RateLimitError { .. } => ErrorSeverity::Warning,
            Self::QuotaError { .. } => ErrorSeverity::Error,
            Self::NetworkError { .. } => ErrorSeverity::Warning,
            Self::ConcurrencyError { .. } => ErrorSeverity::Error,
            Self::ProviderError { .. } => ErrorSeverity::Warning,
            Self::ParseError { .. } => ErrorSeverity::Error,
            _ => ErrorSeverity::Info,
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::TimeoutError { .. } => true,
            Self::ResourceError { .. } => true,
            Self::DatabaseError { .. } => true,
            Self::EmbeddingError { .. } => true,
            Self::ServiceError { .. } => true,
            Self::NetworkError { .. } => true,
            Self::RateLimitError { .. } => true,
            Self::ProviderError { .. } => true,
            Self::ConcurrencyError { .. } => true,
            _ => false,
        }
    }

    /// Get suggested retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> Option<u64> {
        if !self.is_retryable() {
            return None;
        }

        match self {
            Self::TimeoutError { .. } => Some(1000),
            Self::ResourceError { .. } => Some(5000),
            Self::DatabaseError { .. } => Some(2000),
            Self::EmbeddingError { .. } => Some(3000),
            Self::ServiceError { .. } => Some(1000),
            Self::NetworkError { .. } => Some(2000),
            Self::RateLimitError { retry_after_ms, .. } => *retry_after_ms,
            Self::ProviderError { .. } => Some(3000),
            Self::ConcurrencyError { .. } => Some(500),
            _ => Some(1000),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorSeverity {
    /// Debug information
    Debug = 0,
    /// Informational messages
    Info = 1,
    /// Warning messages
    Warning = 2,
    /// Error messages
    Error = 3,
    /// Critical errors
    Critical = 4,
}

/// Error context for additional information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Error identifier
    pub error_id: String,
    /// Timestamp when error occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Operation being performed
    pub operation: Option<String>,
    /// Tool name (if applicable)
    pub tool_name: Option<String>,
    /// File path (if applicable)
    pub file_path: Option<std::path::PathBuf>,
    /// Additional context
    pub additional: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new() -> Self {
        Self {
            error_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            operation: None,
            tool_name: None,
            file_path: None,
            additional: std::collections::HashMap::new(),
        }
    }

    /// Add operation to context
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    /// Add tool name to context
    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    /// Add file path to context
    pub fn with_file_path(mut self, file_path: impl Into<std::path::PathBuf>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    /// Add additional context
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.additional.insert(key.into(), value.into());
        self
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced error with context
#[derive(Error, Debug)]
#[error("{error}")]
pub struct ContextualError {
    /// The underlying error
    pub error: RuneError,
    /// Error context
    pub context: ErrorContext,
}

impl ContextualError {
    /// Create a new contextual error
    pub fn new(error: RuneError, context: ErrorContext) -> Self {
        Self { error, context }
    }

    /// Create a contextual error from any error
    pub fn from_anyhow(error: anyhow::Error, context: ErrorContext) -> Self {
        let rune_error = if let Some(rune_err) = error.downcast_ref::<RuneError>() {
            rune_err.clone()
        } else {
            RuneError::GenericError {
                message: error.to_string(),
                source: Some(error),
            }
        };
        Self::new(rune_error, context)
    }

    /// Get the error category
    pub fn category(&self) -> &'static str {
        self.error.category()
    }

    /// Get the error severity
    pub fn severity(&self) -> ErrorSeverity {
        self.error.severity()
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        self.error.is_retryable()
    }

    /// Get suggested retry delay
    pub fn retry_delay_ms(&self) -> Option<u64> {
        self.error.retry_delay_ms()
    }
}

/// Error recovery strategy
#[derive(Debug, Clone)]
pub enum ErrorRecoveryStrategy {
    /// No recovery attempt
    None,
    /// Retry the operation
    Retry { max_attempts: u32, delay_ms: u64 },
    /// Use fallback value
    Fallback(serde_json::Value),
    /// Skip the operation
    Skip,
    /// Abort the entire process
    Abort,
}

/// Error recovery result
#[derive(Debug, Clone)]
pub struct ErrorRecoveryResult {
    /// Whether recovery was successful
    pub success: bool,
    /// Recovery strategy used
    pub strategy: ErrorRecoveryStrategy,
    /// Number of attempts made
    pub attempts: u32,
    /// Total time spent on recovery
    pub total_time_ms: u64,
    /// Result value (if successful)
    pub result: Option<serde_json::Value>,
    /// Recovery error (if failed)
    pub error: Option<String>,
}

/// Error metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ErrorMetrics {
    /// Total errors by category
    pub errors_by_category: std::collections::HashMap<String, u64>,
    /// Total errors by severity
    pub errors_by_severity: std::collections::HashMap<ErrorSeverity, u64>,
    /// Total retryable errors
    pub retryable_errors: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Failed recoveries
    pub failed_recoveries: u64,
    /// Average recovery time
    pub avg_recovery_time_ms: f64,
    /// Last error timestamp
    pub last_error: Option<chrono::DateTime<chrono::Utc>>,
}

impl ErrorMetrics {
    /// Record an error
    pub fn record_error(&mut self, error: &ContextualError) {
        let category = error.category().to_string();
        let severity = error.severity();

        *self.errors_by_category.entry(category).or_insert(0) += 1;
        *self.errors_by_severity.entry(severity).or_insert(0) += 1;

        if error.is_retryable() {
            self.retryable_errors += 1;
        }

        self.last_error = Some(chrono::Utc::now());
    }

    /// Record a recovery attempt
    pub fn record_recovery(&mut self, result: &ErrorRecoveryResult) {
        if result.success {
            self.successful_recoveries += 1;
        } else {
            self.failed_recoveries += 1;
        }

        // Update average recovery time
        let total_recoveries = self.successful_recoveries + self.failed_recoveries;
        if total_recoveries > 0 {
            self.avg_recovery_time_ms = (self.avg_recovery_time_ms * (total_recoveries - 1) as f64
                + result.total_time_ms as f64) / total_recoveries as f64;
        }
    }

    /// Get error rate (errors per minute)
    pub fn error_rate_per_minute(&self) -> f64 {
        if let Some(last_error) = self.last_error {
            let now = chrono::Utc::now();
            let minutes_elapsed = (now - last_error).num_minutes().max(1) as f64;
            let total_errors: u64 = self.errors_by_category.values().sum();
            total_errors as f64 / minutes_elapsed
        } else {
            0.0
        }
    }

    /// Get recovery success rate
    pub fn recovery_success_rate(&self) -> f64 {
        let total_recoveries = self.successful_recoveries + self.failed_recoveries;
        if total_recoveries > 0 {
            self.successful_recoveries as f64 / total_recoveries as f64
        } else {
            1.0
        }
    }
}

/// Result type for Rune operations
pub type RuneResult<T> = Result<T, RuneError>;

/// Result type for Rune operations with context
pub type ContextualResult<T> = Result<T, ContextualError>;

/// Convert from std::io::Error
impl From<std::io::Error> for RuneError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
            path: None,
            source: err,
        }
    }
}

/// Convert from reqwest::Error
impl From<reqwest::Error> for RuneError {
    fn from(err: reqwest::Error) -> Self {
        let status_code = err.status().map(|s| s.as_u16());
        let url = err.url().map(|u| u.to_string());

        Self::NetworkError {
            message: err.to_string(),
            url,
            status_code,
            source: Some(err),
        }
    }
}

/// Convert from tokio::sync::TryLockError
impl From<tokio::sync::TryLockError> for RuneError {
    fn from(err: tokio::sync::TryLockError) -> Self {
        Self::ConcurrencyError {
            message: format!("Lock error: {}", err),
            operation: None,
            resource_id: None,
        }
    }
}

/// Convert from uuid::Error
impl From<uuid::Error> for RuneError {
    fn from(err: uuid::Error) -> Self {
        Self::ParseError {
            message: format!("UUID parsing error: {}", err),
            input_type: Some("uuid".to_string()),
            source: Some(anyhow::anyhow!(err)),
        }
    }
}

/// Convert from regex::Error
impl From<regex::Error> for RuneError {
    fn from(err: regex::Error) -> Self {
        Self::ParseError {
            message: format!("Regex parsing error: {}", err),
            input_type: Some("regex".to_string()),
            source: Some(anyhow::anyhow!(err)),
        }
    }
}

/// Convert from chrono::ParseError
impl From<chrono::ParseError> for RuneError {
    fn from(err: chrono::ParseError) -> Self {
        Self::ParseError {
            message: format!("Date/time parsing error: {}", err),
            input_type: Some("datetime".to_string()),
            source: Some(anyhow::anyhow!(err)),
        }
    }
}

/// Convert from serde_json::Error
impl From<serde_json::Error> for RuneError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            message: err.to_string(),
            format: Some("json".to_string()),
            source: err,
        }
    }
}

/// Convert from anyhow::Error
impl From<anyhow::Error> for RuneError {
    fn from(err: anyhow::Error) -> Self {
        if let Some(rune_err) = err.downcast_ref::<RuneError>() {
            rune_err.clone()
        } else if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            Self::from(*io_err)
        } else if let Some(json_err) = err.downcast_ref::<serde_json::Error>() {
            Self::from(*json_err)
        } else {
            Self::GenericError {
                message: err.to_string(),
                source: Some(err),
            }
        }
    }
}

/// Convert from rune::compile::Error
impl From<rune::compile::Error> for RuneError {
    fn from(err: rune::compile::Error) -> Self {
        Self::CompilationError {
            message: err.to_string(),
            source: Some(err),
        }
    }
}

/// Convert from rune::runtime::VmError
impl From<rune::runtime::VmError> for RuneError {
    fn from(err: rune::runtime::VmError) -> Self {
        Self::RuntimeError {
            message: err.to_string(),
            source: Some(err),
        }
    }
}

/// Convenience trait for error context
pub trait ErrorExt<T> {
    /// Add context to a Result
    fn with_context(self, context: ErrorContext) -> ContextualResult<T>;

    /// Add context to a Result with closure
    fn with_context_fn<F>(self, f: F) -> ContextualResult<T>
    where
        F: FnOnce() -> ErrorContext;

    /// Add operation context
    fn with_operation(self, operation: impl Into<String>) -> ContextualResult<T>;

    /// Add tool context
    fn with_tool(self, tool_name: impl Into<String>) -> ContextualResult<T>;

    /// Add file context
    fn with_file(self, file_path: impl Into<std::path::PathBuf>) -> ContextualResult<T>;
}

impl<T> ErrorExt<T> for RuneResult<T> {
    fn with_context(self, context: ErrorContext) -> ContextualResult<T> {
        self.map_err(|error| ContextualError::new(error, context))
    }

    fn with_context_fn<F>(self, f: F) -> ContextualResult<T>
    where
        F: FnOnce() -> ErrorContext,
    {
        self.map_err(|error| ContextualError::new(error, f()))
    }

    fn with_operation(self, operation: impl Into<String>) -> ContextualResult<T> {
        self.with_context(ErrorContext::new().with_operation(operation))
    }

    fn with_tool(self, tool_name: impl Into<String>) -> ContextualResult<T> {
        self.with_context(ErrorContext::new().with_tool_name(tool_name))
    }

    fn with_file(self, file_path: impl Into<std::path::PathBuf>) -> ContextualResult<T> {
        self.with_context(ErrorContext::new().with_file_path(file_path))
    }
}

/// Convenience functions for creating common errors
impl RuneError {
    /// Create a compilation error
    pub fn compilation(message: impl Into<String>) -> Self {
        Self::CompilationError {
            message: message.into(),
            source: None,
        }
    }

    /// Create a compilation error with source
    pub fn compilation_with_source(message: impl Into<String>, source: rune::compile::Error) -> Self {
        Self::CompilationError {
            message: message.into(),
            source: Some(source),
        }
    }

    /// Create a runtime error
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::RuntimeError {
            message: message.into(),
            source: None,
        }
    }

    /// Create a runtime error with source
    pub fn runtime_with_source(message: impl Into<String>, source: rune::runtime::VmError) -> Self {
        Self::RuntimeError {
            message: message.into(),
            source: Some(source),
        }
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::ValidationError {
            message: message.into(),
            field: None,
            value: None,
        }
    }

    /// Create a validation error with field
    pub fn validation_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::ValidationError {
            message: message.into(),
            field: Some(field.into()),
            value: None,
        }
    }

    /// Create a validation error with field and value
    pub fn validation_field_value(
        message: impl Into<String>,
        field: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self::ValidationError {
            message: message.into(),
            field: Some(field.into()),
            value: Some(value),
        }
    }

    /// Create a discovery error
    pub fn discovery(message: impl Into<String>) -> Self {
        Self::DiscoveryError {
            message: message.into(),
            path: None,
        }
    }

    /// Create a discovery error with path
    pub fn discovery_with_path(message: impl Into<String>, path: impl Into<std::path::PathBuf>) -> Self {
        Self::DiscoveryError {
            message: message.into(),
            path: Some(path.into()),
        }
    }

    /// Create a network error
    pub fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
            url: None,
            status_code: None,
            source: None,
        }
    }

    /// Create a network error with URL
    pub fn network_with_url(message: impl Into<String>, url: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
            url: Some(url.into()),
            status_code: None,
            source: None,
        }
    }

    /// Create a provider error
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ProviderError {
            provider: provider.into(),
            message: message.into(),
            error_code: None,
            source: None,
        }
    }

    /// Create a provider error with code
    pub fn provider_with_code(
        provider: impl Into<String>,
        message: impl Into<String>,
        error_code: impl Into<String>,
    ) -> Self {
        Self::ProviderError {
            provider: provider.into(),
            message: message.into(),
            error_code: Some(error_code.into()),
            source: None,
        }
    }

    /// Create a timeout error
    pub fn timeout(message: impl Into<String>, timeout_ms: u64, elapsed_ms: u64) -> Self {
        Self::TimeoutError {
            message: message.into(),
            timeout_ms,
            elapsed_ms,
        }
    }

    /// Create a configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
            field: None,
            value: None,
        }
    }

    /// Create a configuration error with field
    pub fn configuration_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
            field: Some(field.into()),
            value: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categories() {
        let error = RuneError::CompilationError {
            message: "Test error".to_string(),
            source: None,
        };
        assert_eq!(error.category(), "compilation");
        assert_eq!(error.severity(), ErrorSeverity::Error);
        assert!(!error.is_retryable());
    }

    #[test]
    fn test_error_context() {
        let context = ErrorContext::new()
            .with_operation("test_operation")
            .with_tool_name("test_tool")
            .with_context("key", "value");

        assert_eq!(context.operation, Some("test_operation".to_string()));
        assert_eq!(context.tool_name, Some("test_tool".to_string()));
        assert_eq!(context.additional.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_contextual_error() {
        let rune_error = RuneError::ValidationError {
            message: "Test validation error".to_string(),
            field: Some("test_field".to_string()),
            value: None,
        };

        let context = ErrorContext::new().with_operation("test");
        let contextual_error = ContextualError::new(rune_error.clone(), context);

        assert_eq!(contextual_error.category(), "validation");
        assert_eq!(contextual_error.severity(), ErrorSeverity::Warning);
    }

    #[test]
    fn test_error_metrics() {
        let mut metrics = ErrorMetrics::default();

        let error = RuneError::ValidationError {
            message: "Test".to_string(),
            field: None,
            value: None,
        };
        let context = ErrorContext::new();
        let contextual_error = ContextualError::new(error, context);

        metrics.record_error(&contextual_error);
        assert_eq!(metrics.errors_by_category.get("validation"), Some(&1));
        assert_eq!(metrics.errors_by_severity.get(&ErrorSeverity::Warning), Some(&1));
    }

    #[test]
    fn test_retryable_errors() {
        let timeout_error = RuneError::TimeoutError {
            message: "Test timeout".to_string(),
            timeout_ms: 1000,
            elapsed_ms: 2000,
        };
        assert!(timeout_error.is_retryable());
        assert_eq!(timeout_error.retry_delay_ms(), Some(1000));

        let compilation_error = RuneError::CompilationError {
            message: "Test compilation error".to_string(),
            source: None,
        };
        assert!(!compilation_error.is_retryable());
        assert_eq!(compilation_error.retry_delay_ms(), None);
    }

    #[test]
    fn test_error_conversions() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let rune_error: RuneError = io_error.into();
        assert!(matches!(rune_error, RuneError::IoError { .. }));

        let json_error = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let rune_error: RuneError = json_error.into();
        assert!(matches!(rune_error, RuneError::SerializationError { .. }));
    }
}