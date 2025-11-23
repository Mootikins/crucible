//! Mock agent implementation for testing
//!
//! This module provides a mock agent that can be used in tests without
//! requiring a real agent process.
//!
//! ## Responsibilities
//!
//! - Simulate agent behavior for testing
//! - Provide configurable responses
//! - Support testing error conditions
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on test support
//! - **Test Isolation**: Enables testing without external dependencies

use std::collections::HashMap;
use serde_json::Value;

use agent_client_protocol::ClientRequest;
use crate::{AcpError, Result};

/// Configuration for the mock agent
#[derive(Debug, Clone)]
pub struct MockAgentConfig {
    /// Predefined responses for specific methods
    pub responses: HashMap<String, Value>,

    /// Whether to simulate delays
    pub simulate_delay: bool,

    /// Delay duration in milliseconds
    pub delay_ms: u64,

    /// Whether to simulate errors
    pub simulate_errors: bool,
}

impl Default for MockAgentConfig {
    fn default() -> Self {
        Self {
            responses: HashMap::new(),
            simulate_delay: false,
            delay_ms: 0,
            simulate_errors: false,
        }
    }
}

/// Mock agent for testing
///
/// This provides a simple in-memory agent that can be configured
/// to return specific responses for testing purposes.
#[derive(Debug)]
pub struct MockAgent {
    config: MockAgentConfig,
    request_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl MockAgent {
    /// Create a new mock agent with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Mock agent configuration
    pub fn new(config: MockAgentConfig) -> Self {
        Self {
            config,
            request_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Add a response for a specific method
    ///
    /// # Arguments
    ///
    /// * `method` - The method name
    /// * `response` - The response to return
    pub fn add_response(&mut self, method: String, response: Value) {
        self.config.responses.insert(method, response);
    }

    /// Handle a request from a client
    ///
    /// # Arguments
    ///
    /// * `_request` - The request to handle
    ///
    /// # Returns
    ///
    /// A response based on the mock configuration
    ///
    /// # Note
    ///
    /// This is a stub that will be fully implemented in TDD cycles
    pub async fn handle_request(&self, _request: ClientRequest) -> Result<()> {
        // Increment request counter
        self.request_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Simulate delay if configured
        if self.config.simulate_delay {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.config.delay_ms)).await;
        }

        // Simulate errors if configured
        if self.config.simulate_errors {
            return Err(AcpError::Session("Simulated error".to_string()));
        }

        // TODO: Implement proper response construction based on request type
        // This will be implemented in TDD cycles
        Ok(())
    }

    /// Get the number of requests handled
    pub fn request_count(&self) -> usize {
        self.request_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Reset the request counter
    pub fn reset_count(&self) {
        self.request_count.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for MockAgent {
    fn default() -> Self {
        Self::new(MockAgentConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{ClientRequest, InitializeRequest, NewSessionRequest, ProtocolVersion, ClientCapabilities};
    use std::path::PathBuf;

    #[test]
    fn test_mock_agent_creation() {
        let agent = MockAgent::default();
        assert_eq!(agent.request_count(), 0);
    }

    #[test]
    fn test_mock_agent_config() {
        let mut config = MockAgentConfig::default();
        config.responses.insert(
            "custom_method".to_string(),
            serde_json::json!({"custom": "response"}),
        );
        config.simulate_delay = true;
        config.delay_ms = 100;

        let agent = MockAgent::new(config);
        assert_eq!(agent.request_count(), 0);
    }

    #[test]
    fn test_request_counter() {
        let agent = MockAgent::default();
        assert_eq!(agent.request_count(), 0);

        agent.request_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(agent.request_count(), 1);

        agent.reset_count();
        assert_eq!(agent.request_count(), 0);
    }

    #[tokio::test]
    async fn test_mock_agent_responds_to_initialize() {
        let agent = MockAgent::default();

        // Create an initialize request
        let request = ClientRequest::InitializeRequest(InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        });

        // This should succeed and not error
        let result = agent.handle_request(request).await;
        assert!(result.is_ok(), "Mock agent should respond to initialize request");
        assert_eq!(agent.request_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_agent_handles_new_session() {
        let agent = MockAgent::default();

        // Create a new session request
        let request = ClientRequest::NewSessionRequest(NewSessionRequest {
            cwd: PathBuf::from("/test"),
            mcp_servers: vec![],
            meta: None,
        });

        // Should handle session creation
        let result = agent.handle_request(request).await;
        assert!(result.is_ok(), "Mock agent should handle new session request");
        assert_eq!(agent.request_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_agent_error_simulation() {
        let mut config = MockAgentConfig::default();
        config.simulate_errors = true;
        let agent = MockAgent::new(config);

        let request = ClientRequest::InitializeRequest(InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        });

        let result = agent.handle_request(request).await;
        assert!(result.is_err(), "Should simulate errors when configured");
    }

    #[tokio::test]
    async fn test_mock_agent_delay_simulation() {
        let mut config = MockAgentConfig::default();
        config.simulate_delay = true;
        config.delay_ms = 50;
        let agent = MockAgent::new(config);

        let request = ClientRequest::InitializeRequest(InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        });

        let start = std::time::Instant::now();
        let _result = agent.handle_request(request).await;
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() >= 50, "Should simulate delay");
    }
}
