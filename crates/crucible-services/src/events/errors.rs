//! Event system error types

use thiserror::Error;

/// Event system error types
#[derive(Error, Debug)]
pub enum EventError {
    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Invalid event data
    #[error("Invalid event data: {0}")]
    InvalidEventData(String),

    /// Event routing failed
    #[error("Event routing failed: {0}")]
    RoutingError(String),

    /// Service not found for routing
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    /// Event filter error
    #[error("Event filter error: {0}")]
    FilterError(String),

    /// Event priority validation error
    #[error("Invalid event priority: {0}")]
    InvalidPriority(String),

    /// Event metadata error
    #[error("Event metadata error: {0}")]
    MetadataError(String),

    /// Event queue full
    #[error("Event queue is full (capacity: {capacity})")]
    QueueFull { capacity: usize },

    /// Event timeout
    #[error("Event processing timeout after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Event processing failed
    #[error("Event processing failed: {0}")]
    ProcessingError(String),

    /// Event source error
    #[error("Event source error: {0}")]
    SourceError(String),

    /// Event validation failed
    #[error("Event validation failed: {0}")]
    ValidationError(String),

    /// Event subscription error
    #[error("Event subscription error: {0}")]
    SubscriptionError(String),

    /// Event delivery failed
    #[error("Event delivery failed: {service_id} - {reason}")]
    DeliveryError { service_id: String, reason: String },

    /// Circuit breaker is open
    #[error("Circuit breaker is open for service: {0}")]
    CircuitBreakerOpen(String),

    /// Event too large
    #[error("Event too large: {size} bytes (max: {max_size} bytes)")]
    EventTooLarge { size: usize, max_size: usize },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for service: {0}")]
    RateLimitExceeded(String),

    /// Internal system error
    #[error("Internal system error: {0}")]
    InternalError(String),
}

impl EventError {
    /// Create a routing error
    pub fn routing_error(msg: impl Into<String>) -> Self {
        Self::RoutingError(msg.into())
    }

    /// Create a validation error
    pub fn validation_error(msg: impl Into<String>) -> Self {
        Self::ValidationError(msg.into())
    }

    /// Create a processing error
    pub fn processing_error(msg: impl Into<String>) -> Self {
        Self::ProcessingError(msg.into())
    }

    /// Create a delivery error
    pub fn delivery_error(service_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::DeliveryError {
            service_id: service_id.into(),
            reason: reason.into(),
        }
    }
}

/// Event result type
pub type EventResult<T> = Result<T, EventError>;