//! The error type ACP integration reports.
//!
//! This module used to declare four traits — `SessionManager`,
//! `FilesystemHandler`, `ToolBridge` and `StreamHandler` — as a dependency
//! inversion layer between `crucible-core` and the daemon's ACP module. Three
//! never had an implementor at all, and `SessionManager`'s single implementor
//! only set and cleared one field, with every caller a test. An ACP session is
//! an ordinary [`Session`](crate::session::Session) held by the daemon's
//! `SessionManager` struct; there was never a second kind of session for a
//! trait to abstract over.
//!
//! What is left is [`AcpError`], which the daemon's ACP tests raise. The
//! daemon's own client error is `crucible_daemon::acp::error::ClientError` and
//! is a different type.

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
