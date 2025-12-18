//! Session event types for the Crucible event system.
//!
//! This module defines the canonical `SessionEvent` enum that represents all events
//! flowing through the Crucible system. Events are categorized by their source:
//!
//! - **User events**: Messages from participants
//! - **Agent events**: Responses and thinking from AI agents
//! - **Tool events**: Tool calls and completions
//! - **Session lifecycle**: Start, compaction, end
//! - **Subagent events**: Spawning and completion of subagents
//! - **Streaming events**: Incremental text deltas
//! - **Note events**: File system changes to notes
//! - **Storage events**: Database persistence (entities, blocks, relations, embeddings)
//! - **MCP events**: Model Context Protocol server connections
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::events::{SessionEvent, NoteChangeType};
//! use std::path::PathBuf;
//!
//! let event = SessionEvent::NoteModified {
//!     path: PathBuf::from("/notes/test.md"),
//!     change_type: NoteChangeType::Content,
//! };
//!
//! assert!(event.is_note_event());
//! assert_eq!(event.event_type(), "note_modified");
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

/// Events that flow through a session.
///
/// These are the high-level session events that reactors and handlers process.
/// They integrate with the EventBus system for pub/sub delivery.
///
/// # Event Categories
///
/// Events are grouped by their source and purpose:
///
/// - **User/participant**: `MessageReceived`
/// - **Agent**: `AgentResponded`, `AgentThinking`
/// - **Tool**: `ToolCalled`, `ToolCompleted`, `ToolDiscovered`
/// - **Session lifecycle**: `SessionStarted`, `SessionCompacted`, `SessionEnded`
/// - **Subagent**: `SubagentSpawned`, `SubagentCompleted`, `SubagentFailed`
/// - **Streaming**: `TextDelta`
/// - **Note**: `NoteParsed`, `NoteCreated`, `NoteModified`
/// - **Storage**: `EntityStored`, `EntityDeleted`, `BlocksUpdated`, `RelationStored`, `RelationDeleted`, `EmbeddingStored`
/// - **MCP**: `McpAttached`
/// - **Custom**: `Custom` for extensibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    // ─────────────────────────────────────────────────────────────────────
    // User/participant events
    // ─────────────────────────────────────────────────────────────────────
    /// Message received from a participant.
    MessageReceived {
        /// The message content.
        content: String,
        /// Identifier of the participant who sent the message.
        participant_id: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Agent events
    // ─────────────────────────────────────────────────────────────────────
    /// Agent responded with content and/or tool calls.
    AgentResponded {
        /// The response content.
        content: String,
        /// Tool calls made by the agent.
        tool_calls: Vec<ToolCall>,
    },

    /// Agent is thinking (intermediate state).
    AgentThinking {
        /// The thought content.
        thought: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Tool events
    // ─────────────────────────────────────────────────────────────────────
    /// Tool was called.
    ToolCalled {
        /// Name of the tool being called.
        name: String,
        /// Arguments passed to the tool.
        args: JsonValue,
    },

    /// Tool execution completed.
    ToolCompleted {
        /// Name of the tool that completed.
        name: String,
        /// Result of the tool execution.
        result: String,
        /// Error message if the tool failed.
        error: Option<String>,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Session lifecycle
    // ─────────────────────────────────────────────────────────────────────
    /// Session started with configuration.
    SessionStarted {
        /// Session configuration.
        config: SessionEventConfig,
    },

    /// Session context was compacted.
    SessionCompacted {
        /// Summary of the compacted context.
        summary: String,
        /// Path to the new context file.
        new_file: PathBuf,
    },

    /// Session ended.
    SessionEnded {
        /// Reason for ending the session.
        reason: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Subagent events
    // ─────────────────────────────────────────────────────────────────────
    /// Subagent was spawned.
    SubagentSpawned {
        /// Unique identifier for the subagent.
        id: String,
        /// Prompt given to the subagent.
        prompt: String,
    },

    /// Subagent completed successfully.
    SubagentCompleted {
        /// Identifier of the completed subagent.
        id: String,
        /// Result from the subagent.
        result: String,
    },

    /// Subagent failed.
    SubagentFailed {
        /// Identifier of the failed subagent.
        id: String,
        /// Error message.
        error: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Streaming events
    // ─────────────────────────────────────────────────────────────────────
    /// Incremental text delta from agent (for streaming responses).
    TextDelta {
        /// The text chunk.
        delta: String,
        /// Sequence number for ordering.
        seq: u64,
    },

    // ─────────────────────────────────────────────────────────────────────
    // File system events (raw file changes before parsing)
    // ─────────────────────────────────────────────────────────────────────
    /// File was changed (created or modified) on disk.
    ///
    /// This is a raw file system event emitted before parsing. Use this to
    /// trigger downstream processing like parsing, indexing, or embedding.
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
    /// Note was parsed (AST available).
    ///
    /// This event is emitted after a markdown note has been parsed. It includes
    /// basic metadata (path, block count) and optionally a full [`NotePayload`]
    /// containing extracted information like tags, wikilinks, and frontmatter.
    NoteParsed {
        /// Path to the parsed note.
        path: PathBuf,
        /// Number of parsed blocks.
        block_count: usize,
        /// Optional payload with full parsed note data.
        ///
        /// When present, contains tags, wikilinks, frontmatter, and other
        /// extracted information. May be omitted for lightweight events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<NotePayload>,
    },

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

    // ─────────────────────────────────────────────────────────────────────
    // Storage events (database persistence)
    // ─────────────────────────────────────────────────────────────────────
    /// Entity was stored/upserted to the database.
    ///
    /// Emitted when a note, block, tag, or other entity is persisted.
    /// This event is typically emitted after parsing completes.
    EntityStored {
        /// The entity identifier (e.g., "entities:note:my-note").
        entity_id: String,
        /// Type of entity that was stored.
        entity_type: EntityType,
    },

    /// Entity was deleted from the database.
    EntityDeleted {
        /// The entity identifier that was deleted.
        entity_id: String,
        /// Type of entity that was deleted.
        entity_type: EntityType,
    },

    /// Blocks for an entity were updated.
    ///
    /// Emitted when content blocks (paragraphs, headings, code blocks, etc.)
    /// are replaced or modified for a note.
    BlocksUpdated {
        /// The parent entity identifier.
        entity_id: String,
        /// Number of blocks after the update.
        block_count: usize,
    },

    /// A relation between entities was stored.
    ///
    /// Relations represent links between entities (wikilinks, tags, etc.).
    RelationStored {
        /// Source entity identifier.
        from_id: String,
        /// Target entity identifier.
        to_id: String,
        /// Type of relation (e.g., "wikilink", "tag", "backlink").
        relation_type: String,
    },

    /// A relation between entities was deleted.
    RelationDeleted {
        /// Source entity identifier.
        from_id: String,
        /// Target entity identifier.
        to_id: String,
        /// Type of relation that was deleted.
        relation_type: String,
    },

    /// A tag was associated with an entity.
    ///
    /// Emitted when a tag is linked to a note or other entity.
    /// Multiple TagAssociated events may be emitted for a single note
    /// if it has multiple tags.
    TagAssociated {
        /// The entity identifier the tag is associated with.
        entity_id: String,
        /// The tag name (without the # prefix).
        tag: String,
    },

    /// Embedding generation was requested.
    ///
    /// Emitted when embedding generation is queued for an entity or block.
    /// This triggers the embedding pipeline to process the content.
    EmbeddingRequested {
        /// The entity identifier to generate embeddings for.
        entity_id: String,
        /// Optional block identifier for block-level embeddings.
        block_id: Option<String>,
        /// Priority of the request (affects queue ordering).
        priority: Priority,
    },

    /// An embedding vector was stored.
    ///
    /// Emitted when embedding generation completes and the vector is persisted.
    EmbeddingStored {
        /// The entity identifier the embedding belongs to.
        entity_id: String,
        /// Optional block identifier for block-level embeddings.
        block_id: Option<String>,
        /// Dimensions of the embedding vector.
        dimensions: usize,
        /// Model used to generate the embedding.
        model: String,
    },

    /// Embedding generation failed.
    ///
    /// Emitted when embedding generation fails for an entity or block.
    /// The error message contains details about what went wrong.
    EmbeddingFailed {
        /// The entity identifier for which embedding failed.
        entity_id: String,
        /// Optional block identifier for block-level embeddings.
        block_id: Option<String>,
        /// Error message describing the failure.
        error: String,
    },

    /// Batch of embeddings completed for an entity.
    ///
    /// Emitted when a batch of embedding generations completes for an entity.
    /// This is useful for tracking overall progress and performance metrics.
    EmbeddingBatchComplete {
        /// The entity identifier for which embeddings were generated.
        entity_id: String,
        /// Number of embeddings generated in this batch.
        count: usize,
        /// Duration of the batch processing in milliseconds.
        duration_ms: u64,
    },

    // ─────────────────────────────────────────────────────────────────────
    // MCP/Tool discovery events
    // ─────────────────────────────────────────────────────────────────────
    /// Upstream MCP server connected.
    McpAttached {
        /// Name of the MCP server.
        server: String,
        /// Number of tools provided by the server.
        tool_count: usize,
    },

    /// New tool discovered (can be filtered).
    ToolDiscovered {
        /// Name of the discovered tool.
        name: String,
        /// Source of the tool.
        source: ToolSource,
        /// Optional JSON schema for the tool's arguments.
        schema: Option<JsonValue>,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Custom events
    // ─────────────────────────────────────────────────────────────────────
    /// Custom event for extensibility.
    Custom {
        /// Name/identifier of the custom event.
        name: String,
        /// Arbitrary payload.
        payload: JsonValue,
    },
}

impl SessionEvent {
    /// Get the event type name for filtering and pattern matching.
    ///
    /// Returns a stable string identifier that can be used for:
    /// - Handler registration (e.g., `bus.on("tool_called", ...)`)
    /// - Event filtering in queries
    /// - Logging and debugging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::MessageReceived { .. } => "message_received",
            Self::AgentResponded { .. } => "agent_responded",
            Self::AgentThinking { .. } => "agent_thinking",
            Self::ToolCalled { .. } => "tool_called",
            Self::ToolCompleted { .. } => "tool_completed",
            Self::SessionStarted { .. } => "session_started",
            Self::SessionCompacted { .. } => "session_compacted",
            Self::SessionEnded { .. } => "session_ended",
            Self::SubagentSpawned { .. } => "subagent_spawned",
            Self::SubagentCompleted { .. } => "subagent_completed",
            Self::SubagentFailed { .. } => "subagent_failed",
            Self::TextDelta { .. } => "text_delta",
            Self::FileChanged { .. } => "file_changed",
            Self::FileDeleted { .. } => "file_deleted",
            Self::FileMoved { .. } => "file_moved",
            Self::NoteParsed { .. } => "note_parsed",
            Self::NoteCreated { .. } => "note_created",
            Self::NoteModified { .. } => "note_modified",
            Self::EntityStored { .. } => "entity_stored",
            Self::EntityDeleted { .. } => "entity_deleted",
            Self::BlocksUpdated { .. } => "blocks_updated",
            Self::RelationStored { .. } => "relation_stored",
            Self::RelationDeleted { .. } => "relation_deleted",
            Self::TagAssociated { .. } => "tag_associated",
            Self::EmbeddingRequested { .. } => "embedding_requested",
            Self::EmbeddingStored { .. } => "embedding_stored",
            Self::EmbeddingFailed { .. } => "embedding_failed",
            Self::EmbeddingBatchComplete { .. } => "embedding_batch_complete",
            Self::McpAttached { .. } => "mcp_attached",
            Self::ToolDiscovered { .. } => "tool_discovered",
            Self::Custom { .. } => "custom",
        }
    }

    /// Get the identifier for pattern matching (tool name, note path, etc.).
    ///
    /// This is used by the EventBus for glob pattern matching against handlers.
    pub fn identifier(&self) -> String {
        match self {
            Self::MessageReceived { participant_id, .. } => format!("message:{}", participant_id),
            Self::AgentResponded { .. } => "agent:responded".into(),
            Self::AgentThinking { .. } => "agent:thinking".into(),
            Self::ToolCalled { name, .. } => name.clone(),
            Self::ToolCompleted { name, .. } => name.clone(),
            Self::SessionStarted { config, .. } => format!("session:{}", config.session_id),
            Self::SessionCompacted { .. } => "session:compacted".into(),
            Self::SessionEnded { .. } => "session:ended".into(),
            Self::SubagentSpawned { id, .. } => format!("subagent:spawned:{}", id),
            Self::SubagentCompleted { id, .. } => format!("subagent:completed:{}", id),
            Self::SubagentFailed { id, .. } => format!("subagent:failed:{}", id),
            Self::TextDelta { seq, .. } => format!("streaming:delta:{}", seq),
            Self::FileChanged { path, .. } => path.display().to_string(),
            Self::FileDeleted { path, .. } => path.display().to_string(),
            Self::FileMoved { to, .. } => to.display().to_string(),
            Self::NoteParsed { path, .. } => path.display().to_string(),
            Self::NoteCreated { path, .. } => path.display().to_string(),
            Self::NoteModified { path, .. } => path.display().to_string(),
            Self::EntityStored { entity_id, .. } => entity_id.clone(),
            Self::EntityDeleted { entity_id, .. } => entity_id.clone(),
            Self::BlocksUpdated { entity_id, .. } => entity_id.clone(),
            Self::RelationStored { from_id, to_id, .. } => format!("{}:{}", from_id, to_id),
            Self::RelationDeleted { from_id, to_id, .. } => format!("{}:{}", from_id, to_id),
            Self::TagAssociated { entity_id, tag } => format!("{}#{}", entity_id, tag),
            Self::EmbeddingRequested {
                entity_id,
                block_id,
                ..
            } => {
                if let Some(block) = block_id {
                    format!("{}#{}", entity_id, block)
                } else {
                    entity_id.clone()
                }
            }
            Self::EmbeddingStored {
                entity_id,
                block_id,
                ..
            } => {
                if let Some(block) = block_id {
                    format!("{}#{}", entity_id, block)
                } else {
                    entity_id.clone()
                }
            }
            Self::EmbeddingFailed {
                entity_id,
                block_id,
                ..
            } => {
                if let Some(block) = block_id {
                    format!("{}#{}", entity_id, block)
                } else {
                    entity_id.clone()
                }
            }
            Self::EmbeddingBatchComplete { entity_id, .. } => entity_id.clone(),
            Self::McpAttached { server, .. } => server.clone(),
            Self::ToolDiscovered { name, .. } => name.clone(),
            Self::Custom { name, .. } => name.clone(),
        }
    }

    /// Check if this is a tool-related event.
    pub fn is_tool_event(&self) -> bool {
        matches!(
            self,
            Self::ToolCalled { .. } | Self::ToolCompleted { .. } | Self::ToolDiscovered { .. }
        )
    }

    /// Check if this is a note-related event.
    pub fn is_note_event(&self) -> bool {
        matches!(
            self,
            Self::NoteParsed { .. } | Self::NoteCreated { .. } | Self::NoteModified { .. }
        )
    }

    /// Check if this is a session lifecycle event.
    pub fn is_lifecycle_event(&self) -> bool {
        matches!(
            self,
            Self::SessionStarted { .. } | Self::SessionCompacted { .. } | Self::SessionEnded { .. }
        )
    }

    /// Check if this is an agent-related event.
    pub fn is_agent_event(&self) -> bool {
        matches!(
            self,
            Self::AgentResponded { .. } | Self::AgentThinking { .. }
        )
    }

    /// Check if this is a subagent-related event.
    pub fn is_subagent_event(&self) -> bool {
        matches!(
            self,
            Self::SubagentSpawned { .. }
                | Self::SubagentCompleted { .. }
                | Self::SubagentFailed { .. }
        )
    }

    /// Check if this is a streaming event.
    pub fn is_streaming_event(&self) -> bool {
        matches!(self, Self::TextDelta { .. })
    }

    /// Check if this is a file system event (raw file changes).
    ///
    /// File events represent raw file system changes before parsing.
    /// They are distinct from note events which represent parsed content.
    pub fn is_file_event(&self) -> bool {
        matches!(
            self,
            Self::FileChanged { .. } | Self::FileDeleted { .. } | Self::FileMoved { .. }
        )
    }

    /// Check if this is an embedding-related event.
    ///
    /// Embedding events track the lifecycle of embedding generation:
    /// request, success (stored), or failure.
    pub fn is_embedding_event(&self) -> bool {
        matches!(
            self,
            Self::EmbeddingRequested { .. }
                | Self::EmbeddingStored { .. }
                | Self::EmbeddingFailed { .. }
                | Self::EmbeddingBatchComplete { .. }
        )
    }

    /// Check if this is a storage event (database operations).
    ///
    /// Storage events represent persistence operations to the database.
    /// They are emitted after entities, blocks, relations, tags, or embeddings
    /// are stored or deleted.
    pub fn is_storage_event(&self) -> bool {
        matches!(
            self,
            Self::EntityStored { .. }
                | Self::EntityDeleted { .. }
                | Self::BlocksUpdated { .. }
                | Self::RelationStored { .. }
                | Self::RelationDeleted { .. }
                | Self::TagAssociated { .. }
                | Self::EmbeddingRequested { .. }
                | Self::EmbeddingStored { .. }
                | Self::EmbeddingFailed { .. }
                | Self::EmbeddingBatchComplete { .. }
        )
    }

    /// Check if this is an MCP-related event.
    pub fn is_mcp_event(&self) -> bool {
        matches!(self, Self::McpAttached { .. })
    }

    /// Check if this is a custom event.
    pub fn is_custom_event(&self) -> bool {
        matches!(self, Self::Custom { .. })
    }

    /// Get the priority of this event.
    ///
    /// Priority affects processing order in priority-aware handlers.
    /// Higher priority events are processed before lower priority events.
    ///
    /// # Priority Mapping
    ///
    /// - `FileChanged(Created)` → High (new files should be indexed promptly)
    /// - `FileChanged(Modified)` → Normal (standard processing)
    /// - `FileDeleted` → Low (cleanup can wait)
    /// - `EmbeddingRequested` → uses the embedded priority field
    /// - All other events → Normal (default priority)
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::{SessionEvent, FileChangeKind, Priority};
    /// use std::path::PathBuf;
    ///
    /// let created = SessionEvent::FileChanged {
    ///     path: PathBuf::from("/notes/new.md"),
    ///     kind: FileChangeKind::Created,
    /// };
    /// assert_eq!(created.priority(), Priority::High);
    ///
    /// let deleted = SessionEvent::FileDeleted {
    ///     path: PathBuf::from("/notes/old.md"),
    /// };
    /// assert_eq!(deleted.priority(), Priority::Low);
    /// ```
    pub fn priority(&self) -> Priority {
        match self {
            Self::FileChanged { kind, .. } => match kind {
                FileChangeKind::Created => Priority::High,
                FileChangeKind::Modified => Priority::Normal,
            },
            Self::FileDeleted { .. } => Priority::Low,
            Self::FileMoved { .. } => Priority::Normal,
            Self::EmbeddingRequested { priority, .. } => *priority,
            // All other events default to Normal priority
            _ => Priority::Normal,
        }
    }
}

impl Default for SessionEvent {
    fn default() -> Self {
        Self::Custom {
            name: "default".into(),
            payload: JsonValue::Null,
        }
    }
}

/// Type of note modification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NoteChangeType {
    /// Content body changed.
    Content,
    /// Frontmatter changed.
    Frontmatter,
    /// Wikilinks changed.
    Links,
    /// Tags changed.
    Tags,
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
pub enum FileChangeKind {
    /// File was newly created.
    Created,
    /// Existing file was modified.
    Modified,
}

impl Default for FileChangeKind {
    fn default() -> Self {
        Self::Modified
    }
}

impl std::fmt::Display for FileChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
        }
    }
}

