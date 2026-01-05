//! Composable TUI widgets
//!
//! This module provides generic, reusable widgets that can be composed
//! to build complex UIs. Designed to eventually support both TUI and web
//! rendering through a common abstraction.
//!
//! ## Available Widgets
//!
//! - [`Popup`] - Generic selection popup with optional fuzzy filtering
//!
//! ## Adapters
//!
//! The [`adapters`] module provides [`PopupItem`] implementations for common types:
//!
//! - [`CommandItem`] - Slash commands
//! - [`AgentItem`] - Agent references
//! - [`FileItem`] - File paths
//! - [`NoteItem`] - Kiln notes
//! - [`SkillItem`] - Skills
//! - [`ChoiceItem`] - Choices for AskRequest
//!
//! ## Design Principles
//!
//! 1. **Generic over item type** - Widgets work with any type implementing the right traits
//! 2. **Decoupled from rendering** - Widgets manage state; renderers handle presentation
//! 3. **Keyboard-first** - All widgets support keyboard navigation
//! 4. **Composable** - Widgets can be nested and combined

mod adapters;
mod popup;

pub use adapters::{AgentItem, ChoiceItem, CommandItem, FileItem, NoteItem, SkillItem};
pub use popup::{
    FuzzyMatcher, GradientPopupRenderer, Popup, PopupConfig, PopupItem, PopupRenderer, PopupStyle,
    PopupViewport, ViewportBounds,
};
