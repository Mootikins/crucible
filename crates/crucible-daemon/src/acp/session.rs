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

/// ACP transport layer configuration.
///
/// Settings for the underlying ACP client transport (timeouts, message limits).
/// This is distinct from `crucible_core::SessionConfig` which is for
/// high-level session parameters (working directory, modes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Session timeout in milliseconds
    pub timeout_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable debug logging for this session
    pub debug: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,                  // 30 seconds
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
    session_id: String,
}

impl AcpSession {
    /// Create a new session with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Session configuration
    /// * `session_id` - Unique identifier for this session
    pub fn new(_config: TransportConfig, session_id: String) -> Self {
        Self { session_id }
    }

    /// Get the session ID
    pub fn id(&self) -> &str {
        &self.session_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let config = TransportConfig::default();
        let session = AcpSession::new(config, "test-session-id".to_string());
        assert_eq!(session.id(), "test-session-id");
    }

    #[test]
    fn test_default_config() {
        let config = TransportConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert!(!config.debug);
    }
}
