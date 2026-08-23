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

/// Priority levels for event processing.
///
/// Events can have different priorities that affect their processing order.
/// Higher priority events are processed before lower priority events in
/// priority-aware handlers (e.g., embedding generation).
///
/// # Ordering
///
/// Priority implements `Ord` such that higher priority variants compare greater:
/// `Critical > High > Normal > Low`
///
/// # Example
///
/// ```
/// use crucible_core::events::Priority;
///
/// assert!(Priority::Critical > Priority::High);
/// assert!(Priority::High > Priority::Normal);
/// assert!(Priority::Normal > Priority::Low);
/// assert_eq!(Priority::default(), Priority::Normal);
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Priority {
    /// Low priority - background processing.
    Low = 1,
    /// Normal priority - standard processing (default).
    #[default]
    Normal = 2,
    /// High priority - user-requested operations.
    High = 3,
    /// Critical priority - system operations requiring immediate attention.
    Critical = 4,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Broad classification of a session event, used to filter events by concern.
///
/// Every event belongs to exactly one category.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    /// User/participant message.
    Message,
    /// Structured interaction request/response.
    Interaction,
    /// Note created/modified/deleted.
    Note,
    /// Raw file-system change (pre-parse).
    File,
    /// User-defined custom event.
    Custom,
    /// Anything else that doesn't fit a specific category.
    Other,
}
