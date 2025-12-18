//! Rune type bindings for Crucible.
//!
//! This module provides Rune-compatible wrappers for core Crucible types,
//! allowing them to be used in Rune scripts without adding Rune as a
//! dependency to crucible-core.

mod recipe;
mod session_event;

pub use recipe::{categorize_by_name_impl, crucible_module, RuneRecipe, RuneRecipeEnrichment};
pub use session_event::{
    module as session_event_module, RuneEventContext, RuneFileChangeKind, RuneNoteChangeType,
    RuneSessionEvent,
};
