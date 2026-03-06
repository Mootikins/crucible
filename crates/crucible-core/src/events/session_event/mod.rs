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

// Submodules for logical organization
pub mod display;
pub mod helpers;
pub mod internal;
pub mod payloads;
pub mod tool_call;
pub mod types;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

pub use internal::InternalSessionEvent;

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

/// Events that flow through a session (wire-facing).
///
/// These are the events that cross the RPC wire between daemon and clients.
/// Internal pipeline events live in [`InternalSessionEvent`] and are wrapped
/// via the `Internal` variant for reactor dispatch.
///
/// # Wire Event Categories
///
/// - **User/participant**: `MessageReceived`
/// - **Agent**: `AgentResponded`, `AgentThinking`
/// - **Tool**: `ToolCalled`, `ToolCompleted`
/// - **Session lifecycle**: `SessionStarted`, `SessionEnded`
/// - **Delegation**: `DelegationSpawned`, `DelegationCompleted`, `DelegationFailed`
/// - **Streaming**: `TextDelta`
/// - **Interaction**: `InteractionRequested`, `InteractionCompleted`
/// - **Custom**: `Custom` for extensibility
/// - **Internal**: Wrapper for [`InternalSessionEvent`]
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
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

    /// Session ended.
    SessionEnded {
        /// Reason for ending the session.
        reason: String,
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
    // Interaction events
    // ─────────────────────────────────────────────────────────────────────
    /// Agent/tool requests structured user interaction.
    InteractionRequested {
        /// Unique ID for correlating request with response.
        request_id: String,
        /// The interaction request details.
        request: crate::interaction::InteractionRequest,
    },

    /// User responded to an interaction request.
    InteractionCompleted {
        /// The request ID this response corresponds to.
        request_id: String,
        /// The user's response.
        response: crate::interaction::InteractionResponse,
    },

    // ─────────────────────────────────────────────────────────────────────
    // Delegation events (session-to-session delegation via ACP)
    // ─────────────────────────────────────────────────────────────────────
    /// Delegation was spawned (child session created).
    DelegationSpawned {
        /// Unique identifier for the delegation.
        delegation_id: String,
        /// Prompt given to the delegated session.
        prompt: String,
        /// Parent session ID that initiated the delegation.
        parent_session_id: String,
        /// Target agent name if delegating to a different agent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_agent: Option<String>,
    },

    /// Delegation completed successfully.
    DelegationCompleted {
        /// Identifier of the completed delegation.
        delegation_id: String,
        /// Summary of the delegation result.
        result_summary: String,
        /// Parent session ID that initiated the delegation.
        parent_session_id: String,
    },

    /// Delegation failed.
    DelegationFailed {
        /// Identifier of the failed delegation.
        delegation_id: String,
        /// Error message.
        error: String,
        /// Parent session ID that initiated the delegation.
        parent_session_id: String,
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

    /// Internal daemon event (never crosses RPC wire).
    /// Wraps [`InternalSessionEvent`] for reactor dispatch.
    Internal(Box<InternalSessionEvent>),
}

impl SessionEvent {
    /// Create an `Internal` variant wrapping an [`InternalSessionEvent`].
    pub fn internal(event: InternalSessionEvent) -> Self {
        Self::Internal(Box::new(event))
    }

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
            Self::SessionEnded { .. } => "session_ended",
            Self::TextDelta { .. } => "text_delta",
            Self::InteractionRequested { .. } => "interaction_requested",
            Self::InteractionCompleted { .. } => "interaction_completed",
            Self::DelegationSpawned { .. } => "delegation_spawned",
            Self::DelegationCompleted { .. } => "delegation_completed",
            Self::DelegationFailed { .. } => "delegation_failed",
            Self::Custom { .. } => "custom",
            Self::Internal(inner) => inner.event_type(),
        }
    }

    /// Get the identifier for pattern matching (tool name, note path, etc.).
    ///
    /// This is used by the EventBus for glob pattern matching against handlers.
    pub fn identifier(&self) -> String {
        identifier_for_event(self)
    }

    /// Check if this is a tool-related event.
    pub fn is_tool_event(&self) -> bool {
        match self {
            Self::ToolCalled { .. } | Self::ToolCompleted { .. } => true,
            Self::Internal(inner) => inner.is_tool_event(),
            _ => false,
        }
    }

    /// Check if this is a pre-event (interception point).
    ///
    /// Pre-events are emitted before the corresponding action occurs,
    /// allowing handlers to modify or cancel the operation.
    pub fn is_pre_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_pre_event(),
            _ => false,
        }
    }

    /// Check if this is an interaction event.
    ///
    /// Interaction events represent structured user input requests and responses.
    pub fn is_interaction_event(&self) -> bool {
        matches!(
            self,
            Self::InteractionRequested { .. } | Self::InteractionCompleted { .. }
        )
    }

    /// Check if this is a note-related event.
    pub fn is_note_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_note_event(),
            _ => false,
        }
    }

    /// Check if this is a session lifecycle event.
    pub fn is_lifecycle_event(&self) -> bool {
        match self {
            Self::SessionStarted { .. } | Self::SessionEnded { .. } => true,
            Self::Internal(inner) => inner.is_lifecycle_event(),
            _ => false,
        }
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
        match self {
            Self::Internal(inner) => inner.is_subagent_event(),
            _ => false,
        }
    }

    /// Check if this is a background task event (bash tasks or completion notifications).
    pub fn is_background_task_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_background_task_event(),
            _ => false,
        }
    }

    /// Check if this is a streaming event.
    pub fn is_streaming_event(&self) -> bool {
        match self {
            Self::TextDelta { .. } => true,
            Self::Internal(inner) => inner.is_streaming_event(),
            _ => false,
        }
    }

    /// Check if this is a file system event (raw file changes).
    ///
    /// File events represent raw file system changes before parsing.
    /// They are distinct from note events which represent parsed content.
    pub fn is_file_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_file_event(),
            _ => false,
        }
    }

    /// Check if this is an embedding-related event.
    ///
    /// Embedding events track the lifecycle of embedding generation:
    /// request, success (stored), or failure.
    pub fn is_embedding_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_embedding_event(),
            _ => false,
        }
    }

    /// Check if this is a storage event (database operations).
    ///
    /// Storage events represent persistence operations to the database.
    /// They are emitted after entities, blocks, relations, tags, or embeddings
    /// are stored or deleted.
    pub fn is_storage_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_storage_event(),
            _ => false,
        }
    }

    /// Check if this is an MCP-related event.
    pub fn is_mcp_event(&self) -> bool {
        match self {
            Self::Internal(inner) => inner.is_mcp_event(),
            _ => false,
        }
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
    /// use crucible_core::events::{SessionEvent, InternalSessionEvent, FileChangeKind, Priority};
    /// use std::path::PathBuf;
    ///
    /// let created = SessionEvent::internal(InternalSessionEvent::FileChanged {
    ///     path: PathBuf::from("/notes/new.md"),
    ///     kind: FileChangeKind::Created,
    /// });
    /// assert_eq!(created.priority(), Priority::High);
    ///
    /// let deleted = SessionEvent::internal(InternalSessionEvent::FileDeleted {
    ///     path: PathBuf::from("/notes/old.md"),
    /// });
    /// assert_eq!(deleted.priority(), Priority::Low);
    /// ```
    pub fn priority(&self) -> Priority {
        match self {
            Self::Internal(inner) => inner.priority(),
            _ => Priority::Normal,
        }
    }

    /// Get the PascalCase type name of this event.
    ///
    /// Returns a human-readable type name suitable for logging and display.
    /// Unlike `event_type()` which returns snake_case identifiers, this method
    /// returns PascalCase names matching the enum variant names.
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::SessionEvent;
    /// use serde_json::Value as JsonValue;
    ///
    /// let event = SessionEvent::ToolCalled {
    ///     name: "search".into(),
    ///     args: JsonValue::Null,
    /// };
    /// assert_eq!(event.type_name(), "ToolCalled");
    /// ```
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::MessageReceived { .. } => "MessageReceived",
            Self::AgentResponded { .. } => "AgentResponded",
            Self::AgentThinking { .. } => "AgentThinking",
            Self::ToolCalled { .. } => "ToolCalled",
            Self::ToolCompleted { .. } => "ToolCompleted",
            Self::SessionStarted { .. } => "SessionStarted",
            Self::SessionEnded { .. } => "SessionEnded",
            Self::TextDelta { .. } => "TextDelta",
            Self::InteractionRequested { .. } => "InteractionRequested",
            Self::InteractionCompleted { .. } => "InteractionCompleted",
            Self::DelegationSpawned { .. } => "DelegationSpawned",
            Self::DelegationCompleted { .. } => "DelegationCompleted",
            Self::DelegationFailed { .. } => "DelegationFailed",
            Self::Custom { .. } => "Custom",
            Self::Internal(inner) => inner.type_name(),
        }
    }

    /// Get a summary of this event's content.
    ///
    /// Returns a concise string describing the event's key fields, suitable for
    /// logging and debugging. The summary is truncated to `max_len` characters.
    ///
    /// # Arguments
    ///
    /// * `max_len` - Maximum length for any individual string field in the summary
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::SessionEvent;
    /// use serde_json::Value as JsonValue;
    ///
    /// let event = SessionEvent::ToolCalled {
    ///     name: "search".into(),
    ///     args: JsonValue::String("query".into()),
    /// };
    /// let summary = event.summary(100);
    /// assert!(summary.contains("tool=search"));
    /// ```
    pub fn summary(&self, max_len: usize) -> String {
        match self {
            Self::MessageReceived {
                content,
                participant_id,
            } => {
                format!("from={}, content_len={}", participant_id, content.len())
            }
            Self::AgentResponded {
                content,
                tool_calls,
            } => {
                format!(
                    "content_len={}, tool_calls={}",
                    content.len(),
                    tool_calls.len()
                )
            }
            Self::AgentThinking { thought } => {
                format!("thought_len={}", thought.len())
            }
            Self::ToolCalled { name, args } => {
                format!("tool={}, args_size={}", name, args.to_string().len())
            }
            Self::ToolCompleted {
                name,
                result,
                error,
            } => {
                format!(
                    "tool={}, result_len={}, error={}",
                    name,
                    result.len(),
                    error.is_some()
                )
            }
            Self::SessionStarted { config } => {
                format!("session_id={}", config.session_id)
            }
            Self::SessionEnded { reason } => {
                format!("reason={}", truncate(reason, max_len))
            }
            Self::TextDelta { delta, seq } => {
                format!("seq={}, delta_len={}", seq, delta.len())
            }
            Self::InteractionRequested {
                request_id,
                request,
            } => {
                format!("id={}, kind={}", request_id, request.kind())
            }
            Self::InteractionCompleted { request_id, .. } => {
                format!("id={}", request_id)
            }
            Self::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                target_agent,
            } => {
                let target_str = target_agent
                    .as_ref()
                    .map(|t| format!(", target={}", t))
                    .unwrap_or_default();
                format!(
                    "delegation_id={}, parent={}, prompt_len={}{}",
                    delegation_id,
                    parent_session_id,
                    prompt.len(),
                    target_str
                )
            }
            Self::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            } => {
                format!(
                    "delegation_id={}, parent={}, result_len={}",
                    delegation_id,
                    parent_session_id,
                    result_summary.len()
                )
            }
            Self::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            } => {
                format!(
                    "delegation_id={}, parent={}, error={}",
                    delegation_id,
                    parent_session_id,
                    truncate(error, max_len)
                )
            }
            Self::Custom { name, payload } => {
                format!("name={}, payload_size={}", name, payload.to_string().len())
            }
            Self::Internal(inner) => inner.summary(max_len),
        }
    }

    /// Get the detailed payload content of this event.
    ///
    /// Returns the main content or data associated with this event, truncated to
    /// `max_len` characters. Returns `None` for events that have no meaningful
    /// payload content.
    ///
    /// # Arguments
    ///
    /// * `max_len` - Maximum length for the returned string
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::SessionEvent;
    ///
    /// let event = SessionEvent::MessageReceived {
    ///     content: "Hello, world!".into(),
    ///     participant_id: "user".into(),
    /// };
    /// let payload = event.payload(100);
    /// assert_eq!(payload, Some("Hello, world!".to_string()));
    /// ```
    pub fn payload(&self, max_len: usize) -> Option<String> {
        payload_for_event(self).map(|p| truncate(&p, max_len).to_string())
    }

    /// Estimate the number of tokens in this event.
    ///
    /// This is a simple heuristic - real implementations should use a proper
    /// tokenizer like tiktoken. The estimate uses a rough approximation of
    /// ~4 characters per token for English text, plus a fixed overhead for
    /// event structure.
    ///
    /// # Returns
    ///
    /// An estimated token count, always at least 11 (10 overhead + 1 minimum content).
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::SessionEvent;
    ///
    /// let event = SessionEvent::MessageReceived {
    ///     content: "Hello, world!".into(),
    ///     participant_id: "user".into(),
    /// };
    /// let tokens = event.estimate_tokens();
    /// assert!(tokens > 10); // At least structural overhead
    /// ```
    pub fn estimate_tokens(&self) -> usize {
        let content_len = estimate_content_len(self);
        // Rough estimate: ~4 characters per token
        // Add fixed overhead for event structure
        (content_len / 4).max(1) + 10
    }
}

