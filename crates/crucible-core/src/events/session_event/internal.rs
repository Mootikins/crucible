//! Internal session events — daemon pipeline signals that never cross the RPC wire.
//!
//! These events are used internally by the daemon for file watching, note processing,
//! storage operations, embedding generation, handler interception, and session lifecycle.
//! They are wrapped in `SessionEvent::Internal(Box<InternalSessionEvent>)` for dispatch
//! through the reactor/event system, but are filtered out before RPC serialization.
//!
//! Inspired by Neovim's RPC model where internal events never cross the wire.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

use super::{
    EntityType, FileChangeKind, InputType, NoteChangeType, NotePayload, Priority, TerminalStream,
    ToolProvider,
};

/// Internal session events that flow through the daemon's event system but never
/// cross the RPC wire to clients.
///
/// These are wrapped in [`SessionEvent::Internal`] for reactor dispatch.
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
    /// Note was parsed (AST available).
    NoteParsed {
        /// Path to the parsed note.
        path: PathBuf,
        /// Number of parsed blocks.
        block_count: usize,
        /// Optional payload with full parsed note data.
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

    /// Note was deleted.
    NoteDeleted {
        /// Path to the deleted note.
        path: PathBuf,
        /// Whether the note existed before deletion.
        existed: bool,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Storage events (database persistence)
    // ─────────────────────────────────────────────────────────────────────
    /// Entity was stored/upserted to the database.
    EntityStored {
        /// The entity identifier.
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
    BlocksUpdated {
        /// The parent entity identifier.
        entity_id: String,
        /// Number of blocks after the update.
        block_count: usize,
    },

    /// A relation between entities was stored.
    RelationStored {
        /// Source entity identifier.
        from_id: String,
        /// Target entity identifier.
        to_id: String,
        /// Type of relation.
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
    TagAssociated {
        /// The entity identifier the tag is associated with.
        entity_id: String,
        /// The tag name (without the # prefix).
        tag: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Embedding events
    // ─────────────────────────────────────────────────────────────────────
    /// Embedding generation was requested.
    EmbeddingRequested {
        /// The entity identifier to generate embeddings for.
        entity_id: String,
        /// Optional block identifier for block-level embeddings.
        block_id: Option<String>,
        /// Priority of the request.
        priority: Priority,
    },

    /// An embedding vector was stored.
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
    EmbeddingFailed {
        /// The entity identifier for which embedding failed.
        entity_id: String,
        /// Optional block identifier for block-level embeddings.
        block_id: Option<String>,
        /// Error message describing the failure.
        error: String,
    },

    /// Batch of embeddings completed for an entity.
    EmbeddingBatchComplete {
        /// The entity identifier for which embeddings were generated.
        entity_id: String,
        /// Number of embeddings generated in this batch.
        count: usize,
        /// Duration of the batch processing in milliseconds.
        duration_ms: u64,
    },

    /// Stored embedding model differs from currently configured model.
    EmbeddingModelMismatch {
        /// Path to the kiln with the mismatch.
        kiln_path: String,
        /// The embedding model stored in the kiln.
        stored_model: String,
        /// The currently configured embedding model.
        current_model: String,
        /// Number of notes with embeddings in the kiln.
        note_count: usize,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Pre/post events (handler interception points)
    // ─────────────────────────────────────────────────────────────────────
    /// Pre-event before tool execution (allows cancellation/modification).
    PreToolCall {
        /// Name of the tool about to be called.
        name: String,
        /// Arguments that will be passed to the tool.
        args: JsonValue,
    },

    /// Pre-event before file parsing.
    PreParse {
        /// Path to the file about to be parsed.
        path: PathBuf,
    },

    /// Pre-event before LLM call (allows cancellation/modification).
    PreLlmCall {
        /// The prompt text being sent.
        prompt: String,
        /// The model being used.
        model: String,
    },

    /// Post-event after LLM call completes (fire-and-forget notification).
    PostLlmCall {
        /// Summary of the response (first 200 chars, truncated).
        response_summary: String,
        /// The model that was used.
        model: String,
        /// Duration of the LLM call in milliseconds.
        duration_ms: u64,
        /// Token count if available from the provider.
        token_count: Option<u64>,
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

    /// Kiln requires data classification before use.
    ClassificationRequired {
        /// Path to the kiln that needs classification.
        kiln_path: PathBuf,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Session state events
    // ─────────────────────────────────────────────────────────────────────
    /// Session context was compacted.
    SessionCompacted {
        /// Summary of the compacted context.
        summary: String,
        /// Path to the new context file.
        new_file: PathBuf,
    },

    /// Session state changed (daemon protocol event).
    SessionStateChanged {
        /// The session ID.
        session_id: String,
        /// The new state.
        state: crate::session::SessionState,
        /// The previous state (if known).
        previous_state: Option<crate::session::SessionState>,
    },

    /// Session paused (agent stops acting).
    SessionPaused {
        /// The session ID.
        session_id: String,
    },

    /// Session resumed after being paused.
    SessionResumed {
        /// The session ID.
        session_id: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Background task events
    // ─────────────────────────────────────────────────────────────────────
    /// Background bash task was spawned.
    BashTaskSpawned {
        /// Unique task identifier.
        id: String,
        /// Shell command being executed.
        command: String,
    },

    /// Background bash task completed successfully.
    BashTaskCompleted {
        /// Task identifier.
        id: String,
        /// Command output (stdout).
        output: String,
        /// Process exit code.
        exit_code: i32,
    },

    /// Background bash task failed.
    BashTaskFailed {
        /// Task identifier.
        id: String,
        /// Error message.
        error: String,
        /// Process exit code if available.
        exit_code: Option<i32>,
    },

    /// Background task completed (for injection into conversation context).
    BackgroundTaskCompleted {
        /// Task identifier.
        id: String,
        /// Task kind ("subagent" or "bash").
        kind: String,
        /// Truncated summary of the result.
        summary: String,
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
    // Tool/MCP discovery events
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
        source: ToolProvider,
        /// Optional JSON schema for the tool's arguments.
        schema: Option<JsonValue>,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Terminal / streaming events
    // ─────────────────────────────────────────────────────────────────────
    /// Terminal output from tool execution (daemon protocol event).
    TerminalOutput {
        /// The session ID.
        session_id: String,
        /// Stream identifier (stdout or stderr).
        stream: TerminalStream,
        /// Base64-encoded content for binary safety.
        content_base64: String,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Input events
    // ─────────────────────────────────────────────────────────────────────
    /// System is awaiting human input (HIL gate / idle prompt).
    AwaitingInput {
        /// What kind of input is needed.
        input_type: InputType,
        /// Optional context.
        context: Option<String>,
    },
}

impl InternalSessionEvent {
    /// Get the event type name (snake_case) for filtering and pattern matching.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::FileChanged { .. } => "file_changed",
            Self::FileDeleted { .. } => "file_deleted",
            Self::FileMoved { .. } => "file_moved",
            Self::NoteParsed { .. } => "note_parsed",
            Self::NoteCreated { .. } => "note_created",
            Self::NoteModified { .. } => "note_modified",
            Self::NoteDeleted { .. } => "note_deleted",
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
            Self::EmbeddingModelMismatch { .. } => "embedding_model_mismatch",
            Self::PreToolCall { .. } => "pre_tool_call",
            Self::PreParse { .. } => "pre_parse",
            Self::PreLlmCall { .. } => "pre_llm_call",
            Self::PostLlmCall { .. } => "post_llm_call",
            Self::PrecognitionComplete { .. } => "precognition_complete",
            Self::ClassificationRequired { .. } => "classification_required",
            Self::SessionCompacted { .. } => "session_compacted",
            Self::SessionStateChanged { .. } => "session_state_changed",
            Self::SessionPaused { .. } => "session_paused",
            Self::SessionResumed { .. } => "session_resumed",
            Self::BashTaskSpawned { .. } => "bash_task_spawned",
            Self::BashTaskCompleted { .. } => "bash_task_completed",
            Self::BashTaskFailed { .. } => "bash_task_failed",
            Self::BackgroundTaskCompleted { .. } => "background_task_completed",
            Self::SubagentSpawned { .. } => "subagent_spawned",
            Self::SubagentCompleted { .. } => "subagent_completed",
            Self::SubagentFailed { .. } => "subagent_failed",
            Self::McpAttached { .. } => "mcp_attached",
            Self::ToolDiscovered { .. } => "tool_discovered",
            Self::TerminalOutput { .. } => "terminal_output",
            Self::AwaitingInput { .. } => "awaiting_input",
        }
    }

    /// Get the PascalCase type name of this event.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FileChanged { .. } => "FileChanged",
            Self::FileDeleted { .. } => "FileDeleted",
            Self::FileMoved { .. } => "FileMoved",
            Self::NoteParsed { .. } => "NoteParsed",
            Self::NoteCreated { .. } => "NoteCreated",
            Self::NoteModified { .. } => "NoteModified",
            Self::NoteDeleted { .. } => "NoteDeleted",
            Self::EntityStored { .. } => "EntityStored",
            Self::EntityDeleted { .. } => "EntityDeleted",
            Self::BlocksUpdated { .. } => "BlocksUpdated",
            Self::RelationStored { .. } => "RelationStored",
            Self::RelationDeleted { .. } => "RelationDeleted",
            Self::TagAssociated { .. } => "TagAssociated",
            Self::EmbeddingRequested { .. } => "EmbeddingRequested",
            Self::EmbeddingStored { .. } => "EmbeddingStored",
            Self::EmbeddingFailed { .. } => "EmbeddingFailed",
            Self::EmbeddingBatchComplete { .. } => "EmbeddingBatchComplete",
            Self::EmbeddingModelMismatch { .. } => "EmbeddingModelMismatch",
            Self::PreToolCall { .. } => "PreToolCall",
            Self::PreParse { .. } => "PreParse",
            Self::PreLlmCall { .. } => "PreLlmCall",
            Self::PostLlmCall { .. } => "PostLlmCall",
            Self::PrecognitionComplete { .. } => "PrecognitionComplete",
            Self::ClassificationRequired { .. } => "ClassificationRequired",
            Self::SessionCompacted { .. } => "SessionCompacted",
            Self::SessionStateChanged { .. } => "SessionStateChanged",
            Self::SessionPaused { .. } => "SessionPaused",
            Self::SessionResumed { .. } => "SessionResumed",
            Self::BashTaskSpawned { .. } => "BashTaskSpawned",
            Self::BashTaskCompleted { .. } => "BashTaskCompleted",
            Self::BashTaskFailed { .. } => "BashTaskFailed",
            Self::BackgroundTaskCompleted { .. } => "BackgroundTaskCompleted",
            Self::SubagentSpawned { .. } => "SubagentSpawned",
            Self::SubagentCompleted { .. } => "SubagentCompleted",
            Self::SubagentFailed { .. } => "SubagentFailed",
            Self::McpAttached { .. } => "McpAttached",
            Self::ToolDiscovered { .. } => "ToolDiscovered",
            Self::TerminalOutput { .. } => "TerminalOutput",
            Self::AwaitingInput { .. } => "AwaitingInput",
        }
    }

    /// Get the identifier for pattern matching (tool name, note path, etc.).
    pub fn identifier(&self) -> String {
        match self {
            Self::FileChanged { path, .. } => path.display().to_string(),
            Self::FileDeleted { path, .. } => path.display().to_string(),
            Self::FileMoved { to, .. } => to.display().to_string(),
            Self::NoteParsed { path, .. } => path.display().to_string(),
            Self::NoteCreated { path, .. } => path.display().to_string(),
            Self::NoteModified { path, .. } => path.display().to_string(),
            Self::NoteDeleted { path, .. } => path.display().to_string(),
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
            Self::EmbeddingModelMismatch { kiln_path, .. } => {
                format!("embedding:mismatch:{}", kiln_path)
            }
            Self::PreToolCall { name, .. } => format!("pre:tool:{}", name),
            Self::PreParse { path, .. } => format!("pre:parse:{}", path.display()),
            Self::PreLlmCall { model, .. } => format!("pre:llm:{}", model),
            Self::PostLlmCall { model, .. } => format!("post:llm:{}", model),
            Self::PrecognitionComplete { .. } => "precognition:complete".into(),
            Self::ClassificationRequired { kiln_path, .. } => {
                format!("classification:required:{}", kiln_path.display())
            }
            Self::SessionCompacted { .. } => "session:compacted".into(),
            Self::SessionStateChanged { session_id, .. } => {
                format!("session:state_changed:{}", session_id)
            }
            Self::SessionPaused { session_id, .. } => format!("session:paused:{}", session_id),
            Self::SessionResumed { session_id, .. } => format!("session:resumed:{}", session_id),
            Self::BashTaskSpawned { id, .. } => format!("bash:spawned:{}", id),
            Self::BashTaskCompleted { id, .. } => format!("bash:completed:{}", id),
            Self::BashTaskFailed { id, .. } => format!("bash:failed:{}", id),
            Self::BackgroundTaskCompleted { id, kind, .. } => {
                format!("background:{}:{}", kind, id)
            }
            Self::SubagentSpawned { id, .. } => format!("subagent:spawned:{}", id),
            Self::SubagentCompleted { id, .. } => format!("subagent:completed:{}", id),
            Self::SubagentFailed { id, .. } => format!("subagent:failed:{}", id),
            Self::McpAttached { server, .. } => server.clone(),
            Self::ToolDiscovered { name, .. } => name.clone(),
            Self::TerminalOutput {
                session_id, stream, ..
            } => format!("terminal:{}:{}", session_id, stream),
            Self::AwaitingInput { input_type, .. } => format!("await:{}", input_type),
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
            Self::FileMoved { .. } => Priority::Normal,
            Self::EmbeddingRequested { priority, .. } => *priority,
            _ => Priority::Normal,
        }
    }

    /// Check if this is a note-related event.
    pub fn is_note_event(&self) -> bool {
        matches!(
            self,
            Self::NoteParsed { .. }
                | Self::NoteCreated { .. }
                | Self::NoteModified { .. }
                | Self::NoteDeleted { .. }
        )
    }

    /// Check if this is a file system event.
    pub fn is_file_event(&self) -> bool {
        matches!(
            self,
            Self::FileChanged { .. } | Self::FileDeleted { .. } | Self::FileMoved { .. }
        )
    }

    /// Check if this is an embedding-related event.
    pub fn is_embedding_event(&self) -> bool {
        matches!(
            self,
            Self::EmbeddingRequested { .. }
                | Self::EmbeddingStored { .. }
                | Self::EmbeddingFailed { .. }
                | Self::EmbeddingBatchComplete { .. }
                | Self::EmbeddingModelMismatch { .. }
        )
    }

    /// Check if this is a storage event.
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

    /// Check if this is a pre-event (interception point).
    pub fn is_pre_event(&self) -> bool {
        matches!(
            self,
            Self::PreToolCall { .. } | Self::PreParse { .. } | Self::PreLlmCall { .. }
        )
    }

    /// Check if this is a lifecycle event.
    pub fn is_lifecycle_event(&self) -> bool {
        matches!(
            self,
            Self::SessionCompacted { .. }
                | Self::SessionStateChanged { .. }
                | Self::SessionPaused { .. }
                | Self::SessionResumed { .. }
        )
    }

    /// Check if this is a subagent event.
    pub fn is_subagent_event(&self) -> bool {
        matches!(
            self,
            Self::SubagentSpawned { .. }
                | Self::SubagentCompleted { .. }
                | Self::SubagentFailed { .. }
        )
    }

    /// Check if this is a background task event.
    pub fn is_background_task_event(&self) -> bool {
        matches!(
            self,
            Self::BashTaskSpawned { .. }
                | Self::BashTaskCompleted { .. }
                | Self::BashTaskFailed { .. }
                | Self::BackgroundTaskCompleted { .. }
        )
    }

    /// Check if this is a tool-related event.
    pub fn is_tool_event(&self) -> bool {
        matches!(self, Self::ToolDiscovered { .. })
    }

    /// Check if this is a streaming event.
    pub fn is_streaming_event(&self) -> bool {
        matches!(self, Self::TerminalOutput { .. })
    }

    /// Check if this is an MCP-related event.
    pub fn is_mcp_event(&self) -> bool {
        matches!(self, Self::McpAttached { .. })
    }

    /// Estimate content length for token estimation.
    pub fn estimate_content_len(&self) -> usize {
        match self {
            Self::NoteParsed { .. } => 50,
            Self::NoteCreated { title, .. } => title.as_ref().map(|t| t.len()).unwrap_or(0) + 50,
            Self::NoteModified { .. } => 50,
            Self::NoteDeleted { .. } => 50,
            Self::FileChanged { .. } => 50,
            Self::FileDeleted { .. } => 50,
            Self::FileMoved { .. } => 50,
            Self::EntityStored { .. } => 50,
            Self::EntityDeleted { .. } => 50,
            Self::BlocksUpdated { .. } => 50,
            Self::RelationStored { .. } => 50,
            Self::RelationDeleted { .. } => 50,
            Self::TagAssociated { tag, .. } => tag.len() + 50,
            Self::EmbeddingRequested { .. } => 50,
            Self::EmbeddingStored { .. } => 50,
            Self::EmbeddingFailed { error, .. } => error.len() + 50,
            Self::EmbeddingBatchComplete { .. } => 50,
            Self::EmbeddingModelMismatch {
                kiln_path,
                stored_model,
                current_model,
                ..
            } => kiln_path.len() + stored_model.len() + current_model.len() + 50,
            Self::PreToolCall { name, .. } => name.len() + 50,
            Self::PreParse { .. } => 50,
            Self::PreLlmCall { prompt, .. } => prompt.len(),
            Self::PostLlmCall {
                response_summary, ..
            } => response_summary.len(),
            Self::PrecognitionComplete {
                notes_count,
                query_summary,
                ..
            } => notes_count.to_string().len() + query_summary.len() + 50,
            Self::ClassificationRequired { .. } => 50,
            Self::SessionCompacted { summary, .. } => summary.len(),
            Self::SessionStateChanged { .. } => 50,
            Self::SessionPaused { .. } => 50,
            Self::SessionResumed { .. } => 50,
            Self::SubagentSpawned { prompt, .. } => prompt.len(),
            Self::SubagentCompleted { result, .. } => result.len(),
            Self::SubagentFailed { error, .. } => error.len(),
            Self::BashTaskSpawned { command, .. } => command.len(),
            Self::BashTaskCompleted { output, .. } => output.len(),
            Self::BashTaskFailed { error, .. } => error.len(),
            Self::BackgroundTaskCompleted { summary, .. } => summary.len(),
            Self::McpAttached { server, .. } => server.len() + 50,
            Self::ToolDiscovered { name, schema, .. } => {
                name.len() + schema.as_ref().map(|s| s.to_string().len()).unwrap_or(0)
            }
            Self::TerminalOutput { content_base64, .. } => content_base64.len(),
            Self::AwaitingInput { context, .. } => context.as_ref().map_or(20, |c| c.len() + 20),
        }
    }
}

impl InternalSessionEvent {
    /// Get a summary of this event's content, truncated to `max_len`.
    pub fn summary(&self, max_len: usize) -> String {
        fn trunc(s: &str, max_len: usize) -> &str {
            if s.len() <= max_len {
                return s;
            }
            let mut end = max_len;
            while !s.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &s[..end]
        }
        match self {
            Self::FileChanged { path, kind } => format!("path={}, kind={:?}", path.display(), kind),
            Self::FileDeleted { path } => format!("path={}", path.display()),
            Self::FileMoved { from, to } => format!("from={}, to={}", from.display(), to.display()),
            Self::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                let ps = if payload.is_some() {
                    ", has_payload"
                } else {
                    ""
                };
                format!("path={}, blocks={}{}", path.display(), block_count, ps)
            }
            Self::NoteCreated { path, title } => {
                let t = title.as_deref().unwrap_or("(none)");
                format!("path={}, title={}", path.display(), trunc(t, max_len))
            }
            Self::NoteModified { path, change_type } => {
                format!("path={}, change={:?}", path.display(), change_type)
            }
            Self::NoteDeleted { path, existed } => {
                format!("path={}, existed={}", path.display(), existed)
            }
            Self::EntityStored {
                entity_id,
                entity_type,
            } => format!("entity_id={}, type={:?}", entity_id, entity_type),
            Self::EntityDeleted {
                entity_id,
                entity_type,
            } => format!("entity_id={}, type={:?}", entity_id, entity_type),
            Self::BlocksUpdated {
                entity_id,
                block_count,
            } => format!("entity_id={}, blocks={}", entity_id, block_count),
            Self::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => format!("from={}, to={}, type={}", from_id, to_id, relation_type),
            Self::RelationDeleted {
                from_id,
                to_id,
                relation_type,
            } => format!("from={}, to={}, type={}", from_id, to_id, relation_type),
            Self::TagAssociated { entity_id, tag } => {
                format!("entity_id={}, tag={}", entity_id, tag)
            }
            Self::EmbeddingRequested {
                entity_id,
                priority,
                ..
            } => format!("entity_id={}, priority={:?}", entity_id, priority),
            Self::EmbeddingStored {
                entity_id,
                dimensions,
                ..
            } => format!("entity_id={}, dims={}", entity_id, dimensions),
            Self::EmbeddingFailed {
                entity_id, error, ..
            } => format!("entity_id={}, error={}", entity_id, trunc(error, max_len)),
            Self::EmbeddingBatchComplete {
                entity_id,
                count,
                duration_ms,
            } => format!(
                "entity_id={}, count={}, duration={}ms",
                entity_id, count, duration_ms
            ),
            Self::EmbeddingModelMismatch {
                kiln_path,
                stored_model,
                current_model,
                note_count,
            } => format!(
                "kiln={}, stored={}, current={}, notes={}",
                kiln_path, stored_model, current_model, note_count
            ),
            Self::PreToolCall { name, args } => {
                format!("tool={}, args_size={}", name, args.to_string().len())
            }
            Self::PreParse { path } => format!("path={}", path.display()),
            Self::PreLlmCall { prompt, model } => {
                format!("model={}, prompt_len={}", model, prompt.len())
            }
            Self::PostLlmCall {
                response_summary,
                model,
                duration_ms,
                token_count,
            } => {
                let ts = token_count.map_or("none".to_string(), |t| t.to_string());
                format!(
                    "model={}, duration={}ms, tokens={}, summary_len={}",
                    model,
                    duration_ms,
                    ts,
                    response_summary.len()
                )
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
                    trunc(query_summary, max_len),
                    kilns_searched,
                    kilns_filtered,
                    kilns_failed
                )
            }
            Self::ClassificationRequired { kiln_path } => {
                format!("kiln_path={}", kiln_path.display())
            }
            Self::SessionCompacted { summary, new_file } => format!(
                "summary_len={}, new_file={}",
                summary.len(),
                new_file.display()
            ),
            Self::SessionStateChanged {
                session_id,
                state,
                previous_state,
            } => {
                let prev = previous_state
                    .as_ref()
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "(none)".to_string());
                format!("session={}, state={:?}, prev={}", session_id, state, prev)
            }
            Self::SessionPaused { session_id } => format!("session={}", session_id),
            Self::SessionResumed { session_id } => format!("session={}", session_id),
            Self::BashTaskSpawned { id, command } => {
                format!("id={}, command={}", id, trunc(command, max_len))
            }
            Self::BashTaskCompleted {
                id,
                output,
                exit_code,
            } => format!(
                "id={}, exit_code={}, output_len={}",
                id,
                exit_code,
                output.len()
            ),
            Self::BashTaskFailed {
                id,
                error,
                exit_code,
            } => {
                let cs = exit_code.map_or("none".to_string(), |c| c.to_string());
                format!(
                    "id={}, exit_code={}, error={}",
                    id,
                    cs,
                    trunc(error, max_len)
                )
            }
            Self::BackgroundTaskCompleted { id, kind, summary } => format!(
                "id={}, kind={}, summary={}",
                id,
                kind,
                trunc(summary, max_len)
            ),
            Self::SubagentSpawned { id, prompt } => {
                format!("id={}, prompt_len={}", id, prompt.len())
            }
            Self::SubagentCompleted { id, result } => {
                format!("id={}, result_len={}", id, result.len())
            }
            Self::SubagentFailed { id, error } => {
                format!("id={}, error={}", id, trunc(error, max_len))
            }
            Self::McpAttached { server, tool_count } => {
                format!("server={}, tools={}", server, tool_count)
            }
            Self::ToolDiscovered { name, source, .. } => {
                format!("name={}, source={:?}", name, source)
            }
            Self::TerminalOutput {
                session_id,
                stream,
                content_base64,
            } => format!(
                "session={}, stream={:?}, content_len={}",
                session_id,
                stream,
                content_base64.len()
            ),
            Self::AwaitingInput {
                input_type,
                context,
            } => format!(
                "type={}, context={}",
                input_type,
                context.as_deref().unwrap_or("(none)")
            ),
        }
    }

    /// Extract the raw payload content from this event.
    pub fn payload_content(&self) -> Option<String> {
        match self {
            Self::NoteParsed { path, .. } => Some(path.display().to_string()),
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
            Self::EntityStored {
                entity_id,
                entity_type,
            } => Some(format!("{}: {:?}", entity_id, entity_type)),
            Self::EntityDeleted {
                entity_id,
                entity_type,
            } => Some(format!("{}: {:?}", entity_id, entity_type)),
            Self::BlocksUpdated {
                entity_id,
                block_count,
            } => Some(format!("{}: {} blocks", entity_id, block_count)),
            Self::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => Some(format!("{} -> {} ({})", from_id, to_id, relation_type)),
            Self::RelationDeleted {
                from_id,
                to_id,
                relation_type,
            } => Some(format!("{} -> {} ({})", from_id, to_id, relation_type)),
            Self::TagAssociated { entity_id, tag } => Some(format!("{}#{}", entity_id, tag)),
            Self::EmbeddingRequested {
                entity_id,
                priority,
                ..
            } => Some(format!("{}: {:?}", entity_id, priority)),
            Self::EmbeddingStored {
                entity_id,
                dimensions,
                model,
                ..
            } => Some(format!(
                "{}: {} dims, model={}",
                entity_id, dimensions, model
            )),
            Self::EmbeddingFailed {
                entity_id, error, ..
            } => Some(format!("{}: {}", entity_id, error)),
            Self::EmbeddingBatchComplete {
                entity_id,
                count,
                duration_ms,
            } => Some(format!(
                "{}: {} embeddings in {}ms",
                entity_id, count, duration_ms
            )),
            Self::EmbeddingModelMismatch {
                kiln_path,
                stored_model,
                current_model,
                note_count,
            } => Some(format!(
                "kiln={}, stored={}, current={}, notes={}",
                kiln_path, stored_model, current_model, note_count
            )),
            Self::PreToolCall { args, .. } => Some(args.to_string()),
            Self::PreParse { path } => Some(path.display().to_string()),
            Self::PreLlmCall { prompt, .. } => Some(prompt.clone()),
            Self::PostLlmCall {
                response_summary, ..
            } => Some(response_summary.clone()),
            Self::PrecognitionComplete {
                notes_count,
                query_summary,
                ..
            } => Some(format!("notes={}, query={}", notes_count, query_summary)),
            Self::ClassificationRequired { kiln_path } => Some(kiln_path.display().to_string()),
            Self::SessionCompacted { summary, .. } => Some(summary.clone()),
            Self::SessionStateChanged {
                session_id,
                state,
                previous_state,
            } => Some(format!(
                "session={}, state={:?}, previous={:?}",
                session_id, state, previous_state
            )),
            Self::SessionPaused { session_id } => Some(format!("session={}", session_id)),
            Self::SessionResumed { session_id } => Some(format!("session={}", session_id)),
            Self::BashTaskSpawned { command, .. } => Some(command.clone()),
            Self::BashTaskCompleted { output, .. } => Some(output.clone()),
            Self::BashTaskFailed { error, .. } => Some(error.clone()),
            Self::BackgroundTaskCompleted { summary, .. } => Some(summary.clone()),
            Self::SubagentSpawned { prompt, .. } => Some(prompt.clone()),
            Self::SubagentCompleted { result, .. } => Some(result.clone()),
            Self::SubagentFailed { error, .. } => Some(error.clone()),
            Self::McpAttached { server, tool_count } => {
                Some(format!("{}: {} tools", server, tool_count))
            }
            Self::ToolDiscovered {
                name,
                source,
                schema,
            } => {
                let sl = schema.as_ref().map(|s| s.to_string().len()).unwrap_or(0);
                Some(format!("{}: {:?}, schema_len={}", name, source, sl))
            }
            Self::TerminalOutput { content_base64, .. } => Some(content_base64.clone()),
            Self::AwaitingInput { context, .. } => context.clone(),
        }
    }
}
