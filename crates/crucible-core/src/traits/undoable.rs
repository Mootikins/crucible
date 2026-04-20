//! Capability trait for agents that support undoing past turns.
//!
//! Split off from `AgentHandle` so only agents that track conversation
//! state carry the methods. Callers dispatch via
//! [`AgentHandle::as_undoable`] / [`AgentHandle::as_undoable_mut`];
//! agents that can't undo keep the default `None` and the caller
//! surfaces a `NotSupported` error.

use async_trait::async_trait;

use super::chat::ChatResult;
use crate::types::UndoSummary;

#[async_trait]
pub trait Undoable: Send + Sync {
    /// Undo up to `count` turns. Returns one summary per turn removed.
    async fn undo(&mut self, count: usize) -> ChatResult<Vec<UndoSummary>>;

    /// Whether there are any turns that can be undone.
    fn can_undo(&self) -> bool;

    /// The number of turns that can be undone.
    fn undo_depth(&self) -> usize;
}