/// Compute the identifier string for a session event.
///
/// Used for glob pattern matching against event handlers.
fn identifier_for_event(event: &SessionEvent) -> String {
    match event {
        SessionEvent::MessageReceived { participant_id, .. } => {
            format!("message:{}", participant_id)
        }
        SessionEvent::AgentResponded { .. } => "agent:responded".into(),
        SessionEvent::AgentThinking { .. } => "agent:thinking".into(),
        SessionEvent::ToolCalled { name, .. } => name.clone(),
        SessionEvent::ToolCompleted { name, .. } => name.clone(),
        SessionEvent::SessionStarted { config, .. } => format!("session:{}", config.session_id),
        SessionEvent::SessionEnded { .. } => "session:ended".into(),
        SessionEvent::TextDelta { seq, .. } => format!("streaming:delta:{}", seq),
        SessionEvent::InteractionRequested {
            request_id,
            request,
        } => {
            format!("interaction:{}:{}", request.kind(), request_id)
        }
        SessionEvent::InteractionCompleted { request_id, .. } => {
            format!("interaction:completed:{}", request_id)
        }
        SessionEvent::DelegationSpawned { delegation_id, .. } => {
            format!("delegation:spawned:{}", delegation_id)
        }
        SessionEvent::DelegationCompleted { delegation_id, .. } => {
            format!("delegation:completed:{}", delegation_id)
        }
        SessionEvent::DelegationFailed { delegation_id, .. } => {
            format!("delegation:failed:{}", delegation_id)
        }
        SessionEvent::Custom { name, .. } => name.clone(),
        SessionEvent::Internal(inner) => inner.identifier(),
    }
}