impl Default for NoteChangeType {
    fn default() -> Self {
        Self::Content
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
pub enum EntityType {
    /// A markdown note (the primary content type).
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
pub enum Priority {
    /// Low priority - background processing.
    Low = 1,
    /// Normal priority - standard processing (default).
    Normal = 2,
    /// High priority - user-requested operations.
    High = 3,
    /// Critical priority - system operations requiring immediate attention.
    Critical = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
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

/// Payload for note events containing parsed note data.
///
/// This is a simplified payload for event transmission. It captures the essential
/// information extracted from a parsed note without the full AST representation.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{SessionEvent, NotePayload};
/// use std::path::PathBuf;
///
/// let payload = NotePayload::new("notes/test.md", "Test Note")
///     .with_tags(vec!["rust".into(), "test".into()])
///     .with_wikilinks(vec!["other-note".into()]);
///
/// let event = SessionEvent::NoteParsed {
///     path: PathBuf::from("notes/test.md"),
///     block_count: 5,
///     payload: Some(payload),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotePayload {
    /// Note path (relative to kiln root).
    pub path: String,

    /// Title (from frontmatter or filename).
    pub title: String,

    /// Frontmatter as JSON value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<JsonValue>,

    /// Tags extracted from content and frontmatter.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Wikilink targets found in the note.
    #[serde(default)]
    pub wikilinks: Vec<String>,

    /// Content hash for change detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// File size in bytes.
    #[serde(default)]
    pub file_size: u64,

    /// Word count of the content.
    #[serde(default)]
    pub word_count: usize,
}

impl NotePayload {
    /// Create a new payload with required fields.
    pub fn new(path: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            title: title.into(),
            frontmatter: None,
            tags: Vec::new(),
            wikilinks: Vec::new(),
            content_hash: None,
            file_size: 0,
            word_count: 0,
        }
    }

    /// Set frontmatter JSON value.
    pub fn with_frontmatter(mut self, frontmatter: JsonValue) -> Self {
        self.frontmatter = Some(frontmatter);
        self
    }

    /// Set tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set wikilink targets.
    pub fn with_wikilinks(mut self, wikilinks: Vec<String>) -> Self {
        self.wikilinks = wikilinks;
        self
    }

    /// Set content hash.
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    /// Set file size.
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = size;
        self
    }

    /// Set word count.
    pub fn with_word_count(mut self, count: usize) -> Self {
        self.word_count = count;
        self
    }
}

impl Default for NotePayload {
    fn default() -> Self {
        Self::new("", "")
    }
}

impl Default for EntityType {
    fn default() -> Self {
        Self::Note
    }
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

/// Source of a discovered tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// Built-in Rune tool.
    Rune,
    /// Tool from an MCP server.
    Mcp {
        /// Name of the MCP server.
        server: String,
    },
    /// Built-in system tool.
    Builtin,
}

impl Default for ToolSource {
    fn default() -> Self {
        Self::Builtin
    }
}

impl std::fmt::Display for ToolSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rune => write!(f, "rune"),
            Self::Mcp { server } => write!(f, "mcp:{}", server),
            Self::Builtin => write!(f, "builtin"),
        }
    }
}

