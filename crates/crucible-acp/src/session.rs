//! Session management for ACP connections
//!
//! This module handles the lifecycle and state of individual agent sessions.
//!
//! ## Responsibilities
//!
//! - Session state management (active, idle, closed)
//! - Message sending and receiving
//! - Session-level error handling and recovery
//! - Resource cleanup on session termination
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on session lifecycle and message exchange
//! - **Open/Closed**: Extensible through configuration without modification

use serde::{Deserialize, Serialize};

use crate::{AcpError, Result};
use agent_client_protocol::{ClientRequest, ClientResponse};

/// Configuration for an ACP session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Session timeout in milliseconds
    pub timeout_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable debug logging for this session
    pub debug: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000, // 30 seconds
            max_message_size: 10 * 1024 * 1024, // 10 MB
            debug: false,
        }
    }
}

/// Represents an active session with an agent
///
/// The session handles communication with a connected agent,
/// including sending requests and receiving responses.
#[derive(Debug)]
pub struct AcpSession {
    config: SessionConfig,
    session_id: String,
}

impl AcpSession {
    /// Create a new session with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Session configuration
    /// * `session_id` - Unique identifier for this session
    pub fn new(config: SessionConfig, session_id: String) -> Self {
        Self { config, session_id }
    }

    /// Get the session ID
    pub fn id(&self) -> &str {
        &self.session_id
    }

    /// Send a request to the agent and wait for a response
    ///
    /// # Arguments
    ///
    /// * `request` - The request to send
    ///
    /// # Returns
    ///
    /// The agent's response
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The session is closed
    /// - The request times out
    /// - The agent returns an error
    /// - Serialization/deserialization fails
    pub async fn send_request(&self, _request: ClientRequest) -> Result<ClientResponse> {
        // TODO: Implement request sending
        // This is a stub - will be implemented in TDD cycles
        Err(AcpError::Session("Not yet implemented".to_string()))
    }

    /// Close the session and cleanup resources
    ///
    /// This will gracefully terminate the session and notify the agent.
    pub async fn close(self) -> Result<()> {
        // TODO: Implement session cleanup
        // This is a stub - will be implemented in TDD cycles
        Ok(())
    }

    /// Check if the session is still active
    pub fn is_active(&self) -> bool {
        // TODO: Implement session state tracking
        // This is a stub - will be implemented in TDD cycles
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let config = SessionConfig::default();
        let session = AcpSession::new(config, "test-session-id".to_string());
        assert_eq!(session.id(), "test-session-id");
        assert!(session.is_active());
    }

    #[test]
    fn test_default_config() {
        let config = SessionConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert!(!config.debug);
    }
}