/// Extract the raw payload content from a session event.
///
/// Returns the main content or data associated with the event before truncation.
fn payload_for_event(event: &SessionEvent) -> Option<String> {
    match event {
        SessionEvent::MessageReceived { content, .. } => Some(content.clone()),
        SessionEvent::AgentResponded { content, .. } => Some(content.clone()),
        SessionEvent::AgentThinking { thought } => Some(thought.clone()),
        SessionEvent::ToolCalled { args, .. } => Some(args.to_string()),
        SessionEvent::ToolCompleted { result, .. } => Some(result.clone()),
        SessionEvent::SessionStarted { .. } => None,
        SessionEvent::SessionEnded { reason } => Some(reason.clone()),
        SessionEvent::TextDelta { delta, .. } => Some(delta.clone()),
        SessionEvent::InteractionRequested { .. } => None,
        SessionEvent::InteractionCompleted { .. } => None,
        SessionEvent::DelegationSpawned { prompt, .. } => Some(prompt.clone()),
        SessionEvent::DelegationCompleted { result_summary, .. } => Some(result_summary.clone()),
        SessionEvent::DelegationFailed { error, .. } => Some(error.clone()),
        SessionEvent::Custom { payload, .. } => Some(payload.to_string()),
        SessionEvent::Internal(inner) => inner.payload_content(),
    }
}

/// Estimate the content length for token estimation.
///
/// Returns a character count representing the meaningful content size of the event.
fn estimate_content_len(event: &SessionEvent) -> usize {
    match event {
        SessionEvent::MessageReceived { content, .. } => content.len(),
        SessionEvent::AgentResponded { content, .. } => content.len(),
        SessionEvent::AgentThinking { thought } => thought.len(),
        SessionEvent::ToolCalled { args, .. } => args.to_string().len(),
        SessionEvent::ToolCompleted { result, error, .. } => {
            result.len() + error.as_ref().map(|e| e.len()).unwrap_or(0)
        }
        SessionEvent::SessionStarted { .. } => 100,
        SessionEvent::SessionEnded { reason } => reason.len(),
        SessionEvent::TextDelta { delta, .. } => delta.len(),
        SessionEvent::InteractionRequested { .. } => 100,
        SessionEvent::InteractionCompleted { .. } => 50,
        SessionEvent::DelegationSpawned { prompt, .. } => prompt.len(),
        SessionEvent::DelegationCompleted { result_summary, .. } => result_summary.len(),
        SessionEvent::DelegationFailed { error, .. } => error.len(),
        SessionEvent::Custom { payload, .. } => payload.to_string().len(),
        SessionEvent::Internal(inner) => inner.estimate_content_len(),
    }
}

