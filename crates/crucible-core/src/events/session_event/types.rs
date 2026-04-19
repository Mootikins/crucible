//! Supporting types for SessionEvent
//!
//! Includes enums and types used by session events.

use serde::{Deserialize, Serialize};

/// Terminal stream identifier for output events.
///
/// Used with `TerminalOutput` events to indicate which stream the output
/// came from (stdout or stderr).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TerminalStream {
    /// Standard output stream.
    #[default]
    Stdout,
    /// Standard error stream.
    Stderr,
}

impl std::fmt::Display for TerminalStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdout => write!(f, "stdout"),
            Self::Stderr => write!(f, "stderr"),
        }
    }
}

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

/// Types of input the system can await from a human.
///
/// Used with `SessionEvent::AwaitingInput` to indicate what kind of
/// human interaction is needed before the system can proceed.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{SessionEvent, InputType};
///
/// let event = SessionEvent::AwaitingInput {
///     input_type: InputType::Approval,
///     context: Some("Agent wants to delete files".into()),
/// };
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum InputType {
    /// Waiting for the next user message (idle prompt).
    #[default]
    Message,
    /// Waiting for user approval to proceed (HIL gate).
    Approval,
    /// Waiting for user to select from options.
    Selection,
}

impl std::fmt::Display for InputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message => write!(f, "message"),
            Self::Approval => write!(f, "approval"),
            Self::Selection => write!(f, "selection"),
        }
    }
}

/// Entity types for event-driven architecture.
///
/// This enum represents the types of entities that can be stored, updated, or deleted
/// through the event system. It is used in `EntityStored`, `EntityDeleted`, and
/// related storage events.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{SessionEvent, EntityType};
///
/// let event = SessionEvent::EntityStored {
///     entity_id: "note:my-note".into(),
///     entity_type: EntityType::Note,
/// };
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum EntityType {
    /// A markdown note (the primary content type).
    #[default]
    Note,
    /// A content block within a note.
    Block,
    /// A tag used for categorization.
    Tag,
    /// A task item (from task lists or task notes).
    Task,
    /// A file containing tasks.
    TaskFile,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Note => write!(f, "note"),
            Self::Block => write!(f, "block"),
            Self::Tag => write!(f, "tag"),
            Self::Task => write!(f, "task"),
            Self::TaskFile => write!(f, "task_file"),
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

/// Provider of a discovered tool in session events.
///
/// Identifies which system provided a tool (Lua script, MCP server,
/// or built-in). This is distinct from `crucible_core::types::ToolSource` which
/// is used for tool indexing and metadata categorization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ToolProvider {
    /// Tool from a Lua/Fennel script.
    Lua,
    /// Tool from an MCP server.
    Mcp {
        /// Name of the MCP server.
        server: String,
    },
    /// Built-in system tool.
    #[default]
    Builtin,
}

impl std::fmt::Display for ToolProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lua => write!(f, "lua"),
            Self::Mcp { server } => write!(f, "mcp:{}", server),
            Self::Builtin => write!(f, "builtin"),
        }
    }
}

/// Broad classification of a session event, used to filter events by concern.
///
/// Every event belongs to exactly one category. When an event could arguably fit
/// multiple categories (e.g. an `EmbeddingStored` is both embedding-related and
/// storage-related) the more specific category wins — `Embedding` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    /// User/participant message.
    Message,
    /// Agent response or thinking.
    Agent,
    /// Tool call, completion, or discovery.
    Tool,
    /// Session lifecycle (start/end/pause/resume/compaction).
    Lifecycle,
    /// Delegation to an external agent.
    Delegation,
    /// Incremental streaming output.
    Streaming,
    /// Structured interaction request/response.
    Interaction,
    /// Pre-event interception point (fires before the corresponding action).
    Pre,
    /// Note parsed/created/modified/deleted.
    Note,
    /// Raw file-system change (pre-parse).
    File,
    /// Embedding lifecycle (request/store/fail/batch).
    Embedding,
    /// Database persistence for non-embedding entities.
    Storage,
    /// Subagent lifecycle.
    Subagent,
    /// Background (bash/task) job lifecycle.
    BackgroundTask,
    /// MCP server connection/discovery.
    Mcp,
    /// User-defined custom event.
    Custom,
    /// Anything else that doesn't fit a specific category.
    Other,
}
