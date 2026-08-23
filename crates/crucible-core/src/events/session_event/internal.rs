//! Internal session events — daemon pipeline signals that never cross the RPC wire.
//!
//! The daemon uses these events for file watching, note processing and
//! precognition. They are wrapped in `SessionEvent::Internal(Box<InternalSessionEvent>)`
//! for dispatch through the reactor/event system. `event_map::message_for`
//! projects the file and note events onto the wire; the rest stay internal.
//!
//! Inspired by Neovim's RPC model where internal events never cross the wire.
//!
//! This enum once held 38 variants. Only the seven below ever had a producer;
//! plan T3-B7 removed the rest. Add a variant when something constructs it.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{EventCategory, FileChangeKind, NoteChangeType, Priority};
use crate::text::truncate_bytes;

/// Internal session events that flow through the daemon's event system but never
/// cross the RPC wire to clients.
///
/// These are wrapped in [`SessionEvent::Internal`](super::SessionEvent::Internal)
/// for reactor dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum InternalSessionEvent {
    // ─────────────────────────────────────────────────────────────────────
    // File system events (raw file changes before parsing)
    // ─────────────────────────────────────────────────────────────────────
    /// File was changed (created or modified) on disk.
    FileChanged {
        /// Path to the changed file.
        path: PathBuf,
        /// Kind of change (created vs modified).
        kind: FileChangeKind,
    },

    /// File was deleted from disk.
    FileDeleted {
        /// Path to the deleted file.
        path: PathBuf,
    },

    /// File was moved or renamed.
    FileMoved {
        /// Original path before the move.
        from: PathBuf,
        /// New path after the move.
        to: PathBuf,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Note events (parsed note changes)
    // ─────────────────────────────────────────────────────────────────────
    /// New note was created.
    NoteCreated {
        /// Path to the new note.
        path: PathBuf,
        /// Optional title from frontmatter.
        title: Option<String>,
    },

    /// Note content was modified.
    NoteModified {
        /// Path to the modified note.
        path: PathBuf,
        /// Type of modification.
        change_type: NoteChangeType,
    },

    /// Note was deleted.
    NoteDeleted {
        /// Path to the deleted note.
        path: PathBuf,
        /// Whether the note existed before deletion.
        existed: bool,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Enrichment events
    // ─────────────────────────────────────────────────────────────────────
    /// Precognition (context enrichment) completed.
    PrecognitionComplete {
        /// Number of notes found and injected into context.
        notes_count: usize,
        /// Summary of the query used for enrichment.
        query_summary: String,
        /// Number of kilns searched during enrichment.
        kilns_searched: usize,
        /// Number of kilns filtered out by trust level.
        kilns_filtered: usize,
        /// Number of kilns that failed during search.
        kilns_failed: usize,
    },
}

impl InternalSessionEvent {
    /// Get the event type name for filtering and pattern matching.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::FileChanged { .. } => "file_changed",
            Self::FileDeleted { .. } => "file_deleted",
            Self::FileMoved { .. } => "file_moved",
            Self::NoteCreated { .. } => "note_created",
            Self::NoteModified { .. } => "note_modified",
            Self::NoteDeleted { .. } => "note_deleted",
            Self::PrecognitionComplete { .. } => {
                super::ScriptingEvent::PrecognitionComplete.as_str()
            }
        }
    }

    /// Get the PascalCase type name of this event.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FileChanged { .. } => "FileChanged",
            Self::FileDeleted { .. } => "FileDeleted",
            Self::FileMoved { .. } => "FileMoved",
            Self::NoteCreated { .. } => "NoteCreated",
            Self::NoteModified { .. } => "NoteModified",
            Self::NoteDeleted { .. } => "NoteDeleted",
            Self::PrecognitionComplete { .. } => "PrecognitionComplete",
        }
    }

    /// Get the identifier for pattern matching (path, entity id, etc.).
    pub fn identifier(&self) -> String {
        match self {
            Self::FileChanged { path, .. } => path.display().to_string(),
            Self::FileDeleted { path, .. } => path.display().to_string(),
            Self::FileMoved { to, .. } => to.display().to_string(),
            Self::NoteCreated { path, .. } => path.display().to_string(),
            Self::NoteModified { path, .. } => path.display().to_string(),
            Self::NoteDeleted { path, .. } => path.display().to_string(),
            Self::PrecognitionComplete { .. } => "precognition:complete".into(),
        }
    }

    /// Get the priority of this event.
    pub fn priority(&self) -> Priority {
        match self {
            Self::FileChanged { kind, .. } => match kind {
                FileChangeKind::Created => Priority::High,
                FileChangeKind::Modified => Priority::Normal,
            },
            Self::FileDeleted { .. } => Priority::Low,
            Self::FileMoved { .. } | Self::NoteCreated { .. } => Priority::Normal,
            Self::NoteModified { .. } | Self::NoteDeleted { .. } => Priority::Normal,
            Self::PrecognitionComplete { .. } => Priority::Normal,
        }
    }

    /// Broad classification used for filtering internal events by concern.
    pub fn category(&self) -> EventCategory {
        match self {
            Self::NoteCreated { .. } | Self::NoteModified { .. } | Self::NoteDeleted { .. } => {
                EventCategory::Note
            }
            Self::FileChanged { .. } | Self::FileDeleted { .. } | Self::FileMoved { .. } => {
                EventCategory::File
            }
            Self::PrecognitionComplete { .. } => EventCategory::Other,
        }
    }

    /// Estimate the content length for token estimation.
    pub fn estimate_content_len(&self) -> usize {
        match self {
            Self::NoteCreated { title, .. } => title.as_ref().map(|t| t.len()).unwrap_or(0) + 50,
            Self::NoteModified { .. } => 50,
            Self::NoteDeleted { .. } => 50,
            Self::FileChanged { .. } => 50,
            Self::FileDeleted { .. } => 50,
            Self::FileMoved { .. } => 50,
            Self::PrecognitionComplete {
                notes_count,
                query_summary,
                ..
            } => notes_count.to_string().len() + query_summary.len() + 50,
        }
    }

    /// Get a summary of this event's content.
    ///
    /// Free-text fields are cut to `max_len` bytes on a char boundary.
    pub fn summary(&self, max_len: usize) -> String {
        match self {
            Self::FileChanged { path, kind } => format!("path={}, kind={:?}", path.display(), kind),
            Self::FileDeleted { path } => format!("path={}", path.display()),
            Self::FileMoved { from, to } => format!("from={}, to={}", from.display(), to.display()),
            Self::NoteCreated { path, title } => {
                let t = title.as_deref().unwrap_or("(none)");
                format!(
                    "path={}, title={}",
                    path.display(),
                    truncate_bytes(t, max_len)
                )
            }
            Self::NoteModified { path, change_type } => {
                format!("path={}, change={:?}", path.display(), change_type)
            }
            Self::NoteDeleted { path, existed } => {
                format!("path={}, existed={}", path.display(), existed)
            }
            Self::PrecognitionComplete {
                notes_count,
                query_summary,
                kilns_searched,
                kilns_filtered,
                kilns_failed,
            } => {
                format!(
                    "notes={}, query={}, searched={}, filtered={}, failed={}",
                    notes_count,
                    truncate_bytes(query_summary, max_len),
                    kilns_searched,
                    kilns_filtered,
                    kilns_failed
                )
            }
        }
    }

    /// Get the detailed payload content of this event.
    pub fn payload_content(&self) -> Option<String> {
        match self {
            Self::NoteCreated { path, title } => Some(format!(
                "{}: {}",
                path.display(),
                title.as_deref().unwrap_or("(none)")
            )),
            Self::NoteModified { path, change_type } => {
                Some(format!("{}: {:?}", path.display(), change_type))
            }
            Self::NoteDeleted { path, existed } => {
                Some(format!("{}: existed={}", path.display(), existed))
            }
            Self::FileChanged { path, kind } => Some(format!("{}: {:?}", path.display(), kind)),
            Self::FileDeleted { path } => Some(path.display().to_string()),
            Self::FileMoved { from, to } => Some(format!("{} -> {}", from.display(), to.display())),
            Self::PrecognitionComplete {
                notes_count,
                query_summary,
                ..
            } => Some(format!("notes={}, query={}", notes_count, query_summary)),
        }
    }
}