/// Truncate a string to `max_len`, respecting UTF-8 char boundaries.
///
/// If the string is longer than `max_len`, it will be truncated at the nearest
/// valid UTF-8 character boundary.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a char boundary near max_len
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
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

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

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
            SessionEvent::internal(InternalSessionEvent::FileChanged {
                path: PathBuf::new(),
                kind: FileChangeKind::Created
            })
            .event_type(),
            "file_changed"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::FileDeleted {
                path: PathBuf::new()
            })
            .event_type(),
            "file_deleted"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::FileMoved {
                from: PathBuf::new(),
                to: PathBuf::new()
            })
            .event_type(),
            "file_moved"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::NoteParsed {
                path: PathBuf::new(),
                block_count: 0,
                payload: None,
            })
            .event_type(),
            "note_parsed"
        );
        // Storage events
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::EntityStored {
                entity_id: "".into(),
                entity_type: EntityType::Note
            })
            .event_type(),
            "entity_stored"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::EntityDeleted {
                entity_id: "".into(),
                entity_type: EntityType::Note
            })
            .event_type(),
            "entity_deleted"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
                entity_id: "".into(),
                block_count: 0
            })
            .event_type(),
            "blocks_updated"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::RelationStored {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            })
            .event_type(),
            "relation_stored"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::RelationDeleted {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            })
            .event_type(),
            "relation_deleted"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
                entity_id: "".into(),
                block_id: None,
                priority: Priority::Normal
            })
            .event_type(),
            "embedding_requested"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
                entity_id: "".into(),
                block_id: None,
                dimensions: 0,
                model: "".into()
            })
            .event_type(),
            "embedding_stored"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
                entity_id: "".into(),
                block_id: None,
                error: "".into()
            })
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

        let event = SessionEvent::internal(InternalSessionEvent::NoteParsed {
            path: PathBuf::from("/notes/test.md"),
            block_count: 5,
            payload: None,
        });
        assert_eq!(event.identifier(), "/notes/test.md");

        let event = SessionEvent::MessageReceived {
            content: "hello".into(),
            participant_id: "user".into(),
        };
        assert_eq!(event.identifier(), "message:user");

        // File events identifiers
        let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/changed.md"),
            kind: FileChangeKind::Modified,
        });
        assert_eq!(event.identifier(), "/notes/changed.md");

        let event = SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::from("/notes/deleted.md"),
        });
        assert_eq!(event.identifier(), "/notes/deleted.md");

        // FileMoved uses the "to" path as identifier
        let event = SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::from("/notes/old.md"),
            to: PathBuf::from("/notes/new.md"),
        });
        assert_eq!(event.identifier(), "/notes/new.md");

        // Storage events identifiers
        let event = SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        });
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::internal(InternalSessionEvent::EntityDeleted {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        });
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
            entity_id: "entities:note:test".into(),
            block_count: 5,
        });
        assert_eq!(event.identifier(), "entities:note:test");

        let event = SessionEvent::internal(InternalSessionEvent::RelationStored {
            from_id: "entities:note:a".into(),
            to_id: "entities:note:b".into(),
            relation_type: "wikilink".into(),
        });
        assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

        let event = SessionEvent::internal(InternalSessionEvent::RelationDeleted {
            from_id: "entities:note:a".into(),
            to_id: "entities:note:b".into(),
            relation_type: "wikilink".into(),
        });
        assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

        // EmbeddingRequested with block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            priority: Priority::High,
        });
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingRequested without block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: None,
            priority: Priority::Normal,
        });
        assert_eq!(event.identifier(), "entities:note:test");

        // EmbeddingStored with block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            dimensions: 384,
            model: "nomic-embed-text".into(),
        });
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingStored without block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "entities:note:test".into(),
            block_id: None,
            dimensions: 384,
            model: "nomic-embed-text".into(),
        });
        assert_eq!(event.identifier(), "entities:note:test");

        // EmbeddingFailed with block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            error: "provider timeout".into(),
        });
        assert_eq!(event.identifier(), "entities:note:test#block:0");

        // EmbeddingFailed without block_id
        let event = SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: None,
            error: "provider timeout".into(),
        });
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
        assert!(SessionEvent::internal(InternalSessionEvent::NoteParsed {
            path: PathBuf::new(),
            block_count: 0,
            payload: None,
        })
        .is_note_event());
        assert!(SessionEvent::internal(InternalSessionEvent::NoteCreated {
            path: PathBuf::new(),
            title: None
        })
        .is_note_event());
        assert!(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
            path: PathBuf::from("/notes/test.md"),
            existed: true,
        })
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
        assert!(
            SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
                id: "".into(),
                prompt: "".into()
            })
            .is_subagent_event()
        );

        // Streaming events
        assert!(SessionEvent::TextDelta {
            delta: "".into(),
            seq: 0
        }
        .is_streaming_event());

        // File events
        assert!(SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::new(),
            kind: FileChangeKind::Created
        })
        .is_file_event());
        assert!(SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::new()
        })
        .is_file_event());
        assert!(SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::new(),
            to: PathBuf::new()
        })
        .is_file_event());
        // File events are not note events
        assert!(!SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::new(),
            kind: FileChangeKind::Modified
        })
        .is_note_event());

        // Embedding events
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
                entity_id: "".into(),
                block_id: None,
                priority: Priority::Normal
            })
            .is_embedding_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
                entity_id: "".into(),
                block_id: None,
                dimensions: 0,
                model: "".into()
            })
            .is_embedding_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
                entity_id: "".into(),
                block_id: None,
                error: "".into()
            })
            .is_embedding_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingBatchComplete {
                entity_id: "".into(),
                count: 5,
                duration_ms: 100
            })
            .is_embedding_event()
        );
        // Non-embedding events
        assert!(!SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .is_embedding_event());

        // Storage events
        assert!(SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .is_storage_event());
        assert!(SessionEvent::internal(InternalSessionEvent::EntityDeleted {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .is_storage_event());
        assert!(SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
            entity_id: "".into(),
            block_count: 0
        })
        .is_storage_event());
        assert!(
            SessionEvent::internal(InternalSessionEvent::RelationStored {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            })
            .is_storage_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::RelationDeleted {
                from_id: "".into(),
                to_id: "".into(),
                relation_type: "".into()
            })
            .is_storage_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
                entity_id: "".into(),
                block_id: None,
                priority: Priority::Normal
            })
            .is_storage_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
                entity_id: "".into(),
                block_id: None,
                dimensions: 0,
                model: "".into()
            })
            .is_storage_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
                entity_id: "".into(),
                block_id: None,
                error: "".into()
            })
            .is_storage_event()
        );
        assert!(
            SessionEvent::internal(InternalSessionEvent::EmbeddingBatchComplete {
                entity_id: "".into(),
                count: 5,
                duration_ms: 100
            })
            .is_storage_event()
        );
        // Storage events are not note events
        assert!(!SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .is_note_event());

        // MCP events
        assert!(SessionEvent::internal(InternalSessionEvent::McpAttached {
            server: "".into(),
            tool_count: 0
        })
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
            SessionEvent::internal(InternalSessionEvent::SessionCompacted {
                summary: "summary".into(),
                new_file: test_path("new"),
            }),
            SessionEvent::SessionEnded {
                reason: "user closed".into(),
            },
            SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
                id: "sub1".into(),
                prompt: "do stuff".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
                id: "sub1".into(),
                result: "done".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::SubagentFailed {
                id: "sub1".into(),
                error: "failed".into(),
            }),
            SessionEvent::TextDelta {
                delta: "chunk".into(),
                seq: 1,
            },
            // File system events
            SessionEvent::internal(InternalSessionEvent::FileChanged {
                path: PathBuf::from("/notes/test.md"),
                kind: FileChangeKind::Created,
            }),
            SessionEvent::internal(InternalSessionEvent::FileChanged {
                path: PathBuf::from("/notes/test.md"),
                kind: FileChangeKind::Modified,
            }),
            SessionEvent::internal(InternalSessionEvent::FileDeleted {
                path: PathBuf::from("/notes/deleted.md"),
            }),
            SessionEvent::internal(InternalSessionEvent::FileMoved {
                from: PathBuf::from("/notes/old.md"),
                to: PathBuf::from("/notes/new.md"),
            }),
            // Note events
            SessionEvent::internal(InternalSessionEvent::NoteParsed {
                path: PathBuf::from("/notes/test.md"),
                block_count: 5,
                payload: None,
            }),
            SessionEvent::internal(InternalSessionEvent::NoteCreated {
                path: PathBuf::from("/notes/new.md"),
                title: Some("New Note".into()),
            }),
            SessionEvent::internal(InternalSessionEvent::NoteModified {
                path: PathBuf::from("/notes/test.md"),
                change_type: NoteChangeType::Content,
            }),
            SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: PathBuf::from("/notes/deleted.md"),
                existed: true,
            }),
            // Storage events
            SessionEvent::internal(InternalSessionEvent::EntityStored {
                entity_id: "entities:note:test".into(),
                entity_type: EntityType::Note,
            }),
            SessionEvent::internal(InternalSessionEvent::EntityDeleted {
                entity_id: "entities:note:test".into(),
                entity_type: EntityType::Note,
            }),
            SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
                entity_id: "entities:note:test".into(),
                block_count: 5,
            }),
            SessionEvent::internal(InternalSessionEvent::RelationStored {
                from_id: "entities:note:source".into(),
                to_id: "entities:note:target".into(),
                relation_type: "wikilink".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::RelationDeleted {
                from_id: "entities:note:source".into(),
                to_id: "entities:note:target".into(),
                relation_type: "wikilink".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
                entity_id: "entities:note:test".into(),
                block_id: None,
                priority: Priority::Normal,
            }),
            SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                priority: Priority::High,
            }),
            SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                dimensions: 384,
                model: "nomic-embed-text".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
                entity_id: "entities:note:test".into(),
                block_id: None,
                error: "provider timeout".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
                entity_id: "entities:note:test".into(),
                block_id: Some("block:0".into()),
                error: "rate limited".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::McpAttached {
                server: "crucible".into(),
                tool_count: 10,
            }),
            SessionEvent::internal(InternalSessionEvent::ToolDiscovered {
                name: "search".into(),
                source: ToolProvider::Mcp {
                    server: "crucible".into(),
                },
                schema: Some(serde_json::json!({"type": "object"})),
            }),
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
        let cloned = kind;
        let copied = kind;
        assert_eq!(kind, cloned);
        assert_eq!(kind, copied);
    }

    #[test]
    fn test_tool_provider() {
        assert_eq!(ToolProvider::default(), ToolProvider::Builtin);
        assert_eq!(format!("{}", ToolProvider::Lua), "lua");
        assert_eq!(format!("{}", ToolProvider::Lua), "lua");
        assert_eq!(
            format!(
                "{}",
                ToolProvider::Mcp {
                    server: "test".into()
                }
            ),
            "mcp:test"
        );
        assert_eq!(format!("{}", ToolProvider::Builtin), "builtin");
    }

    #[test]
    fn test_tool_call() {
        let test_file = test_path("test.txt");
        let test_file_str = test_file.to_string_lossy();
        let call = ToolCall::new("read_file", serde_json::json!({"path": test_file_str}))
            .with_call_id("call_123");

        assert_eq!(call.name, "read_file");
        assert_eq!(call.args["path"], test_file_str.as_ref());
        assert_eq!(call.call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_session_event_config() {
        let session_folder = test_path("session");
        let config = SessionEventConfig::new("test-session")
            .with_folder(&session_folder)
            .with_max_context_tokens(50_000)
            .with_system_prompt("You are helpful.");

        assert_eq!(config.session_id, "test-session");
        assert_eq!(config.folder, Some(session_folder));
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
        let cloned = entity_type;
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
        let cloned = priority;
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
        let created = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/new.md"),
            kind: FileChangeKind::Created,
        });
        assert_eq!(created.priority(), Priority::High);

        // FileChanged(Modified) → Normal
        let modified = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/existing.md"),
            kind: FileChangeKind::Modified,
        });
        assert_eq!(modified.priority(), Priority::Normal);

        // FileDeleted → Low
        let deleted = SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::from("/notes/old.md"),
        });
        assert_eq!(deleted.priority(), Priority::Low);

        // FileMoved → Normal
        let moved = SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::from("/notes/old.md"),
            to: PathBuf::from("/notes/new.md"),
        });
        assert_eq!(moved.priority(), Priority::Normal);

        // EmbeddingRequested → uses embedded priority
        let embedding_normal = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::Normal,
        });
        assert_eq!(embedding_normal.priority(), Priority::Normal);

        let embedding_high = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::High,
        });
        assert_eq!(embedding_high.priority(), Priority::High);

        let embedding_critical = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "test".into(),
            block_id: None,
            priority: Priority::Critical,
        });
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

        let entity_stored = SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "test".into(),
            entity_type: EntityType::Note,
        });
        assert_eq!(entity_stored.priority(), Priority::Normal);

        let custom = SessionEvent::Custom {
            name: "custom".into(),
            payload: JsonValue::Null,
        };
        assert_eq!(custom.priority(), Priority::Normal);
    }

    // ─────────────────────────────────────────────────────────────────────
    // Pre-event contract tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_pre_tool_call_event_type() {
        let event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
            name: "search".into(),
            args: serde_json::json!({"q": "rust"}),
        });
        assert_eq!(event.event_type(), "pre_tool_call");
        assert!(event.is_pre_event());
        assert!(!event.is_tool_event()); // Pre-events are separate from tool events
    }

    #[test]
    fn test_pre_parse_event_type() {
        let event = SessionEvent::internal(InternalSessionEvent::PreParse {
            path: PathBuf::from("/notes/test.md"),
        });
        assert_eq!(event.event_type(), "pre_parse");
        assert!(event.is_pre_event());
        assert!(!event.is_note_event()); // Pre-events are separate from note events
    }

    #[test]
    fn test_pre_llm_call_event_type() {
        let event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: "Hello".into(),
            model: "gpt-4".into(),
        });
        assert_eq!(event.event_type(), "pre_llm_call");
        assert!(event.is_pre_event());
    }

    #[test]
    fn test_pre_event_identifiers() {
        let tool_event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
            name: "bash".into(),
            args: serde_json::json!({"cmd": "ls"}),
        });
        assert_eq!(tool_event.identifier(), "pre:tool:bash");

        let parse_event = SessionEvent::internal(InternalSessionEvent::PreParse {
            path: PathBuf::from("/notes/test.md"),
        });
        assert_eq!(parse_event.identifier(), "pre:parse:/notes/test.md");

        let llm_event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: "Hello".into(),
            model: "gpt-4".into(),
        });
        assert_eq!(llm_event.identifier(), "pre:llm:gpt-4");
    }

    #[test]
    fn test_pre_event_serialization() {
        let event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
            name: "bash".into(),
            args: serde_json::json!({"cmd": "ls"}),
        });

        let json = serde_json::to_string(&event).unwrap();
        let restored: SessionEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event, restored);
    }

    #[test]
    fn test_all_pre_events_serialize() {
        let events = vec![
            SessionEvent::internal(InternalSessionEvent::PreToolCall {
                name: "search".into(),
                args: serde_json::json!({"q": "test"}),
            }),
            SessionEvent::internal(InternalSessionEvent::PreParse {
                path: PathBuf::from("/notes/test.md"),
            }),
            SessionEvent::internal(InternalSessionEvent::PreLlmCall {
                prompt: "Hello".into(),
                model: "claude-3".into(),
            }),
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    #[test]
    fn test_pre_event_priority() {
        // Pre-events default to Normal priority
        let pre_tool = SessionEvent::internal(InternalSessionEvent::PreToolCall {
            name: "search".into(),
            args: JsonValue::Null,
        });
        assert_eq!(pre_tool.priority(), Priority::Normal);

        let pre_parse = SessionEvent::internal(InternalSessionEvent::PreParse {
            path: PathBuf::from("/notes/test.md"),
        });
        assert_eq!(pre_parse.priority(), Priority::Normal);

        let pre_llm = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: "test".into(),
            model: "gpt-4".into(),
        });
        assert_eq!(pre_llm.priority(), Priority::Normal);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // AwaitingInput / InputType contract tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_input_type_default() {
        assert_eq!(InputType::default(), InputType::Message);
    }

    #[test]
    fn test_input_type_display() {
        assert_eq!(format!("{}", InputType::Message), "message");
        assert_eq!(format!("{}", InputType::Approval), "approval");
        assert_eq!(format!("{}", InputType::Selection), "selection");
    }

    #[test]
    fn test_input_type_serialization() {
        let message: InputType = serde_json::from_str("\"message\"").unwrap();
        assert_eq!(message, InputType::Message);

        let approval: InputType = serde_json::from_str("\"approval\"").unwrap();
        assert_eq!(approval, InputType::Approval);

        let selection: InputType = serde_json::from_str("\"selection\"").unwrap();
        assert_eq!(selection, InputType::Selection);
    }

    #[test]
    fn test_awaiting_input_event_type() {
        let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
            input_type: InputType::Message,
            context: None,
        });
        assert_eq!(event.event_type(), "awaiting_input");
    }

    #[test]
    fn test_awaiting_input_identifier() {
        let message_event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
            input_type: InputType::Message,
            context: None,
        });
        assert_eq!(message_event.identifier(), "await:message");

        let approval_event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
            input_type: InputType::Approval,
            context: Some("delete files".into()),
        });
        assert_eq!(approval_event.identifier(), "await:approval");
    }

    #[test]
    fn test_awaiting_input_serialization() {
        let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
            input_type: InputType::Approval,
            context: Some("Agent wants to delete files".into()),
        });

        let json = serde_json::to_string(&event).unwrap();
        let restored: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);

        // Verify JSON structure
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "awaiting_input");
        assert_eq!(parsed["input_type"], "approval");
        assert_eq!(parsed["context"], "Agent wants to delete files");
    }

    #[test]
    fn test_awaiting_input_not_pre_event() {
        // AwaitingInput is NOT a pre-event (it's a state change, not an interception point)
        let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
            input_type: InputType::Message,
            context: None,
        });
        assert!(!event.is_pre_event());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Daemon protocol event tests (SessionStateChanged, SessionPaused, SessionResumed, TerminalOutput)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_session_state_changed_event() {
        use crate::session::SessionState;

        let event = SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "chat-2025-01-08T1530-abc123".into(),
            state: SessionState::Paused,
            previous_state: Some(SessionState::Active),
        });

        assert_eq!(event.event_type(), "session_state_changed");
        assert!(event.is_lifecycle_event());
        assert_eq!(
            event.identifier(),
            "session:state_changed:chat-2025-01-08T1530-abc123"
        );

        // Verify serialization
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_session_paused_event() {
        let event = SessionEvent::internal(InternalSessionEvent::SessionPaused {
            session_id: "chat-2025-01-08T1530-abc123".into(),
        });

        assert_eq!(event.event_type(), "session_paused");
        assert!(event.is_lifecycle_event());
        assert_eq!(
            event.identifier(),
            "session:paused:chat-2025-01-08T1530-abc123"
        );

        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_session_resumed_event() {
        let event = SessionEvent::internal(InternalSessionEvent::SessionResumed {
            session_id: "chat-2025-01-08T1530-abc123".into(),
        });

        assert_eq!(event.event_type(), "session_resumed");
        assert!(event.is_lifecycle_event());
        assert_eq!(
            event.identifier(),
            "session:resumed:chat-2025-01-08T1530-abc123"
        );

        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_terminal_output_event() {
        let event = SessionEvent::internal(InternalSessionEvent::TerminalOutput {
            session_id: "chat-2025-01-08T1530-abc123".into(),
            stream: TerminalStream::Stdout,
            content_base64: "SGVsbG8gV29ybGQK".into(), // "Hello World\n"
        });

        assert_eq!(event.event_type(), "terminal_output");
        assert!(event.is_streaming_event());
        assert!(!event.is_lifecycle_event());
        assert_eq!(
            event.identifier(),
            "terminal:chat-2025-01-08T1530-abc123:stdout"
        );

        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_terminal_stream() {
        // Test default
        assert_eq!(TerminalStream::default(), TerminalStream::Stdout);

        // Test Display
        assert_eq!(format!("{}", TerminalStream::Stdout), "stdout");
        assert_eq!(format!("{}", TerminalStream::Stderr), "stderr");

        // Test serialization
        let stdout = TerminalStream::Stdout;
        let json = serde_json::to_string(&stdout).unwrap();
        assert_eq!(json, "\"stdout\"");

        let stderr = TerminalStream::Stderr;
        let json = serde_json::to_string(&stderr).unwrap();
        assert_eq!(json, "\"stderr\"");

        // Test deserialization
        let stdout: TerminalStream = serde_json::from_str("\"stdout\"").unwrap();
        assert_eq!(stdout, TerminalStream::Stdout);

        let stderr: TerminalStream = serde_json::from_str("\"stderr\"").unwrap();
        assert_eq!(stderr, TerminalStream::Stderr);
    }

    #[test]
    fn test_daemon_protocol_events_serialize() {
        use crate::session::SessionState;

        let events = vec![
            SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
                session_id: "chat-test".into(),
                state: SessionState::Active,
                previous_state: None,
            }),
            SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
                session_id: "chat-test".into(),
                state: SessionState::Paused,
                previous_state: Some(SessionState::Active),
            }),
            SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
                session_id: "chat-test".into(),
                state: SessionState::Compacting,
                previous_state: Some(SessionState::Active),
            }),
            SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
                session_id: "chat-test".into(),
                state: SessionState::Ended,
                previous_state: Some(SessionState::Active),
            }),
            SessionEvent::internal(InternalSessionEvent::SessionPaused {
                session_id: "agent-test".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::SessionResumed {
                session_id: "agent-test".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::TerminalOutput {
                session_id: "workflow-test".into(),
                stream: TerminalStream::Stdout,
                content_base64: "dGVzdA==".into(),
            }),
            SessionEvent::internal(InternalSessionEvent::TerminalOutput {
                session_id: "workflow-test".into(),
                stream: TerminalStream::Stderr,
                content_base64: "ZXJyb3I=".into(),
            }),
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // New method tests: type_name, summary, payload, estimate_tokens
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_type_name() {
        // Test a few representative variants
        assert_eq!(
            SessionEvent::MessageReceived {
                content: "".into(),
                participant_id: "".into()
            }
            .type_name(),
            "MessageReceived"
        );
        assert_eq!(
            SessionEvent::ToolCalled {
                name: "".into(),
                args: JsonValue::Null
            }
            .type_name(),
            "ToolCalled"
        );
        assert_eq!(
            SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
                session_id: "".into(),
                state: crate::session::SessionState::Active,
                previous_state: None,
            })
            .type_name(),
            "SessionStateChanged"
        );
        assert_eq!(
            SessionEvent::Custom {
                name: "".into(),
                payload: JsonValue::Null
            }
            .type_name(),
            "Custom"
        );
    }

    #[test]
    fn test_summary() {
        // Test MessageReceived summary
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let summary = event.summary(100);
        assert!(summary.contains("from=user"));
        assert!(summary.contains("content_len=11"));

        // Test ToolCalled summary
        let event = SessionEvent::ToolCalled {
            name: "search".into(),
            args: serde_json::json!({"query": "test"}),
        };
        let summary = event.summary(100);
        assert!(summary.contains("tool=search"));
        assert!(summary.contains("args_size="));

        // Test truncation in summary
        let event = SessionEvent::SessionEnded {
            reason: "This is a very long reason that should be truncated when max_len is small"
                .into(),
        };
        let summary = event.summary(20);
        assert!(summary.contains("reason="));
        // The truncated reason should be <= 20 chars
        assert!(summary.len() < 50);
    }

    #[test]
    fn test_payload() {
        // Test MessageReceived payload
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let payload = event.payload(100);
        assert_eq!(payload, Some("Hello world".to_string()));

        // Test SessionStarted has no payload
        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::default(),
        };
        let payload = event.payload(100);
        assert_eq!(payload, None);

        // Test truncation
        let event = SessionEvent::MessageReceived {
            content: "This is a long message that should be truncated".into(),
            participant_id: "user".into(),
        };
        let payload = event.payload(10);
        assert!(payload.is_some());
        assert!(payload.unwrap().len() <= 10);
    }

    #[test]
    fn test_estimate_tokens() {
        // Test MessageReceived token estimate
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(), // 11 chars -> ~3 tokens + 10 overhead
            participant_id: "user".into(),
        };
        let tokens = event.estimate_tokens();
        assert!(tokens >= 11); // At least 10 overhead + 1 minimum
        assert!(tokens < 20); // Should be reasonable

        // Test SessionStarted fixed overhead
        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::default(),
        };
        let tokens = event.estimate_tokens();
        assert_eq!(tokens, 100 / 4 + 10); // 100 fixed + overhead

        // Test small metadata events
        let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/test.md"),
            kind: FileChangeKind::Modified,
        });
        let tokens = event.estimate_tokens();
        assert_eq!(tokens, 50 / 4 + 10); // 50 fixed + overhead
    }

    #[test]
    fn test_delegation_spawned_serde() {
        // Test DelegationSpawned variant serializes/deserializes correctly
        let event = SessionEvent::DelegationSpawned {
            delegation_id: "deleg-123".into(),
            prompt: "Analyze this code".into(),
            parent_session_id: "parent-456".into(),
            target_agent: None,
        };

        // Test JSON round-trip
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, deserialized);

        // Verify JSON structure
        let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(json_obj["type"], "delegation_spawned");
        assert_eq!(json_obj["delegation_id"], "deleg-123");
        assert_eq!(json_obj["parent_session_id"], "parent-456");
    }

    #[test]
    fn test_delegation_completed_serde() {
        // Test DelegationCompleted variant serializes/deserializes correctly
        let event = SessionEvent::DelegationCompleted {
            delegation_id: "deleg-123".into(),
            result_summary: "Analysis complete: 5 issues found".into(),
            parent_session_id: "parent-456".into(),
        };

        // Test JSON round-trip
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, deserialized);

        // Verify JSON structure
        let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(json_obj["type"], "delegation_completed");
        assert_eq!(json_obj["delegation_id"], "deleg-123");
        assert_eq!(json_obj["parent_session_id"], "parent-456");
    }

    #[test]
    fn test_delegation_failed_serde() {
        // Test DelegationFailed variant serializes/deserializes correctly
        let event = SessionEvent::DelegationFailed {
            delegation_id: "deleg-123".into(),
            error: "Timeout after 30s".into(),
            parent_session_id: "parent-456".into(),
        };

        // Test JSON round-trip
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, deserialized);

        // Verify JSON structure
        let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(json_obj["type"], "delegation_failed");
        assert_eq!(json_obj["delegation_id"], "deleg-123");
        assert_eq!(json_obj["parent_session_id"], "parent-456");
    }

    #[test]
    fn test_subagent_variants_still_deserialize() {
        // Verify backwards compatibility: existing SubagentSpawned/Completed/Failed still work
        let subagent_spawned = SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
            id: "sub-123".into(),
            prompt: "Do something".into(),
        });
        let json = serde_json::to_string(&subagent_spawned).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(subagent_spawned, deserialized);

        let subagent_completed = SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
            id: "sub-123".into(),
            result: "Done".into(),
        });
        let json = serde_json::to_string(&subagent_completed).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(subagent_completed, deserialized);

        let subagent_failed = SessionEvent::internal(InternalSessionEvent::SubagentFailed {
            id: "sub-123".into(),
            error: "Failed".into(),
        });
        let json = serde_json::to_string(&subagent_failed).expect("serialize");
        let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(subagent_failed, deserialized);
    }

    #[test]
    fn test_truncate_helper() {
        // Test short string (no truncation needed)
        let short = "hello";
        assert_eq!(super::truncate(short, 10), "hello");

        // Test exact length
        let exact = "hello";
        assert_eq!(super::truncate(exact, 5), "hello");

        // Test truncation
        let long = "hello world";
        assert_eq!(super::truncate(long, 5), "hello");

        // Test UTF-8 boundary handling
        let utf8 = "hello\u{00e9}world"; // e with accent
        let truncated = super::truncate(utf8, 6);
        assert!(truncated.len() <= 6);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}

