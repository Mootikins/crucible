//! Protocol message handling utilities
//!
//! This module provides utilities for working with ACP protocol messages,
//! including serialization, validation, and routing.
//!
//! ## Responsibilities
//!
//! - Message serialization and deserialization
//! - Protocol version handling
//! - Message validation
//! - Request/response matching
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on protocol-level operations
//! - **Open/Closed**: Extensible for new message types

use serde::{Deserialize, Serialize};

/// Protocol version information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolVersion {
    /// Major version number
    pub major: u32,
    /// Minor version number
    pub minor: u32,
    /// Patch version number
    pub patch: u32,
}

impl ProtocolVersion {
    /// Create a new protocol version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Check if this version is compatible with another version
    ///
    /// Versions are compatible if they have the same major version
    pub fn is_compatible_with(&self, other: &ProtocolVersion) -> bool {
        self.major == other.major
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        // Default to ACP 0.7.0 (current agent-client-protocol version)
        Self::new(0, 7, 0)
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Handles protocol message operations
///
/// This provides utilities for working with protocol messages,
/// ensuring correct formatting and validation.
#[derive(Debug)]
pub struct MessageHandler {
    version: ProtocolVersion,
}

impl MessageHandler {
    /// Create a new message handler with the given protocol version
    ///
    /// # Arguments
    ///
    /// * `version` - Protocol version to use
    pub fn new(version: ProtocolVersion) -> Self {
        Self { version }
    }

    /// Get the protocol version
    pub fn version(&self) -> &ProtocolVersion {
        &self.version
    }
}

impl Default for MessageHandler {
    fn default() -> Self {
        Self::new(ProtocolVersion::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        let v1 = ProtocolVersion::new(0, 7, 0);
        let v2 = ProtocolVersion::new(0, 8, 0);
        let v3 = ProtocolVersion::new(1, 0, 0);

        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
        assert_eq!(v1.to_string(), "0.7.0");
    }

    #[test]
    fn test_message_handler_creation() {
        let handler = MessageHandler::default();
        assert_eq!(handler.version().major, 0);
        assert_eq!(handler.version().minor, 7);
        assert_eq!(handler.version().patch, 0);
    }

    #[test]
    fn test_protocol_version_display() {
        let version = ProtocolVersion::new(1, 2, 3);
        assert_eq!(format!("{}", version), "1.2.3");
    }
}
