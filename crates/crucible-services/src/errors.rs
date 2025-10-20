use std::fmt;
use thiserror::Error;

/// Service-level error types for the Crucible service abstraction layer
#[derive(Error, Debug)]
pub enum ServiceError {
    /// Tool service related errors
    #[error("Tool service error: {message}")]
    ToolError { message: String },

    /// Database service related errors
    #[error("Database service error: {message}")]
    DatabaseError { message: String },

    /// LLM service related errors
    #[error("LLM service error: {message}")]
    LLMError { message: String },

    /// Configuration service related errors
    #[error("Configuration service error: {message}")]
    ConfigError { message: String },

    /// Routing related errors
    #[error("Routing error: {message}")]
    RoutingError { message: String },

    /// Service not available
    #[error("Service '{service_name}' is not available")]
    ServiceUnavailable { service_name: String },

    /// Invalid request format
    #[error("Invalid request format: {message}")]
    InvalidRequest { message: String },

    /// Timeout error
    #[error("Service operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Authentication/authorization error
    #[error("Access denied: {message}")]
    AccessDenied { message: String },

    /// Rate limiting error
    #[error("Rate limit exceeded: {message}")]
    RateLimitExceeded { message: String },

    /// Internal service error
    #[error("Internal service error: {message}")]
    InternalError { message: String },

    /// Service configuration error
    #[error("Service configuration error: {message}")]
    ConfigurationError { message: String },

    /// Dependency injection error
    #[error("Dependency injection error: {message}")]
    DependencyError { message: String },

    /// Serialization/deserialization error
    #[error("Serialization error: {message}")]
    SerializationError { message: String },
}

impl ServiceError {
    /// Create a new tool service error
    pub fn tool_error(message: impl Into<String>) -> Self {
        Self::ToolError {
            message: message.into(),
        }
    }

    /// Create a new database service error
    pub fn database_error(message: impl Into<String>) -> Self {
        Self::DatabaseError {
            message: message.into(),
        }
    }

    /// Create a new LLM service error
    pub fn llm_error(message: impl Into<String>) -> Self {
        Self::LLMError {
            message: message.into(),
        }
    }

    /// Create a new configuration service error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }

    /// Create a new routing error
    pub fn routing_error(message: impl Into<String>) -> Self {
        Self::RoutingError {
            message: message.into(),
        }
    }

    /// Create a new service unavailable error
    pub fn service_unavailable(service_name: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service_name: service_name.into(),
        }
    }

    /// Create a new invalid request error
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
        }
    }

    /// Create a new timeout error
    pub fn timeout(timeout_ms: u64) -> Self {
        Self::Timeout { timeout_ms }
    }

    /// Create a new access denied error
    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::AccessDenied {
            message: message.into(),
        }
    }

    /// Create a new rate limit exceeded error
    pub fn rate_limit_exceeded(message: impl Into<String>) -> Self {
        Self::RateLimitExceeded {
            message: message.into(),
        }
    }

    /// Create a new internal error
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::InternalError {
            message: message.into(),
        }
    }

    /// Create a new configuration error
    pub fn configuration_error(message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
        }
    }

    /// Create a new dependency error
    pub fn dependency_error(message: impl Into<String>) -> Self {
        Self::DependencyError {
            message: message.into(),
        }
    }

    /// Create a new serialization error
    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        match self {
            ServiceError::Timeout { .. }
            | ServiceError::ServiceUnavailable { .. }
            | ServiceError::RateLimitExceeded { .. }
            | ServiceError::InternalError { .. } => true,
            _ => false,
        }
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        match self {
            ServiceError::InvalidRequest { .. }
            | ServiceError::AccessDenied { .. }
            | ServiceError::ConfigurationError { .. } => true,
            _ => false,
        }
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        match self {
            ServiceError::ToolError { .. }
            | ServiceError::DatabaseError { .. }
            | ServiceError::LLMError { .. }
            | ServiceError::ConfigError { .. }
            | ServiceError::RoutingError { .. }
            | ServiceError::ServiceUnavailable { .. }
            | ServiceError::Timeout { .. }
            | ServiceError::InternalError { .. }
            | ServiceError::DependencyError { .. }
            | ServiceError::SerializationError { .. } => true,
            ServiceError::RateLimitExceeded { .. } => false, // Could be either
            ServiceError::InvalidRequest { .. } => false,    // Client error
            ServiceError::AccessDenied { .. } => false,      // Client error
            ServiceError::ConfigurationError { .. } => false, // Client error
        }
    }
}

/// Result type for service operations
pub type ServiceResult<T> = Result<T, ServiceError>;

/// Convert from common error types to ServiceError
impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        ServiceError::serialization_error(format!("JSON serialization error: {}", err))
    }
}

impl From<tokio::time::error::Elapsed> for ServiceError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        ServiceError::timeout(0) // The actual timeout should be set by the caller
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(err: anyhow::Error) -> Self {
        ServiceError::internal_error(format!("Anyhow error: {}", err))
    }
}

/// Error context for service operations
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Operation being performed
    pub operation: String,
    /// Service name
    pub service: String,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Additional context
    pub context: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(operation: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            service: service.into(),
            request_id: None,
            context: std::collections::HashMap::new(),
        }
    }

    /// Add a context field
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Set the request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

/// Enhanced error with context
#[derive(Error, Debug)]
#[error("{error}")]
pub struct ContextualError {
    #[source]
    pub error: ServiceError,
    pub context: ErrorContext,
}

impl ContextualError {
    /// Create a new contextual error
    pub fn new(error: ServiceError, context: ErrorContext) -> Self {
        Self { error, context }
    }

    /// Get the underlying service error
    pub fn service_error(&self) -> &ServiceError {
        &self.error
    }
}

/// Result type for service operations with context
pub type ContextualServiceResult<T> = Result<T, ContextualError>;