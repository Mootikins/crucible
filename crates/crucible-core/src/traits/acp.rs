//! Agent Client Protocol (ACP) abstraction traits
//!
//! This module defines the core abstractions for integrating with ACP-compatible agents
//! (Claude Code, Gemini CLI, etc.) while maintaining strict dependency inversion.
//!
//! ## Architecture Pattern
//!
//! Following SOLID principles (Interface Segregation & Dependency Inversion):
//! - **crucible-core** defines traits and associated types (this module)
//! - **crucible-acp** implements ACP-specific protocol logic
//! - **crucible-cli** provides concrete implementations and glue code
//!
//! ## Design Principles
//!
//! **Interface Segregation**: Separate traits for distinct capabilities
//! - `SessionManager` - Session lifecycle (create, load, end)
//! - `FilesystemHandler` - File operations (read, write, list)
//! - `ToolBridge` - Tool discovery and execution
//! - `StreamHandler` - Response streaming
//!
//! **Dependency Inversion**: Traits use associated types for flexibility
//! - Implementations choose concrete types (SessionId, ToolCall, etc.)
//! - Core never depends on concrete implementations
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crucible_core::traits::acp::{SessionManager, FilesystemHandler};
//!
//! async fn start_chat<S, F>(session_mgr: &mut S, fs: &F)
//! where
//!     S: SessionManager,
//!     F: FilesystemHandler,
//! {
//!     let session = session_mgr.create_session(config).await?;
//!     let content = fs.read_file("/path/to/file").await?;
//!     // Use session and file content
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};


/// Result type for ACP operations
pub type AcpResult<T> = Result<T, AcpError>;

/// ACP operation errors
///
/// Covers common failure modes across all ACP operations.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AcpError {
    #[error("Session error: {0}")]
    Session(String),

    #[error("Filesystem error: {0}")]
    Filesystem(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Session management abstraction
///
/// Handles the lifecycle of ACP sessions (create, load, end). Sessions represent
/// conversations with an agent, including context, mode (plan/act), and history.
///
/// ## Design Rationale
///
/// Uses associated types to allow implementations to choose their session representation:
/// - `Session` - The session handle/identifier type
/// - `Config` - Configuration for new sessions
///
/// ## Thread Safety
///
/// Implementations must be Send to enable use across async boundaries.
/// Sessions are not required to be Sync as they represent single-conversation state.
///
/// ## Example Implementation
///
/// ```rust,ignore
/// impl SessionManager for AcpClient {
///     type Session = SessionId;
///     type Config = SessionConfig;
///
///     async fn create_session(&mut self, config: Self::Config) -> AcpResult<Self::Session> {
///         // Initialize agent, send configuration, return session ID
///     }
/// }
/// ```
#[async_trait]
pub trait SessionManager: Send {
    /// The session handle/identifier type
    type Session: Send;

    /// Session configuration type
    type Config: Send;

    /// Create a new session with the given configuration
    ///
    /// Initializes a new conversation session with an ACP agent, including:
    /// - Working directory setup
    /// - Chat mode (plan/act) configuration
    /// - Context size limits
    /// - History initialization
    ///
    /// # Arguments
    ///
    /// * `config` - Session configuration (cwd, mode, context_size, etc.)
    ///
    /// # Returns
    ///
    /// Returns a session handle on success, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::Session` - Session initialization failed
    /// - `AcpError::Protocol` - Agent communication error
    /// - `AcpError::InvalidOperation` - Invalid configuration
    async fn create_session(&mut self, config: Self::Config) -> AcpResult<Self::Session>;

    /// Load an existing session by identifier
    ///
    /// Restores a previously created session, including conversation history
    /// and state. May fail if the session has expired or doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `session` - The session identifier to load
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful restoration, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::NotFound` - Session doesn't exist
    /// - `AcpError::Session` - Session corrupted or expired
    async fn load_session(&mut self, session: Self::Session) -> AcpResult<()>;

    /// End a session and clean up resources
    ///
    /// Gracefully terminates the session, saving any necessary state and
    /// releasing resources. After calling this, the session handle is invalid.
    ///
    /// # Arguments
    ///
    /// * `session` - The session to terminate
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful cleanup, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::Session` - Cleanup failed (resources may leak)
    async fn end_session(&mut self, session: Self::Session) -> AcpResult<()>;
}

/// Tool discovery and execution abstraction
///
/// Bridges ACP tool calls to Crucible's tool system (MCP-compatible tools for
/// semantic search, note operations, kiln management, etc.).
///
/// ## Design Rationale
///
/// Separate from `FilesystemHandler` to follow Interface Segregation:
/// - Filesystem operations are standard ACP (workspace files)
/// - Tool operations are application-specific (knowledge base operations)
///
/// Uses associated types for tool representation:
/// - `ToolCall` - Represents a tool invocation request
/// - `ToolResult` - Represents the execution result
/// - `ToolDescriptor` - Metadata for tool discovery
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable concurrent tool execution.
///
/// ## Example Implementation
///
/// ```rust,ignore
/// impl ToolBridge for CrucibleToolBridge {
///     type ToolCall = ToolInvocation;
///     type ToolResult = serde_json::Value;
///     type ToolDescriptor = ToolMetadata;
///
///     async fn execute_tool(&self, call: Self::ToolCall) -> AcpResult<Self::ToolResult> {
///         // Route to appropriate Crucible tool, return result
///     }
/// }
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    // Test that error types are cloneable and serializable
    #[test]
    fn test_acp_error_clone_serialize() {
        let err = AcpError::Session("test error".to_string());
        let cloned = err.clone();
        assert_eq!(format!("{}", err), format!("{}", cloned));

        let json = serde_json::to_string(&err).unwrap();
        let deserialized: AcpError = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{}", err), format!("{}", deserialized));
    }

    // Test error variants
    #[test]
    fn test_error_variants() {
        let errors = vec![
            AcpError::Session("session error".to_string()),
            AcpError::Filesystem("filesystem error".to_string()),
            AcpError::Tool("tool error".to_string()),
            AcpError::Stream("stream error".to_string()),
            AcpError::Protocol("protocol error".to_string()),
            AcpError::PermissionDenied("permission denied".to_string()),
            AcpError::NotFound("not found".to_string()),
            AcpError::InvalidOperation("invalid operation".to_string()),
            AcpError::Internal("internal error".to_string()),
        ];

        for err in errors {
            let msg = format!("{}", err);
            assert!(!msg.is_empty());
        }
    }
}
