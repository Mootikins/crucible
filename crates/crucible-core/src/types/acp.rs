//! ACP domain types for cross-crate handoff
//!
//! This module contains concrete data structures used across the ACP integration.
//! These types are implementation-independent and serve as the "lingua franca"
//! between crucible-core, crucible-daemon (acp module), and crucible-cli.
//!
//! ## Design Principles
//!
//! - **Pure data**: No business logic, just structure
//! - **Serializable**: All types support serde for persistence/transport
//! - **Cross-crate**: Designed for use across module boundaries
//! - **Associated types**: Used as concrete types in trait implementations
//!
//! ## Organization
//!
//! - **Tool types**: FileDiff (TurnEvent::ToolCall carries it).
//!   ToolDefinition lives in traits::tools.

use serde::{Deserialize, Serialize};

// ============================================================================
// ACP Schema Re-exports
// ============================================================================

/// Re-exports from agent-client-protocol-schema for ACP protocol types.
///
/// These types are the canonical definitions from the ACP protocol specification.
/// They are used for interoperability with external agents and the protocol.
pub mod schema {
    // Mode types from ACP protocol
    pub use agent_client_protocol_schema::v1::{SessionMode, SessionModeId, SessionModeState};

    // Command types from ACP protocol
    pub use agent_client_protocol_schema::v1::{
        AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate,
    };

    // Session config options: the settings an agent advertises for itself.
    pub use agent_client_protocol_schema::v1::{
        SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelect,
        SessionConfigSelectOptions,
    };
}

/// File diff representing changes to a file
///
/// Protocol-agnostic representation of file modifications. Can be populated
/// from ACP's `ToolCallContent::Diff`, generated from tool arguments, or
/// computed by comparing file states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// Path to the modified file
    pub path: String,

    /// Original content (None for new files)
    pub old_content: Option<String>,

    /// New content after modification
    pub new_content: String,
}

/// Maximum byte size of either side of a diff before it is suppressed.
///
/// Producers (daemon-side synth, ACP forwarding) drop oversize diffs at the
/// edge so the cache and renderer never have to hold huge payloads. The
/// renderer applies the same threshold defensively.
pub const MAX_DIFF_BYTES: usize = 1024 * 1024;

impl FileDiff {
    /// Create a new file diff (no prior content; for new-file creation cases)
    pub fn new(path: impl Into<String>, new_content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            old_content: None,
            new_content: new_content.into(),
        }
    }

    /// True if either side exceeds [`MAX_DIFF_BYTES`].
    pub fn is_oversize(&self) -> bool {
        self.new_content.len() > MAX_DIFF_BYTES
            || self
                .old_content
                .as_ref()
                .is_some_and(|s| s.len() > MAX_DIFF_BYTES)
    }

    /// Create from old and new content
    pub fn from_contents(
        path: impl Into<String>,
        old: Option<String>,
        new: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            old_content: old,
            new_content: new.into(),
        }
    }
}
