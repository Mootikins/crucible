//! Undo types for reverting agent turns.
//!
//! An `UndoEntry` captures the conversation state before an agent turn so it
//! can be restored later. `UndoSummary` is the user-facing result of an undo
//! operation, describing what was reverted.

use serde::{Deserialize, Serialize};

/// Snapshot of conversation state before an agent turn.
///
/// Stored on a per-session undo stack. When the user triggers undo, the
/// conversation history is truncated back to `message_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    /// Number of messages in history before this turn started.
    pub message_index: usize,
    /// Human-readable description of what the turn did (first ~80 chars of response).
    pub description: String,
}

/// Result of a single undo operation, returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSummary {
    /// How many messages were removed from history.
    pub messages_removed: usize,
    /// Description of the reverted turn.
    pub description: String,
}
