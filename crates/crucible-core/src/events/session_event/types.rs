//! Supporting types for SessionEvent
//!
//! Includes enums and types used by session events.

use serde::{Deserialize, Serialize};

/// Type of note modification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum NoteChangeType {
    /// Content body changed.
    #[default]
    Content,
    /// Frontmatter changed.
    Frontmatter,
    /// Wikilinks changed.
    Links,
    /// Tags changed.
    Tags,
}

impl std::fmt::Display for NoteChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content => write!(f, "content"),
            Self::Frontmatter => write!(f, "frontmatter"),
            Self::Links => write!(f, "links"),
            Self::Tags => write!(f, "tags"),
        }
    }
}

/// Kind of file change detected by the watch system.
///
/// This enum represents the type of file system change that triggered an event.
/// It is used by `FileChanged` events to distinguish between new files and
/// modifications to existing files.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{SessionEvent, FileChangeKind};
/// use std::path::PathBuf;
///
/// let event = SessionEvent::FileChanged {
///     path: PathBuf::from("/notes/test.md"),
///     kind: FileChangeKind::Modified,
/// };
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum FileChangeKind {
    /// File was newly created.
    Created,
    /// Existing file was modified.
    #[default]
    Modified,
}

impl std::fmt::Display for FileChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
        }
    }
}
