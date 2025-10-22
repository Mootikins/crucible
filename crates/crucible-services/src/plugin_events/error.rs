//! Error types for plugin event subscription system

use crate::events::EventError;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use thiserror::Error;

/// Plugin event subscription error types
#[derive(Error, Debug)]
pub enum SubscriptionError {
    /// Invalid subscription configuration
    #[error("Invalid subscription configuration: {0}")]
    InvalidConfiguration(String),

    /// Subscription not found
    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(String),

    /// Plugin not found
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    /// Authorization error
    #[error("Authorization error: {0}")]
    Authorization(String),

    /// Event delivery error
    #[error("Event delivery error: {0}")]
    DeliveryError(String),

    /// Event filtering error
    #[error("Event filtering error: {0}")]
    FilteringError(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Event system error
    #[error("Event system error: {0}")]
    EventError(#[from] EventError),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Timeout error
    #[error("Timeout error: {0}")]
    TimeoutError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Resource exhausted
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    InternalError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Security error
    #[error("Security error: {0}")]
    SecurityError(String),

    /// Plugin error
    #[error("Plugin error: {0}")]
    PluginError(String),

    /// Backpressure error
    #[error("Backpressure error: {0}")]
    BackpressureError(String),

    /// Retry exhausted
    #[error("Retry exhausted: {0}")]
    RetryExhausted(String),
}

/// Result type for subscription operations
pub type SubscriptionResult<T> = Result<T, SubscriptionError>;

/// Detailed error context
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Error code
    pub code: String,

    /// Error message
    pub message: String,

    /// Error category
    pub category: ErrorCategory,

    /// Error severity
    pub severity: ErrorSeverity,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Additional context
    pub context: std::collections::HashMap<String, String>,

    /// Cause chain
    pub cause: Option<Arc<ErrorContext>>,

    /// Suggested resolution
    pub resolution: Option<String>,

    /// Retry information
    pub retry_info: Option<RetryInfo>,
}

impl ErrorContext {
    /// Create new error context
    pub fn new(
        code: String,
        message: String,
        category: ErrorCategory,
        severity: ErrorSeverity,
    ) -> Self {
        Self {
            code,
            message,
            category,
            severity,
            timestamp: Utc::now(),
            context: std::collections::HashMap::new(),
            cause: None,
            resolution: None,
            retry_info: None,
        }
    }

    /// Add context field
    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.context.insert(key, value);
        self
    }

    /// Add cause
    pub fn with_cause(mut self, cause: Arc<ErrorContext>) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Add resolution
    pub fn with_resolution(mut self, resolution: String) -> Self {
        self.resolution = Some(resolution);
        self
    }

    /// Add retry information
    pub fn with_retry_info(mut self, retry_info: RetryInfo) -> Self {
        self.retry_info = Some(retry_info);
        self
    }

    /// Get full error message with context
    pub fn full_message(&self) -> String {
        let mut msg = format!("[{}] {} ({}: {})",
            self.code,
            self.message,
            self.category,
            self.severity
        );

        // Add context fields
        if !self.context.is_empty() {
            let context_str = self.context
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            msg.push_str(&format!(" | {}", context_str));
        }

        // Add resolution if available
        if let Some(resolution) = &self.resolution {
            msg.push_str(&format!(" | Resolution: {}", resolution));
        }

        msg
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        self.retry_info.as_ref()
            .map_or(false, |info| info.is_retryable())
    }

    /// Get retry delay
    pub fn retry_delay(&self) -> Option<std::time::Duration> {
        self.retry_info.as_ref()
            .and_then(|info| info.next_retry_delay())
    }
}

/// Error category classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Configuration related errors
    Configuration,

    /// Authorization and security errors
    Security,

    /// Network and communication errors
    Network,

    /// Data processing errors
    Data,

    /// Resource management errors
    Resource,

    /// Plugin related errors
    Plugin,

    /// System errors
    System,

    /// User errors
    User,

    /// Unknown category
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Configuration => write!(f, "Configuration"),
            ErrorCategory::Security => write!(f, "Security"),
            ErrorCategory::Network => write!(f, "Network"),
            ErrorCategory::Data => write!(f, "Data"),
            ErrorCategory::Resource => write!(f, "Resource"),
            ErrorCategory::Plugin => write!(f, "Plugin"),
            ErrorCategory::System => write!(f, "System"),
            ErrorCategory::User => write!(f, "User"),
            ErrorCategory::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Error severity level
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Debug information
    Debug,

    /// Informational message
    Info,

    /// Warning condition
    Warning,

    /// Error condition
    Error,

    /// Critical error
    Critical,

    /// Fatal error
    Fatal,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Debug => write!(f, "Debug"),
            ErrorSeverity::Info => write!(f, "Info"),
            ErrorSeverity::Warning => write!(f, "Warning"),
            ErrorSeverity::Error => write!(f, "Error"),
            ErrorSeverity::Critical => write!(f, "Critical"),
            ErrorSeverity::Fatal => write!(f, "Fatal"),
        }
    }
}

