//! Configuration for the Obsidian HTTP client

use std::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
        }
    }
}

/// Configuration for the Obsidian client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Port number for the Obsidian HTTP API
    pub port: u16,
    /// Request timeout
    pub timeout: Duration,
    /// Retry configuration
    pub retry: RetryConfig,
    /// Maximum number of idle connections per host
    pub max_idle_per_host: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            port: 27123,
            timeout: Duration::from_secs(30),
            retry: RetryConfig::default(),
            max_idle_per_host: 10,
        }
    }
}

impl ClientConfig {
    /// Create a new builder for client configuration
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::default()
    }

    /// Get the base URL for the Obsidian API
    pub fn base_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

/// Builder for ClientConfig
#[derive(Debug, Default)]
pub struct ClientConfigBuilder {
    port: Option<u16>,
    timeout: Option<Duration>,
    retry: Option<RetryConfig>,
    max_idle_per_host: Option<usize>,
}

impl ClientConfigBuilder {
    /// Set the port number
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the retry configuration
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Set the maximum number of idle connections per host
    pub fn max_idle_per_host(mut self, max: usize) -> Self {
        self.max_idle_per_host = Some(max);
        self
    }

    /// Build the client configuration
    pub fn build(self) -> ClientConfig {
        let defaults = ClientConfig::default();
        ClientConfig {
            port: self.port.unwrap_or(defaults.port),
            timeout: self.timeout.unwrap_or(defaults.timeout),
            retry: self.retry.unwrap_or(defaults.retry),
            max_idle_per_host: self.max_idle_per_host.unwrap_or(defaults.max_idle_per_host),
        }
    }
}
