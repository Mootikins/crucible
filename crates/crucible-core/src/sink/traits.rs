//! Traits for pipeline output sinks

use super::error::SinkResult;
use crate::parser::ParsedDocument;
use async_trait::async_trait;

/// Trait for pipeline output destinations
///
/// Output sinks receive parsed documents from the pipeline and write them to
/// their destination (database, logger, file, etc.). Each sink runs independently
/// in its own async task to provide fault isolation.
///
/// # Design Principles
///
/// 1. **Isolation**: Errors in one sink must not affect others
/// 2. **Backpressure**: Sinks should handle slow writes gracefully
/// 3. **Observability**: Health checks enable monitoring
/// 4. **Graceful Shutdown**: Flush buffered data before exit
///
/// # Threading
///
/// Each sink runs in a separate tokio task and receives documents via a
/// broadcast channel. Sinks must be Send + Sync.
///
/// # Error Handling
///
/// Sinks should handle transient errors internally (retry, buffer, etc.)
/// and only return errors for fatal conditions that require pipeline attention.
#[async_trait]
pub trait OutputSink: Send + Sync {
    /// Process a parsed document
    ///
    /// This is the main entry point for writing documents. Implementations should:
    /// - Validate the document
    /// - Transform as needed for destination
    /// - Buffer writes if appropriate
    /// - Handle transient errors with retry
    ///
    /// # Errors
    ///
    /// Should return `SinkError` only for fatal errors. Transient errors should
    /// be handled internally with logging.
    ///
    /// # Performance
    ///
    /// This method should be fast (<10ms for buffered writes). Slow operations
    /// should be batched and flushed separately.
    async fn write(&self, doc: ParsedDocument) -> SinkResult<()>;

    /// Flush buffered writes to destination
    ///
    /// Called periodically by the pipeline and before shutdown. Implementations
    /// should ensure all buffered data is persisted.
    ///
    /// # Timeout
    ///
    /// Implementations should timeout flush operations (default: 30s) to prevent
    /// blocking shutdown indefinitely.
    async fn flush(&self) -> SinkResult<()>;

    /// Get the sink name for logging and metrics
    ///
    /// Should return a static string identifier for this sink instance.
    fn name(&self) -> &'static str;

    /// Get the sink health status
    ///
    /// Used for monitoring and circuit breaker decisions. Should return:
    /// - `Healthy`: Normal operation
    /// - `Degraded`: Experiencing issues but still functional
    /// - `Unhealthy`: Cannot process writes, circuit should open
    ///
    /// # Performance
    ///
    /// This should be fast (<1ms) as it may be called frequently.
    async fn health_check(&self) -> SinkHealth;

    /// Graceful shutdown
    ///
    /// Called when the pipeline is shutting down. Should:
    /// 1. Stop accepting new writes
    /// 2. Flush all buffered data
    /// 3. Close connections
    /// 4. Clean up resources
    ///
    /// # Timeout
    ///
    /// Implementations should complete within 30 seconds or force-close.
    async fn shutdown(&self) -> SinkResult<()> {
        // Default implementation: just flush
        self.flush().await
    }

    /// Get sink configuration (for debugging)
    fn config(&self) -> SinkConfig {
        SinkConfig::default()
    }
}

/// Health status of a sink
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkHealth {
    /// Sink is operating normally
    Healthy,

    /// Sink is experiencing issues but still functional
    ///
    /// Examples: high latency, partial connectivity, approaching limits
    Degraded {
        /// Reason for degradation
        reason: String,
    },

    /// Sink cannot process writes
    ///
    /// Examples: connection lost, disk full, authentication failed
    Unhealthy {
        /// Reason for unhealthy status
        reason: String,
    },
}

impl SinkHealth {
    /// Check if sink is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Check if sink is degraded
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    /// Check if sink is unhealthy
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, Self::Unhealthy { .. })
    }

    /// Get the status as a string
    pub fn status(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded { .. } => "degraded",
            Self::Unhealthy { .. } => "unhealthy",
        }
    }

    /// Get the reason if degraded or unhealthy
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Degraded { reason } | Self::Unhealthy { reason } => Some(reason),
            Self::Healthy => None,
        }
    }
}

/// Sink configuration metadata
#[derive(Debug, Clone)]
pub struct SinkConfig {
    /// Buffer size (if buffered)
    pub buffer_size: Option<usize>,

    /// Flush interval (if buffered)
    pub flush_interval: Option<std::time::Duration>,

    /// Retry configuration
    pub max_retries: Option<u32>,

    /// Timeout for write operations
    pub write_timeout: Option<std::time::Duration>,

    /// Custom configuration properties
    pub properties: std::collections::HashMap<String, String>,
}

impl SinkConfig {
    /// Create a new empty config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = Some(size);
        self
    }

    /// Set flush interval
    pub fn with_flush_interval(mut self, interval: std::time::Duration) -> Self {
        self.flush_interval = Some(interval);
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Set write timeout
    pub fn with_write_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.write_timeout = Some(timeout);
        self
    }

    /// Add a custom property
    pub fn with_property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }
}

impl Default for SinkConfig {
    fn default() -> Self {
        Self {
            buffer_size: None,
            flush_interval: None,
            max_retries: Some(3),
            write_timeout: Some(std::time::Duration::from_secs(5)),
            properties: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sink_health_status() {
        let health = SinkHealth::Healthy;
        assert!(health.is_healthy());
        assert!(!health.is_degraded());
        assert!(!health.is_unhealthy());
        assert_eq!(health.status(), "healthy");
        assert_eq!(health.reason(), None);

        let health = SinkHealth::Degraded {
            reason: "high latency".to_string(),
        };
        assert!(!health.is_healthy());
        assert!(health.is_degraded());
        assert_eq!(health.status(), "degraded");
        assert_eq!(health.reason(), Some("high latency"));

        let health = SinkHealth::Unhealthy {
            reason: "connection lost".to_string(),
        };
        assert!(health.is_unhealthy());
        assert_eq!(health.reason(), Some("connection lost"));
    }

    #[test]
    fn test_sink_config_builder() {
        let config = SinkConfig::new()
            .with_buffer_size(100)
            .with_flush_interval(std::time::Duration::from_secs(5))
            .with_max_retries(5)
            .with_property("db_name".to_string(), "vault".to_string());

        assert_eq!(config.buffer_size, Some(100));
        assert_eq!(config.max_retries, Some(5));
        assert_eq!(config.properties.get("db_name"), Some(&"vault".to_string()));
    }
}
