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
use std::path::PathBuf;

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

/// Filesystem operations abstraction
///
/// Handles file operations requested by ACP agents (read, write, list).
/// These operations are scoped to the workspace directory and respect
/// the session's chat mode (plan/act) for permission enforcement.
///
/// ## Design Rationale
///
/// Separate from `ToolBridge` to follow Interface Segregation:
/// - Filesystem operations are standard ACP primitives (ReadTextFileRequest, etc.)
/// - Tool operations are higher-level, application-specific
///
/// Uses associated types for file content representation:
/// - `FileContent` - The type representing file contents (String, Vec<u8>, etc.)
///
/// ## Permission Model
///
/// - **Plan mode**: Read-only (writes return `PermissionDenied`)
/// - **Act mode**: Read and write allowed
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable concurrent file operations.
///
/// ## Example Implementation
///
/// ```rust,ignore
/// impl FilesystemHandler for AcpFilesystem {
///     type FileContent = String;
///
///     async fn read_file(&self, path: &Path) -> AcpResult<Self::FileContent> {
///         // Resolve path relative to CWD, read file, return contents
///     }
/// }
/// ```
#[async_trait]
pub trait FilesystemHandler: Send + Sync {
    /// File content representation type
    type FileContent: Send;

    /// Read a text file from the filesystem
    ///
    /// Reads the file at the given path (relative to session CWD) and returns
    /// its contents. This corresponds to ACP's `ReadTextFileRequest`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (relative or absolute)
    ///
    /// # Returns
    ///
    /// Returns the file contents on success, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::NotFound` - File doesn't exist
    /// - `AcpError::Filesystem` - Read failed (permissions, encoding, etc.)
    /// - `AcpError::PermissionDenied` - Path outside allowed scope
    async fn read_file(&self, path: &PathBuf) -> AcpResult<Self::FileContent>;

    /// Write content to a text file
    ///
    /// Writes content to the file at the given path. Creates the file if it
    /// doesn't exist, overwrites if it does. Respects plan/act mode permissions.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (relative or absolute)
    /// * `content` - Content to write
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::PermissionDenied` - Plan mode blocks writes
    /// - `AcpError::Filesystem` - Write failed (permissions, disk space, etc.)
    async fn write_file(&self, path: &PathBuf, content: Self::FileContent) -> AcpResult<()>;

    /// List files in a directory
    ///
    /// Returns a list of file paths in the given directory. Used for agent
    /// exploration of the workspace.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path to list
    /// * `recursive` - Whether to list recursively
    ///
    /// # Returns
    ///
    /// Returns a vector of file paths, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::NotFound` - Directory doesn't exist
    /// - `AcpError::Filesystem` - Read failed
    /// - `AcpError::PermissionDenied` - Path outside allowed scope
    async fn list_files(&self, path: &PathBuf, recursive: bool) -> AcpResult<Vec<PathBuf>>;
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
#[async_trait]
pub trait ToolBridge: Send + Sync {
    /// Tool invocation request type
    type ToolCall: Send;

    /// Tool execution result type
    type ToolResult: Send;

    /// Tool metadata/descriptor type
    type ToolDescriptor: Send;

    /// Execute a tool with the given invocation
    ///
    /// Routes the tool call to the appropriate Crucible tool implementation
    /// (semantic_search, read_note, etc.) and returns the result.
    ///
    /// # Arguments
    ///
    /// * `call` - The tool invocation (name, parameters, context)
    ///
    /// # Returns
    ///
    /// Returns the tool execution result on success, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::NotFound` - Tool doesn't exist
    /// - `AcpError::Tool` - Execution failed
    /// - `AcpError::InvalidOperation` - Invalid parameters
    async fn execute_tool(&self, call: Self::ToolCall) -> AcpResult<Self::ToolResult>;

    /// List all available tools
    ///
    /// Returns metadata about all available Crucible tools for agent discovery.
    /// Used during session initialization to populate the agent's tool catalog.
    ///
    /// # Returns
    ///
    /// Returns a vector of tool descriptors, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::Internal` - Tool discovery failed
    async fn list_tools(&self) -> AcpResult<Vec<Self::ToolDescriptor>>;

    /// Get the schema for a specific tool
    ///
    /// Returns the JSON Schema describing the tool's parameters and return type.
    /// Used by agents to understand how to invoke the tool.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool
    ///
    /// # Returns
    ///
    /// Returns the JSON Schema, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::NotFound` - Tool doesn't exist
    /// - `AcpError::Internal` - Schema generation failed
    async fn get_tool_schema(&self, tool_name: &str) -> AcpResult<serde_json::Value>;
}

/// Response streaming abstraction
///
/// Handles real-time streaming of agent responses to the user interface.
/// Processes chunks as they arrive and signals completion.
///
/// ## Design Rationale
///
/// Separate from `SessionManager` to follow Interface Segregation:
/// - Session management handles lifecycle
/// - Stream handling processes real-time output
///
/// Uses associated types for chunk representation:
/// - `Chunk` - A single streaming chunk (text, tool call, etc.)
///
/// ## Thread Safety
///
/// Implementations must be Send to enable async processing of chunks.
///
/// ## Example Implementation
///
/// ```rust,ignore
/// impl StreamHandler for TerminalStreamer {
///     type Chunk = StreamChunk;
///
///     async fn on_stream_chunk(&mut self, chunk: Self::Chunk) -> AcpResult<()> {
///         // Write chunk to terminal, update UI
///     }
/// }
/// ```
#[async_trait]
pub trait StreamHandler: Send {
    /// Stream chunk type
    type Chunk: Send;

    /// Handle a streaming response chunk
    ///
    /// Called for each chunk as it arrives from the agent. Implementations
    /// should update the UI or accumulate the chunk for later processing.
    ///
    /// # Arguments
    ///
    /// * `chunk` - The stream chunk (text, tool call, metadata, etc.)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` to continue streaming, or an `AcpError` to abort.
    ///
    /// # Errors
    ///
    /// - `AcpError::Stream` - Chunk processing failed
    /// - `AcpError::Internal` - UI update failed
    async fn on_stream_chunk(&mut self, chunk: Self::Chunk) -> AcpResult<()>;

    /// Handle stream completion
    ///
    /// Called when the agent finishes streaming a response. Implementations
    /// should finalize the UI, persist history, etc.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful completion, or an `AcpError`.
    ///
    /// # Errors
    ///
    /// - `AcpError::Stream` - Completion handling failed
    /// - `AcpError::Internal` - Finalization failed
    async fn on_stream_complete(&mut self) -> AcpResult<()>;
}

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
