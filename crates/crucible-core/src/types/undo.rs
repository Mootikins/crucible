//! Undo types for reverting agent turns.
//!
//! `UndoSummary` is the user-facing result of an undo operation. It describes
//! what the undo reverted.

use serde::{Deserialize, Serialize};

/// Result of a single undo operation, returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSummary {
    /// How many messages were removed from history.
    pub messages_removed: usize,
}
