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
//! - **Tool types**: ToolCallInfo (the ACP client builds it), FileDiff (TurnEvent::ToolCall carries it).
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
}

/// Tool call information for streaming/display
///
/// Represents a tool call during agent execution. Used by streaming handlers
/// and UI layers to display tool activity. This is a protocol-agnostic type
/// that can be populated from ACP, MCP, or other agent protocols.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::acp::ToolCallInfo;
///
/// let tool = ToolCallInfo::new("semantic_search")
///     .with_id("call-123")
///     .with_arguments(serde_json::json!({"query": "rust async"}));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Human-readable title/description of the tool call
    pub title: String,

    /// Tool parameters/arguments as JSON
    pub arguments: Option<serde_json::Value>,

    /// Unique identifier for deduplication/updates during streaming
    pub id: Option<String>,

    /// File diffs produced by this tool call (for write operations)
    pub diffs: Vec<FileDiff>,
}

impl ToolCallInfo {
    /// Create a new tool call info with a title
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            arguments: None,
            id: None,
            diffs: Vec::new(),
        }
    }

    /// Set the tool call ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the tool arguments
    pub fn with_arguments(mut self, args: serde_json::Value) -> Self {
        self.arguments = Some(args);
        self
    }

    /// Add multiple file diffs
    pub fn with_diffs(mut self, diffs: impl IntoIterator<Item = FileDiff>) -> Self {
        self.diffs.extend(diffs);
        self
    }
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
