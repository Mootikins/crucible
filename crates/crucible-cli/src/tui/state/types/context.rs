//! Context attachment types for TUI state
//!
//! This module contains types for pending file/note context attachments.

/// Kind of context attachment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    /// External file from workspace
    File,
    /// Note from the kiln
    Note,
}

/// A context attachment pending inclusion in the next message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextAttachment {
    /// What type of context this is
    pub kind: ContextKind,
    /// Path to the file or note
    pub path: String,
    /// Display name (usually basename) for rendering chips
    pub display_name: String,
}
