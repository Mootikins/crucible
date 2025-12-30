//! Crucible Context Management
//!
//! This crate provides context management implementations for Crucible chat sessions.
//!
//! ## Components
//!
//! - [`SlidingWindowContext`]: FIFO message trimming with token budget
//! - [`LayeredPromptBuilder`]: Priority-ordered system prompt assembly
//!
//! ## Design Philosophy
//!
//! Context management is separated from LLM calls to allow:
//! - Swapping LLM backends without changing context logic
//! - Script-based context manipulation via events
//! - Composable operations for flexible policies

mod layered_prompt;
mod sliding_window;

pub use layered_prompt::LayeredPromptBuilder;
pub use sliding_window::SlidingWindowContext;

// Re-export core traits for convenience
pub use crucible_core::traits::{
    ContextError, ContextMessage, ContextOps, MessageMetadata, Position, PromptBuilder, Range,
    priorities,
};
