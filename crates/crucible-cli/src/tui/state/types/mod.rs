//! Data type modules for TUI state
//!
//! This module organizes state-related data types by domain.

pub mod context;
pub mod popup;

// Re-export all types for convenience
pub use context::{ContextAttachment, ContextKind};
pub use popup::{PopupItem, PopupItemKind, PopupKind};
