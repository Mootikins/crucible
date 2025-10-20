//! Error handling for the Crucible Rune system
//!
//! This module provides comprehensive error types and handling for all Rune operations.

use thiserror::Error;

/// Main error type for the Rune system
#[derive(Error, Debug)]
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
            _ => Some(1000),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
            Self::from(io_err.clone())
        } else if let Some(json_err) = err.downcast_ref::<serde_json::Error>() {
            Self::from(json_err.clone())
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