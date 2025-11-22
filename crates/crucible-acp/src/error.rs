//! Error types for ACP integration

use thiserror::Error;

/// Result type alias for ACP operations
pub type Result<T> = std::result::Result<T, AcpError>;

/// Errors that can occur during ACP operations
#[derive(Debug, Error)]
pub enum AcpError {
    /// Protocol-level errors from agent-client-protocol
    #[error("Protocol error: {0}")]
    Protocol(#[from] agent_client_protocol::Error),

    /// IO errors (file operations, network, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Session-related errors
    #[error("Session error: {0}")]
    Session(String),

    /// Agent connection errors
    #[error("Connection error: {0}")]
    Connection(String),

    /// Agent communication timeout
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// File system operation errors
    #[error("File system error: {0}")]
    FileSystem(String),

    /// Permission denied errors
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Resource not found errors
    #[error("Not found: {0}")]
    NotFound(String),

    /// General errors
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