/// A tool call made by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Tool name.
    pub name: String,
    /// Tool arguments as JSON.
    pub args: JsonValue,
    /// Optional call ID for correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(name: impl Into<String>, args: JsonValue) -> Self {
        Self {
            name: name.into(),
            args,
            call_id: None,
        }
    }

    /// Set the call ID.
    pub fn with_call_id(mut self, id: impl Into<String>) -> Self {
        self.call_id = Some(id.into());
        self
    }
}

impl Default for ToolCall {
    fn default() -> Self {
        Self {
            name: String::new(),
            args: JsonValue::Null,
            call_id: None,
        }
    }
}

/// Session configuration for SessionStarted events.
///
/// This is a simplified version of session config for event serialization.
/// The full configuration is in `crucible-rune::reactor::SessionConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionEventConfig {
    /// Unique session identifier.
    pub session_id: String,
    /// Session folder path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// Maximum context tokens before compaction.
    #[serde(default)]
    pub max_context_tokens: usize,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

impl SessionEventConfig {
    /// Create a new session config with the given ID.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: None,
            max_context_tokens: 100_000,
            system_prompt: None,
        }
    }

    /// Set the folder path.
    pub fn with_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.folder = Some(folder.into());
        self
    }

    /// Set the maximum context tokens.
    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_event_type() {
        assert_eq!(
            SessionEvent::MessageReceived {
                content: "".into(),
                participant_id: "".into()
            }
            .event_type(),
            "message_received"
        );
        assert_eq!(
            SessionEvent::ToolCalled {
                name: "".into(),
                args: JsonValue::Null
            }
            .event_type(),
            "tool_called"
        );
        assert_eq!(
            SessionEvent::FileChanged {
                path: PathBuf::new(),
                kind: FileChangeKind::Created
            }
            .event_type(),
            "file_changed"
        );
        assert_eq!(
            SessionEvent::FileDeleted {
                path: PathBuf::new()
            }
            .event_type(),
            "file_deleted"
        );
        assert_eq!(
            SessionEvent::FileMoved {
                from: PathBuf::new(),
                to: PathBuf::new()
            }
            .event_type(),
            "file_moved"
        );
        assert_eq!(
            SessionEvent::NoteParsed {
                path: PathBuf::new(),
                block_count: 0,
                payload: None,
            }
            .event_type(),
            "note_parsed"
        );
        // Storage events
        assert_eq!(
            SessionEvent::EntityStored {
                entity_id: "".into(),
                entity_type: EntityType::Note
            }
            .event_type(),
            "entity_stored"
        );
        assert_eq!(
            SessionEvent::EntityDeleted {
                entity_id: "".into(),
                entity_type: EntityType::Note
            }
            .event_type(),
            "entity_deleted"
        );
        assert_eq!(
            SessionEvent::BlocksUpdated {
                entity_id: "".into(),
                block_count: 0
            }
            .event_type(),
            "blocks_updated"
        );
        assert_eq!(
            SessionEvent::RelationStored {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            }
            .event_type(),
            "relation_stored"
        );
        assert_eq!(
            SessionEvent::RelationDeleted {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            }
            .event_type(),
            "relation_deleted"
        );
        assert_eq!(
            SessionEvent::EmbeddingRequested {
                entity_id: "".into(),
                block_id: None,
                priority: Priority::Normal
            }
            .event_type(),
            "embedding_requested"
        );
        assert_eq!(
            SessionEvent::EmbeddingStored {
                entity_id: "".into(),
                block_id: None,
                dimensions: 0,
                model: "".into()
            }
            .event_type(),
            "embedding_stored"
        );
        assert_eq!(
            SessionEvent::EmbeddingFailed {
                entity_id: "".into(),
                block_id: None,
                error: "".into()
            }
            .event_type(),
            "embedding_failed"
        );
        assert_eq!(
            SessionEvent::Custom {
                name: "test".into(),
                payload: JsonValue::Null
            }
            .event_type(),
            "custom"
        );
    }

    #[test]
    fn test_session_event_identifier() {
        let event = SessionEvent::ToolCalled {
            name: "search".into(),
            args: JsonValue::Null,
        };
        assert_eq!(event.identifier(), "search");

        let event = SessionEvent::NoteParsed {
            path: PathBuf::from("/notes/test.md"),
            block_count: 5,
            payload: None,
        };
        assert_eq!(event.identifier(), "/notes/test.md");

        let event = SessionEvent::MessageReceived {
            content: "hello".into(),
            participant_id: "user".into(),
        };
        assert_eq!(event.identifier(), "message:user");

        // File events identifiers
        let event = SessionEvent::FileChanged {
            path: PathBuf::from("/notes/changed.md"),
            kind: FileChangeKind::Modified,
        };
        assert_eq!(event.identifier(), "/notes/changed.md");

        let event = SessionEvent::FileDeleted {
            path: PathBuf::from("/notes/deleted.md"),
        };
        assert_eq!(event.identifier(), "/notes/deleted.md");

        // FileMoved uses the "to" path as identifier
        let event = SessionEvent::FileMoved {
            from: PathBuf::from("/notes/old.md"),
            to: PathBuf::from("/notes/new.md"),
        };
        assert_eq!(event.identifier(), "/notes/new.md");

        // Storage events identifiers
        let event = SessionEvent::EntityStored {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        };
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::EntityDeleted {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        };
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::BlocksUpdated {
            entity_id: "entities:note:test".into(),
            block_count: 5,
        };
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::RelationStored {
            from_id: "entities:note:a".into(),
            to_id: "entities:note:b".into(),
            relation_type: "wikilink".into(),
        };
        assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

        let event = SessionEvent::RelationDeleted {
            from_id: "entities:note:a".into(),
            to_id: "entities:note:b".into(),
            relation_type: "wikilink".into(),
        };
        assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

        // EmbeddingRequested with block_id
        let event = SessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            priority: Priority::High,
        };
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingRequested without block_id
        let event = SessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: None,
            priority: Priority::Normal,
        };
        assert_eq!(event.identifier(), "entities:note:test");

        // EmbeddingStored with block_id
        let event = SessionEvent::EmbeddingStored {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            dimensions: 384,
            model: "nomic-embed-text".into(),
        };
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingStored without block_id
        let event = SessionEvent::EmbeddingStored {
            entity_id: "entities:note:test".into(),
            block_id: None,
            dimensions: 384,
            model: "nomic-embed-text".into(),
        };
        assert_eq!(event.identifier(), "entities:note:test");

        // EmbeddingFailed with block_id
        let event = SessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            error: "provider timeout".into(),
        };
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingFailed without block_id
        let event = SessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: None,
            error: "provider timeout".into(),
        };
        assert_eq!(event.identifier(), "entities:note:test");
    }

    #[test]
    fn test_session_event_category_helpers() {
        // Tool events
        assert!(SessionEvent::ToolCalled {
            name: "".into(),
            args: JsonValue::Null
        }
        .is_tool_event());
        assert!(SessionEvent::ToolCompleted {
            name: "".into(),
            result: "".into(),
            error: None
        }
        .is_tool_event());
        assert!(!SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "".into()
        }
        .is_tool_event());

        // Note events
        assert!(SessionEvent::NoteParsed {
            path: PathBuf::new(),
            block_count: 0,
            payload: None,
        }
        .is_note_event());
        assert!(SessionEvent::NoteCreated {
            path: PathBuf::new(),
            title: None
        }
        .is_note_event());
        assert!(!SessionEvent::ToolCalled {
            name: "".into(),
            args: JsonValue::Null
        }
        .is_note_event());

        // Lifecycle events
        assert!(SessionEvent::SessionStarted {
            config: SessionEventConfig::default()
        }
        .is_lifecycle_event());
        assert!(SessionEvent::SessionEnded { reason: "".into() }.is_lifecycle_event());

        // Agent events
        assert!(SessionEvent::AgentResponded {
            content: "".into(),
            tool_calls: vec![]
        }
        .is_agent_event());
        assert!(SessionEvent::AgentThinking { thought: "".into() }.is_agent_event());

        // Subagent events
        assert!(SessionEvent::SubagentSpawned {
            id: "".into(),
            prompt: "".into()
        }
        .is_subagent_event());

        // Streaming events
        assert!(SessionEvent::TextDelta {
            delta: "".into(),
            seq: 0
        }
        .is_streaming_event());

        // File events
        assert!(SessionEvent::FileChanged {
            path: PathBuf::new(),
            kind: FileChangeKind::Created
        }
        .is_file_event());
        assert!(SessionEvent::FileDeleted {
            path: PathBuf::new()
        }
        .is_file_event());
        assert!(SessionEvent::FileMoved {
            from: PathBuf::new(),
            to: PathBuf::new()
        }
        .is_file_event());
        // File events are not note events
        assert!(!SessionEvent::FileChanged {
            path: PathBuf::new(),
            kind: FileChangeKind::Modified
        }
        .is_note_event());

        // Embedding events
        assert!(SessionEvent::EmbeddingRequested {
            entity_id: "".into(),
            block_id: None,
            priority: Priority::Normal
        }
        .is_embedding_event());
        assert!(SessionEvent::EmbeddingStored {
            entity_id: "".into(),
            block_id: None,
            dimensions: 0,
            model: "".into()
        }
        .is_embedding_event());
        assert!(SessionEvent::EmbeddingFailed {
            entity_id: "".into(),
            block_id: None,
            error: "".into()
        }
        .is_embedding_event());
        assert!(SessionEvent::EmbeddingBatchComplete {
            entity_id: "".into(),
            count: 5,
            duration_ms: 100
        }
        .is_embedding_event());
        // Non-embedding events
        assert!(!SessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        }
        .is_embedding_event());

        // Storage events
        assert!(SessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        }
        .is_storage_event());
        assert!(SessionEvent::EntityDeleted {
            entity_id: "".into(),
            entity_type: EntityType::Note
        }
        .is_storage_event());
        assert!(SessionEvent::BlocksUpdated {
            entity_id: "".into(),
            block_count: 0
        }
        .is_storage_event());
        assert!(SessionEvent::RelationStored {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        }
        .is_storage_event());
        assert!(SessionEvent::RelationDeleted {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        }
        .is_storage_event());
        assert!(SessionEvent::EmbeddingRequested {
            entity_id: "".into(),
            block_id: None,
            priority: Priority::Normal
        }
        .is_storage_event());
        assert!(SessionEvent::EmbeddingStored {
            entity_id: "".into(),
            block_id: None,
            dimensions: 0,
            model: "".into()
        }
        .is_storage_event());
        assert!(SessionEvent::EmbeddingFailed {
            entity_id: "".into(),
            block_id: None,
            error: "".into()
        }
        .is_storage_event());
        assert!(SessionEvent::EmbeddingBatchComplete {
            entity_id: "".into(),
            count: 5,
            duration_ms: 100
        }
        .is_storage_event());
        // Storage events are not note events
        assert!(!SessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        }
        .is_note_event());

        // MCP events
        assert!(SessionEvent::McpAttached {
            server: "".into(),
            tool_count: 0
        }
        .is_mcp_event());

        // Custom events
        assert!(SessionEvent::Custom {
            name: "".into(),
            payload: JsonValue::Null
        }
        .is_custom_event());
    }

    #[test]
    fn test_session_event_serialization() {
        let event = SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("message_received"));
        assert!(json.contains("Hello"));

        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_all_variants_serialize() {
        let events = vec![
            SessionEvent::MessageReceived {
                content: "test".into(),
                participant_id: "user".into(),
            },
            SessionEvent::AgentResponded {
                content: "response".into(),
                tool_calls: vec![],
            },
            SessionEvent::AgentThinking {
                thought: "thinking".into(),
            },
            SessionEvent::ToolCalled {
                name: "tool".into(),
                args: serde_json::json!({}),
            },
            SessionEvent::ToolCompleted {
                name: "tool".into(),
                result: "done".into(),
                error: None,
            },
            SessionEvent::SessionStarted {
                config: SessionEventConfig::default(),
            },
            SessionEvent::SessionCompacted {
                summary: "summary".into(),
                new_file: PathBuf::from("/tmp/new"),
            },
            SessionEvent::SessionEnded {
                reason: "user closed".into(),
            },
            SessionEvent::SubagentSpawned {
                id: "sub1".into(),
                prompt: "do stuff".into(),
            },
            SessionEvent::SubagentCompleted {
                id: "sub1".into(),
                result: "done".into(),
            },
            SessionEvent::SubagentFailed {
                id: "sub1".into(),
                error: "failed".into(),
            },
            SessionEvent::TextDelta {
                delta: "chunk".into(),
                seq: 1,
            },
            // File system events
            SessionEvent::FileChanged {
                path: PathBuf::from("/notes/test.md"),
                kind: FileChangeKind::Created,
            },
            SessionEvent::FileChanged {
                path: PathBuf::from("/notes/test.md"),
                kind: FileChangeKind::Modified,
            },
            SessionEvent::FileDeleted {
                path: PathBuf::from("/notes/deleted.md"),
            },
            SessionEvent::FileMoved {
                from: PathBuf::from("/notes/old.md"),
                to: PathBuf::from("/notes/new.md"),
            },
            // Note events
            SessionEvent::NoteParsed {
                path: PathBuf::from("/notes/test.md"),
                block_count: 5,
                payload: None,
            },
            SessionEvent::NoteCreated {
                path: PathBuf::from("/notes/new.md"),
                title: Some("New Note".into()),
            },
            SessionEvent::NoteModified {
                path: PathBuf::from("/notes/test.md"),
                change_type: NoteChangeType::Content,
            },
            // Storage events
            SessionEvent::EntityStored {
                entity_id: "entities:note:test".into(),
                entity_type: EntityType::Note,
            },
            SessionEvent::EntityDeleted {
                entity_id: "entities:note:test".into(),
                entity_type: EntityType::Note,
            },
            SessionEvent::BlocksUpdated {
                entity_id: "entities:note:test".into(),
                block_count: 5,
            },
            SessionEvent::RelationStored {
                from_id: "entities:note:source".into(),
                to_id: "entities:note:target".into(),
                relation_type: "wikilink".into(),
            },
            SessionEvent::RelationDeleted {
                from_id: "entities:note:source".into(),
                to_id: "entities:note:target".into(),
                relation_type: "wikilink".into(),
            },
            SessionEvent::EmbeddingRequested {
                entity_id: "entities:note:test".into(),
                block_id: None,
                priority: Priority::Normal,
            },
            SessionEvent::EmbeddingRequested {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                priority: Priority::High,
            },
            SessionEvent::EmbeddingStored {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                dimensions: 384,
                model: "nomic-embed-text".into(),
            },
            SessionEvent::EmbeddingFailed {
                entity_id: "entities:note:test".into(),
                block_id: None,
                error: "provider timeout".into(),
            },
            SessionEvent::EmbeddingFailed {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                error: "rate limited".into(),
            },
            SessionEvent::McpAttached {
                server: "crucible".into(),
                tool_count: 10,
            },
            SessionEvent::ToolDiscovered {
                name: "search".into(),
                source: ToolSource::Mcp {
                    server: "crucible".into(),
                },
                schema: Some(serde_json::json!({"type": "object"})),
            },
            SessionEvent::Custom {
                name: "custom".into(),
                payload: serde_json::json!({}),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    #[test]
    fn test_note_change_type() {
        assert_eq!(NoteChangeType::default(), NoteChangeType::Content);
        assert_eq!(format!("{}", NoteChangeType::Content), "content");
        assert_eq!(format!("{}", NoteChangeType::Frontmatter), "frontmatter");
        assert_eq!(format!("{}", NoteChangeType::Links), "links");
        assert_eq!(format!("{}", NoteChangeType::Tags), "tags");
    }

    #[test]
    fn test_file_change_kind() {
        // Test default
        assert_eq!(FileChangeKind::default(), FileChangeKind::Modified);

        // Test Display
        assert_eq!(format!("{}", FileChangeKind::Created), "created");
        assert_eq!(format!("{}", FileChangeKind::Modified), "modified");

        // Test serialization
        let created = FileChangeKind::Created;
        let json = serde_json::to_string(&created).unwrap();
        assert_eq!(json, "\"created\"");

        let modified = FileChangeKind::Modified;
        let json = serde_json::to_string(&modified).unwrap();
        assert_eq!(json, "\"modified\"");

        // Test deserialization
        let created: FileChangeKind = serde_json::from_str("\"created\"").unwrap();
        assert_eq!(created, FileChangeKind::Created);

        let modified: FileChangeKind = serde_json::from_str("\"modified\"").unwrap();
        assert_eq!(modified, FileChangeKind::Modified);

        // Test equality and hashing
        assert_eq!(FileChangeKind::Created, FileChangeKind::Created);
        assert_ne!(FileChangeKind::Created, FileChangeKind::Modified);

        // Test Clone and Copy
        let kind = FileChangeKind::Created;
        let cloned = kind.clone();
        let copied = kind;
        assert_eq!(kind, cloned);
        assert_eq!(kind, copied);
    }

    #[test]
    fn test_tool_source() {
        assert_eq!(ToolSource::default(), ToolSource::Builtin);
        assert_eq!(format!("{}", ToolSource::Rune), "rune");
        assert_eq!(
            format!(
                "{}",
                ToolSource::Mcp {
                    server: "test".into()
                }
            ),
            "mcp:test"
        );
        assert_eq!(format!("{}", ToolSource::Builtin), "builtin");
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("read_file", serde_json::json!({"path": "/tmp/test.txt"}))
            .with_call_id("call_123");

        assert_eq!(call.name, "read_file");
        assert_eq!(call.args["path"], "/tmp/test.txt");
        assert_eq!(call.call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_session_event_config() {
        let config = SessionEventConfig::new("test-session")
            .with_folder("/tmp/session")
            .with_max_context_tokens(50_000)
            .with_system_prompt("You are helpful.");

        assert_eq!(config.session_id, "test-session");
        assert_eq!(config.folder, Some(PathBuf::from("/tmp/session")));
        assert_eq!(config.max_context_tokens, 50_000);
        assert_eq!(config.system_prompt, Some("You are helpful.".to_string()));
    }

    #[test]
    fn test_session_event_default() {
        let event = SessionEvent::default();
        match event {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "default");
                assert_eq!(payload, JsonValue::Null);
            }
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_entity_type() {
        // Test default
        assert_eq!(EntityType::default(), EntityType::Note);

        // Test Display
        assert_eq!(format!("{}", EntityType::Note), "note");
        assert_eq!(format!("{}", EntityType::Block), "block");
        assert_eq!(format!("{}", EntityType::Tag), "tag");
        assert_eq!(format!("{}", EntityType::Task), "task");
        assert_eq!(format!("{}", EntityType::TaskFile), "task_file");

        // Test serialization
        let note = EntityType::Note;
        let json = serde_json::to_string(&note).unwrap();
        assert_eq!(json, "\"note\"");

        let task_file = EntityType::TaskFile;
        let json = serde_json::to_string(&task_file).unwrap();
        assert_eq!(json, "\"task_file\"");

        // Test deserialization
        let note: EntityType = serde_json::from_str("\"note\"").unwrap();
        assert_eq!(note, EntityType::Note);

        let task_file: EntityType = serde_json::from_str("\"task_file\"").unwrap();
        assert_eq!(task_file, EntityType::TaskFile);

        // Test equality and hashing
        assert_eq!(EntityType::Note, EntityType::Note);
        assert_ne!(EntityType::Note, EntityType::Block);

        // Test Clone and Copy
        let entity_type = EntityType::Task;
        let cloned = entity_type.clone();
        let copied = entity_type;
        assert_eq!(entity_type, cloned);
        assert_eq!(entity_type, copied);

        // Test Hash (use in HashSet)
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(EntityType::Note);
        set.insert(EntityType::Block);
        set.insert(EntityType::Note); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_priority() {
        // Test default
        assert_eq!(Priority::default(), Priority::Normal);

        // Test Display
        assert_eq!(format!("{}", Priority::Low), "low");
        assert_eq!(format!("{}", Priority::Normal), "normal");
        assert_eq!(format!("{}", Priority::High), "high");
        assert_eq!(format!("{}", Priority::Critical), "critical");

        // Test serialization
        let low = Priority::Low;
        let json = serde_json::to_string(&low).unwrap();
        assert_eq!(json, "\"low\"");

        let critical = Priority::Critical;
        let json = serde_json::to_string(&critical).unwrap();
        assert_eq!(json, "\"critical\"");

        // Test deserialization
        let low: Priority = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(low, Priority::Low);

        let critical: Priority = serde_json::from_str("\"critical\"").unwrap();
        assert_eq!(critical, Priority::Critical);

        // Test equality
        assert_eq!(Priority::Normal, Priority::Normal);
        assert_ne!(Priority::Low, Priority::High);

        // Test Clone and Copy
        let priority = Priority::High;
        let cloned = priority.clone();
        let copied = priority;
        assert_eq!(priority, cloned);
        assert_eq!(priority, copied);

        // Test Hash (use in HashSet)
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Priority::Low);
        set.insert(Priority::High);
        set.insert(Priority::Low); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_priority_ordering() {
        // Test that higher priority values compare greater
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);

        // Test min/max
        assert!(Priority::Critical >= Priority::Low);
        assert!(Priority::Low <= Priority::Critical);

        // Test sorting
        let mut priorities = vec![
            Priority::Normal,
            Priority::Critical,
            Priority::Low,
            Priority::High,
        ];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![
                Priority::Low,
                Priority::Normal,
                Priority::High,
                Priority::Critical
            ]
        );
    }

    #[test]
    fn test_session_event_priority() {
        // FileChanged(Created) → High
        let created = SessionEvent::FileChanged {
            path: PathBuf::from("/notes/new.md"),
            kind: FileChangeKind::Created,
        };
        assert_eq!(created.priority(), Priority::High);

        // FileChanged(Modified) → Normal
        let modified = SessionEvent::FileChanged {
            path: PathBuf::from("/notes/existing.md"),
            kind: FileChangeKind::Modified,
        };
        assert_eq!(modified.priority(), Priority::Normal);

        // FileDeleted → Low
        let deleted = SessionEvent::FileDeleted {
            path: PathBuf::from("/notes/old.md"),
        };
        assert_eq!(deleted.priority(), Priority::Low);

        // FileMoved → Normal
        let moved = SessionEvent::FileMoved {
            from: PathBuf::from("/notes/old.md"),
            to: PathBuf::from("/notes/new.md"),
        };
        assert_eq!(moved.priority(), Priority::Normal);

        // EmbeddingRequested → uses embedded priority
        let embedding_normal = SessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::Normal,
        };
        assert_eq!(embedding_normal.priority(), Priority::Normal);

        let embedding_high = SessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::High,
        };
        assert_eq!(embedding_high.priority(), Priority::High);

        let embedding_critical = SessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::Critical,
        };
        assert_eq!(embedding_critical.priority(), Priority::Critical);

        // Other events default to Normal
        let message = SessionEvent::MessageReceived {
            content: "hello".into(),
            participant_id: "user".into(),
        };
        assert_eq!(message.priority(), Priority::Normal);

        let tool_called = SessionEvent::ToolCalled {
            name: "search".into(),
            args: JsonValue::Null,
        };
        assert_eq!(tool_called.priority(), Priority::Normal);

        let entity_stored = SessionEvent::EntityStored {
            entity_id: "test".into(),
            entity_type: EntityType::Note,
        };
        assert_eq!(entity_stored.priority(), Priority::Normal);

        let custom = SessionEvent::Custom {
            name: "custom".into(),
            payload: JsonValue::Null,
        };
        assert_eq!(custom.priority(), Priority::Normal);
    }
}