/// Retry information for retryable errors
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// Current attempt number
    pub attempt: u32,

    /// Maximum retry attempts
    pub max_attempts: u32,

    /// Initial retry delay
    pub initial_delay: std::time::Duration,

    /// Maximum retry delay
    pub max_delay: std::time::Duration,

    /// Backoff multiplier
    pub backoff_multiplier: f64,

    /// Retry strategy
    pub strategy: RetryStrategy,

    /// Next retry timestamp
    pub next_retry_at: Option<DateTime<Utc>>,
}

impl RetryInfo {
    /// Create new retry info
    pub fn new(
        max_attempts: u32,
        initial_delay: std::time::Duration,
        strategy: RetryStrategy,
    ) -> Self {
        Self {
            attempt: 0,
            max_attempts,
            initial_delay,
            max_delay: std::time::Duration::from_secs(300), // 5 minutes max
            backoff_multiplier: 2.0,
            strategy,
            next_retry_at: None,
        }
    }

    /// Check if retry is possible
    pub fn is_retryable(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Calculate next retry delay
    pub fn next_retry_delay(&self) -> Option<std::time::Duration> {
        if !self.is_retryable() {
            return None;
        }

        let delay = match self.strategy {
            RetryStrategy::Fixed => self.initial_delay,
            RetryStrategy::Exponential => {
                let delay = self.initial_delay * (self.backoff_multiplier.powi(self.attempt as i32));
                std::cmp::min(delay, self.max_delay)
            }
            RetryStrategy::Linear => {
                let delay = self.initial_delay * (self.attempt + 1);
                std::cmp::min(delay, self.max_delay)
            }
        };

        Some(delay)
    }

    /// Increment attempt count and schedule next retry
    pub fn increment_attempt(&mut self) {
        self.attempt += 1;

        if let Some(delay) = self.next_retry_delay() {
            self.next_retry_at = Some(Utc::now() + chrono::Duration::from_std(delay).unwrap());
        }
    }

    /// Check if retry should be attempted now
    pub fn should_retry_now(&self) -> bool {
        if !self.is_retryable() {
            return false;
        }

        self.next_retry_at
            .map_or(true, |retry_time| retry_time <= Utc::now())
    }
}

/// Retry strategy
#[derive(Debug, Clone, PartialEq)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed,

    /// Exponential backoff
    Exponential,

    /// Linear backoff
    Linear,
}

/// Enhanced error with context
#[derive(Error, Debug)]
#[error("{context}")]
pub struct ContextualError {
    pub context: ErrorContext,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ContextualError {
    /// Create new contextual error
    pub fn new(context: ErrorContext) -> Self {
        Self {
            context,
            source: None,
        }
    }

    /// Create contextual error from existing error
    pub fn from_error<E: std::error::Error + Send + Sync + 'static>(
        error: E,
        context: ErrorContext,
    ) -> Self {
        Self {
            context,
            source: Some(Box::new(error)),
        }
    }

    /// Create subscription error with context
    pub fn subscription_error(
        error: SubscriptionError,
        code: String,
        message: String,
        category: ErrorCategory,
        severity: ErrorSeverity,
    ) -> Self {
        let context = ErrorContext::new(code, message, category, severity);
        Self::from_error(error, context)
    }
}

/// Error metrics for monitoring
#[derive(Debug, Clone)]
pub struct ErrorMetrics {
    /// Total errors by category
    pub errors_by_category: std::collections::HashMap<ErrorCategory, u64>,

    /// Total errors by severity
    pub errors_by_severity: std::collections::HashMap<ErrorSeverity, u64>,

    /// Error rate (errors per second)
    pub error_rate: f64,

    /// Most recent errors
    pub recent_errors: Vec<ErrorContext>,

    /// Error trend (increasing/decreasing/stable)
    pub trend: ErrorTrend,

    /// Top error codes
    pub top_error_codes: std::collections::HashMap<String, u64>,
}

impl Default for ErrorMetrics {
    fn default() -> Self {
        Self {
            errors_by_category: std::collections::HashMap::new(),
            errors_by_severity: std::collections::HashMap::new(),
            error_rate: 0.0,
            recent_errors: Vec::new(),
            trend: ErrorTrend::Stable,
            top_error_codes: std::collections::HashMap::new(),
        }
    }
}

