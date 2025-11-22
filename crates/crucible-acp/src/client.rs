//! ACP Client implementation for agent communication
//!
//! This module provides the main client interface for communicating with
//! AI agents via the Agent Client Protocol.
//!
//! ## Responsibilities
//!
//! - Agent process lifecycle management (start, stop, restart)
//! - Connection establishment and maintenance
//! - Protocol version negotiation
//! - Message routing to appropriate handlers
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on agent connection and lifecycle
//! - **Dependency Inversion**: Uses traits from crucible-core for extensibility
//! - **Open/Closed**: New agent types can be added without modifying this code

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;

use crate::{AcpError, Result};
use crate::session::AcpSession;
use crucible_core::traits::acp::{SessionManager, AcpResult};
use crucible_core::types::acp::{SessionConfig, SessionId};

/// Configuration for the ACP client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Path to the agent executable or script
    pub agent_path: PathBuf,

    /// Working directory for the agent process
    pub working_dir: Option<PathBuf>,

    /// Environment variables to pass to the agent
    pub env_vars: Option<Vec<(String, String)>>,

    /// Timeout for agent operations (in milliseconds)
    pub timeout_ms: Option<u64>,

    /// Maximum number of retry attempts
    pub max_retries: Option<u32>,
}

/// Main client for ACP communication
///
/// This struct manages the lifecycle of agent connections and provides
/// the primary interface for sending requests to agents.
#[derive(Debug)]
pub struct CrucibleAcpClient {
    config: ClientConfig,
    /// Current active session ID, if any
    active_session: Option<SessionId>,
}

impl CrucibleAcpClient {
    /// Create a new ACP client with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let config = ClientConfig {
    ///     agent_path: PathBuf::from("/path/to/agent"),
    ///     working_dir: None,
    ///     env_vars: None,
    ///     timeout_ms: Some(5000),
    ///     max_retries: Some(3),
    /// };
    /// let client = CrucibleAcpClient::new(config);
    /// ```
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            active_session: None,
        }
    }

    /// Connect to an agent and establish a session
    ///
    /// This will start the agent process if needed and perform protocol
    /// negotiation to establish a communication session.
    ///
    /// # Returns
    ///
    /// An active `AcpSession` that can be used to send requests to the agent
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The agent process cannot be started
    /// - Protocol negotiation fails
    /// - Connection times out
    pub async fn connect(&self) -> Result<AcpSession> {
        // TODO: Implement agent process startup and connection
        // This is a stub - will be implemented in TDD cycles
        Err(AcpError::Connection("Not yet implemented".to_string()))
    }

    /// Get the client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get the current active session, if any
    pub fn active_session(&self) -> Option<&SessionId> {
        self.active_session.as_ref()
    }
}

// TDD Cycle 5 - GREEN: Implement SessionManager trait
#[async_trait]
impl SessionManager for CrucibleAcpClient {
    type Session = SessionId;
    type Config = SessionConfig;

    async fn create_session(&mut self, config: Self::Config) -> AcpResult<Self::Session> {
        // TDD Cycle 6 - GREEN: Create session with basic state tracking
        // For now, we create a session ID and track it internally
        // Full agent connection will be implemented in later cycles

        // Generate a new session ID
        let session_id = SessionId::new();

        // Store session configuration in metadata
        let mut metadata = config.metadata.clone();
        metadata.insert(
            "cwd".to_string(),
            serde_json::json!(config.cwd.to_string_lossy())
        );
        metadata.insert(
            "mode".to_string(),
            serde_json::json!(format!("{:?}", config.mode))
        );

        // Track as active session
        self.active_session = Some(session_id.clone());

        Ok(session_id)
    }

    async fn load_session(&mut self, session: Self::Session) -> AcpResult<()> {
        // TDD Cycle 6 - GREEN: Track session loading
        // For now, just set it as active (actual restoration comes later)
        self.active_session = Some(session);
        Ok(())
    }

    async fn end_session(&mut self, session: Self::Session) -> AcpResult<()> {
        // TDD Cycle 6 - GREEN: Clean up session state
        if self.active_session.as_ref() == Some(&session) {
            self.active_session = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::acp::SessionManager;
    use crucible_core::types::acp::{SessionConfig, SessionId};

    #[test]
    fn test_client_creation() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let client = CrucibleAcpClient::new(config);
        assert_eq!(client.config().agent_path, PathBuf::from("/test/agent"));
    }

    // TDD Cycle 6 - Updated: Now expects successful session creation
    #[tokio::test]
    async fn test_client_implements_session_manager() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            working_dir: Some(PathBuf::from("/test/workspace")),
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Should start with no active session
        assert!(client.active_session().is_none());

        // Should implement SessionManager trait
        let session_config = SessionConfig {
            cwd: PathBuf::from("/test/workspace"),
            mode: crucible_core::types::acp::ChatMode::Plan,
            context_size: 5,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: std::collections::HashMap::new(),
        };

        // This should now succeed and create a session
        let result = client.create_session(session_config).await;
        assert!(result.is_ok(), "Should successfully create session");

        // Should track active session
        let session_id = result.unwrap();
        assert!(client.active_session().is_some());
        assert_eq!(client.active_session(), Some(&session_id));
    }

    // TDD Cycle 6 - Updated: Full session lifecycle should work
    #[tokio::test]
    async fn test_session_lifecycle() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            working_dir: Some(PathBuf::from("/test/workspace")),
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        let session_config = SessionConfig {
            cwd: PathBuf::from("/test/workspace"),
            mode: crucible_core::types::acp::ChatMode::Plan,
            context_size: 5,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: std::collections::HashMap::new(),
        };

        // Create session should now succeed
        let create_result = client.create_session(session_config).await;
        assert!(create_result.is_ok());
        let session_id = create_result.unwrap();

        // Should be able to load session
        let load_result = client.load_session(session_id.clone()).await;
        assert!(load_result.is_ok());
        assert_eq!(client.active_session(), Some(&session_id));

        // Should be able to end session
        let end_result = client.end_session(session_id).await;
        assert!(end_result.is_ok());
        assert!(client.active_session().is_none());
    }

    // TDD Cycle 6 - RED: Test expects successful session creation with mock agent
    #[tokio::test]
    async fn test_session_creation_with_mock_agent() {
        use crate::mock_agent::{MockAgent, MockAgentConfig};
        use std::collections::HashMap;

        // Create a mock agent that will respond successfully
        let mut responses = HashMap::new();
        responses.insert(
            "initialize".to_string(),
            serde_json::json!({
                "agent_capabilities": {},
                "agent_info": {
                    "name": "mock-agent",
                    "version": "0.1.0"
                }
            })
        );
        responses.insert(
            "new_session".to_string(),
            serde_json::json!({
                "session_id": "test-session-123"
            })
        );

        let mock_config = MockAgentConfig {
            responses,
            simulate_delay: false,
            delay_ms: 0,
            simulate_errors: false,
        };
        let _mock_agent = MockAgent::new(mock_config);

        // TODO: Once we implement the actual connection logic,
        // this test will verify that we can create a session with the mock agent
        // For now, this is a placeholder showing the expected API
    }

    #[tokio::test]
    async fn test_session_initialization_flow() {
        // TDD Cycle 6 - RED: This test expects the full initialization flow
        // 1. Connect to agent (or mock)
        // 2. Send initialize request
        // 3. Create new session
        // 4. Return session ID

        // This will fail until we implement the connection logic
        // but defines the expected behavior
    }
}
