//! Error types for ACP integration

use thiserror::Error;

/// Result type alias for ACP operations
pub type Result<T> = std::result::Result<T, ClientError>;

/// Errors that can occur during ACP client operations.
///
/// Each variant has its own `TurnError` in `acp_handle::translate`.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The agent answered a request with an error. The text keeps the
    /// agent's own words.
    #[error("Session error: {0}")]
    Session(String),

    /// The agent process did not start, or the connection to it ended.
    #[error("Connection error: {0}")]
    Connection(String),

    /// The agent did not answer in time.
    #[error("Operation timed out: {0}")]
    Timeout(String),
}