impl<'de> Deserialize<'de> for SessionEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // Deserialize to a raw Value first
        let value = serde_json::Value::deserialize(deserializer)?;

        // Extract the type field
        let type_str = value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;

        // Known SessionEvent variants (non-Internal)
        const KNOWN_VARIANTS: &[&str] = &[
            "message_received",
            "agent_responded",
            "agent_thinking",
            "tool_called",
            "tool_completed",
            "session_started",
            "session_ended",
            "text_delta",
            "interaction_requested",
            "interaction_completed",
            "delegation_spawned",
            "delegation_completed",
            "delegation_failed",
            "custom",
        ];

        if KNOWN_VARIANTS.contains(&type_str) {
            // For known SessionEvent variants, use serde_json to deserialize
            serde_json::from_value::<SessionEventHelper>(value)
                .map_err(|e| D::Error::custom(format!("failed to deserialize SessionEvent: {}", e)))
                .map(|helper| helper.into())
        } else {
            // Try to deserialize as InternalSessionEvent
            let type_str_owned = type_str.to_string();
            serde_json::from_value::<InternalSessionEvent>(value)
                .map_err(|e| {
                    D::Error::custom(format!("unknown event type '{}': {}", type_str_owned, e))
                })
                .map(|inner| SessionEvent::Internal(Box::new(inner)))
        }
    }
}

