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
        Self { config }
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
}

// TDD Cycle 5 - GREEN: Implement SessionManager trait
#[async_trait]
impl SessionManager for CrucibleAcpClient {
    type Session = SessionId;
    type Config = SessionConfig;

    async fn create_session(&mut self, _config: Self::Config) -> AcpResult<Self::Session> {
        // TODO: Implement actual agent connection and session creation
        // For now, return an error since we haven't implemented the connection logic
        Err(crucible_core::traits::acp::AcpError::Session(
            "Session creation not yet implemented - need agent connection".to_string()
        ))
    }

    async fn load_session(&mut self, _session: Self::Session) -> AcpResult<()> {
        // TODO: Implement session loading from storage/agent
        Err(crucible_core::traits::acp::AcpError::Session(
            "Session loading not yet implemented".to_string()
        ))
    }

    async fn end_session(&mut self, _session: Self::Session) -> AcpResult<()> {
        // TODO: Implement session cleanup
        Err(crucible_core::traits::acp::AcpError::Session(
            "Session cleanup not yet implemented".to_string()
        ))
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

    // TDD Cycle 5 - RED: Test expecting Client trait implementation
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

        // Should implement SessionManager trait
        let session_config = SessionConfig {
            cwd: PathBuf::from("/test/workspace"),
            mode: crucible_core::types::acp::ChatMode::Plan,
            context_size: 5,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: std::collections::HashMap::new(),
        };

        // This should compile and work (test trait implementation)
        let result = client.create_session(session_config).await;

        // For now, we expect it to fail since we haven't connected to an agent yet
        // But the trait implementation should exist
        assert!(result.is_err(), "Should fail without agent connection");
    }

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

        // Try to create, load, and end sessions
        // These should fail gracefully without an agent but the interface should exist
        let create_result = client.create_session(session_config).await;
        assert!(create_result.is_err());

        // Test load_session interface
        let session_id = SessionId::new();
        let load_result = client.load_session(session_id.clone()).await;
        assert!(load_result.is_err());

        // Test end_session interface
        let end_result = client.end_session(session_id).await;
        assert!(end_result.is_err());
    }
}