/// Error trend
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorTrend {
    Increasing,
    Decreasing,
    Stable,
}

/// Convert subscription errors to error context
impl From<SubscriptionError> for ErrorContext {
    fn from(error: SubscriptionError) -> Self {
        let (code, message, category, severity) = match &error {
            SubscriptionError::InvalidConfiguration(msg) => {
                ("SUB_INVALID_CONFIG".to_string(), msg.clone(), ErrorCategory::Configuration, ErrorSeverity::Error)
            }
            SubscriptionError::SubscriptionNotFound(id) => {
                ("SUB_NOT_FOUND".to_string(), format!("Subscription {} not found", id), ErrorCategory::User, ErrorSeverity::Error)
            }
            SubscriptionError::PluginNotFound(id) => {
                ("PLUGIN_NOT_FOUND".to_string(), format!("Plugin {} not found", id), ErrorCategory::Plugin, ErrorSeverity::Error)
            }
            SubscriptionError::Authorization(msg) => {
                ("AUTH_ERROR".to_string(), msg.clone(), ErrorCategory::Security, ErrorSeverity::Warning)
            }
            SubscriptionError::DeliveryError(msg) => {
                ("DELIVERY_ERROR".to_string(), msg.clone(), ErrorCategory::Network, ErrorSeverity::Error)
            }
            SubscriptionError::FilteringError(msg) => {
                ("FILTER_ERROR".to_string(), msg.clone(), ErrorCategory::Data, ErrorSeverity::Error)
            }
            SubscriptionError::SerializationError(_) => {
                ("SERIALIZATION_ERROR".to_string(), "Failed to serialize/deserialize data".to_string(), ErrorCategory::Data, ErrorSeverity::Error)
            }
            SubscriptionError::IoError(_) => {
                ("IO_ERROR".to_string(), "Input/output error occurred".to_string(), ErrorCategory::System, ErrorSeverity::Error)
            }
            SubscriptionError::EventError(_) => {
                ("EVENT_ERROR".to_string(), "Event system error".to_string(), ErrorCategory::System, ErrorSeverity::Error)
            }
            SubscriptionError::DatabaseError(msg) => {
                ("DB_ERROR".to_string(), msg.clone(), ErrorCategory::Data, ErrorSeverity::Error)
            }
            SubscriptionError::NetworkError(msg) => {
                ("NETWORK_ERROR".to_string(), msg.clone(), ErrorCategory::Network, ErrorSeverity::Error)
            }
            SubscriptionError::TimeoutError(msg) => {
                ("TIMEOUT_ERROR".to_string(), msg.clone(), ErrorCategory::Network, ErrorSeverity::Warning)
            }
            SubscriptionError::RateLimitExceeded(msg) => {
                ("RATE_LIMIT".to_string(), msg.clone(), ErrorCategory::Resource, ErrorSeverity::Warning)
            }
            SubscriptionError::ResourceExhausted(msg) => {
                ("RESOURCE_EXHAUSTED".to_string(), msg.clone(), ErrorCategory::Resource, ErrorSeverity::Error)
            }
            SubscriptionError::InternalError(msg) => {
                ("INTERNAL_ERROR".to_string(), msg.clone(), ErrorCategory::System, ErrorSeverity::Critical)
            }
            SubscriptionError::ValidationError(msg) => {
                ("VALIDATION_ERROR".to_string(), msg.clone(), ErrorCategory::User, ErrorSeverity::Warning)
            }
            SubscriptionError::ConfigurationError(msg) => {
                ("CONFIG_ERROR".to_string(), msg.clone(), ErrorCategory::Configuration, ErrorSeverity::Error)
            }
            SubscriptionError::SecurityError(msg) => {
                ("SECURITY_ERROR".to_string(), msg.clone(), ErrorCategory::Security, ErrorSeverity::Critical)
            }
            SubscriptionError::PluginError(msg) => {
                ("PLUGIN_ERROR".to_string(), msg.clone(), ErrorCategory::Plugin, ErrorSeverity::Error)
            }
            SubscriptionError::BackpressureError(msg) => {
                ("BACKPRESSURE_ERROR".to_string(), msg.clone(), ErrorCategory::Resource, ErrorSeverity::Warning)
            }
            SubscriptionError::RetryExhausted(msg) => {
                ("RETRY_EXHAUSTED".to_string(), msg.clone(), ErrorCategory::Network, ErrorSeverity::Error)
            }
        };

        ErrorContext::new(code, message, category, severity)
    }
}