/// Helper enum for deserializing known SessionEvent variants.
/// This avoids infinite recursion when deserializing the main SessionEvent enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum SessionEventHelper {
    MessageReceived {
        content: String,
        participant_id: String,
    },
    AgentResponded {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    AgentThinking {
        thought: String,
    },
    ToolCalled {
        name: String,
        args: JsonValue,
    },
    ToolCompleted {
        name: String,
        result: String,
        #[serde(default)]
        error: Option<String>,
    },
    SessionStarted {
        config: SessionEventConfig,
    },
    SessionEnded {
        reason: String,
    },
    TextDelta {
        delta: String,
        seq: u64,
    },
    InteractionRequested {
        request_id: String,
        request: crate::interaction::InteractionRequest,
    },
    InteractionCompleted {
        request_id: String,
        response: crate::interaction::InteractionResponse,
    },
    DelegationSpawned {
        delegation_id: String,
        prompt: String,
        parent_session_id: String,
        #[serde(default)]
        target_agent: Option<String>,
    },
    DelegationCompleted {
        delegation_id: String,
        result_summary: String,
        parent_session_id: String,
    },
    DelegationFailed {
        delegation_id: String,
        error: String,
        parent_session_id: String,
    },
    Custom {
        name: String,
        payload: JsonValue,
    },
}

impl From<SessionEventHelper> for SessionEvent {
    fn from(helper: SessionEventHelper) -> Self {
        match helper {
            SessionEventHelper::MessageReceived {
                content,
                participant_id,
            } => SessionEvent::MessageReceived {
                content,
                participant_id,
            },
            SessionEventHelper::AgentResponded {
                content,
                tool_calls,
            } => SessionEvent::AgentResponded {
                content,
                tool_calls,
            },
            SessionEventHelper::AgentThinking { thought } => {
                SessionEvent::AgentThinking { thought }
            }
            SessionEventHelper::ToolCalled { name, args } => {
                SessionEvent::ToolCalled { name, args }
            }
            SessionEventHelper::ToolCompleted {
                name,
                result,
                error,
            } => SessionEvent::ToolCompleted {
                name,
                result,
                error,
            },
            SessionEventHelper::SessionStarted { config } => {
                SessionEvent::SessionStarted { config }
            }
            SessionEventHelper::SessionEnded { reason } => SessionEvent::SessionEnded { reason },
            SessionEventHelper::TextDelta { delta, seq } => SessionEvent::TextDelta { delta, seq },
            SessionEventHelper::InteractionRequested {
                request_id,
                request,
            } => SessionEvent::InteractionRequested {
                request_id,
                request,
            },
            SessionEventHelper::InteractionCompleted {
                request_id,
                response,
            } => SessionEvent::InteractionCompleted {
                request_id,
                response,
            },
            SessionEventHelper::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                target_agent,
            } => SessionEvent::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                target_agent,
            },
            SessionEventHelper::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            } => SessionEvent::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            },
            SessionEventHelper::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            } => SessionEvent::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            },
            SessionEventHelper::Custom { name, payload } => SessionEvent::Custom { name, payload },
        }
    }
}
