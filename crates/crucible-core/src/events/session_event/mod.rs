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
//! use crate::events::{SessionEvent, NoteChangeType};
//! use std::path::PathBuf;
//!
//! let event = SessionEvent::NoteModified {
//!     path: PathBuf::from("/notes/test.md"),
//!     change_type: NoteChangeType::Content,
//! };
//!
//! assert!(event.category() == EventCategory::Note);
//! assert_eq!(event.event_type(), "note_modified");
//! ```

// Submodules for logical organization
mod deserialize;
pub mod display;
pub mod helpers;
pub mod internal;
pub mod payloads;
pub mod tool_call;
pub mod types;

#[cfg(test)]
mod tests;

use serde::Serialize;
use serde_json::Value as JsonValue;

use helpers::{estimate_content_len, identifier_for_event, payload_for_event, truncate};

pub use internal::InternalSessionEvent;
pub use payloads::{NotePayload, SessionEventConfig};
pub use tool_call::ToolCall;
pub use types::{
    EntityType, EventCategory, FileChangeKind, InputType, NoteChangeType, Priority, TerminalStream,
    ToolProvider,
};

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
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
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

    /// Broad classification used for filtering events by concern.
    ///
    /// Each event belongs to exactly one category; see [`EventCategory`] for the
    /// tiebreak rule when an event could plausibly fit multiple categories.
    pub fn category(&self) -> EventCategory {
        match self {
            Self::MessageReceived { .. } => EventCategory::Message,
            Self::AgentResponded { .. } | Self::AgentThinking { .. } => EventCategory::Agent,
            Self::ToolCalled { .. } | Self::ToolCompleted { .. } => EventCategory::Tool,
            Self::SessionStarted { .. } | Self::SessionEnded { .. } => EventCategory::Lifecycle,
            Self::DelegationSpawned { .. }
            | Self::DelegationCompleted { .. }
            | Self::DelegationFailed { .. } => EventCategory::Delegation,
            Self::TextDelta { .. } => EventCategory::Streaming,
            Self::InteractionRequested { .. } | Self::InteractionCompleted { .. } => {
                EventCategory::Interaction
            }
            Self::Custom { .. } => EventCategory::Custom,
            Self::Internal(inner) => inner.category(),
        }
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
    /// use crate::events::{SessionEvent, InternalSessionEvent, FileChangeKind, Priority};
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
    /// use crate::events::SessionEvent;
    /// use serde_json::Value as JsonValue;
    ///
    /// let event = SessionEvent::ToolCalled {
    ///     name: "search".into(),
    ///     args: JsonValue::Null,
    ///     description: None,
    ///     source: None,
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
    /// use crate::events::SessionEvent;
    /// use serde_json::Value as JsonValue;
    ///
    /// let event = SessionEvent::ToolCalled {
    ///     name: "search".into(),
    ///     args: JsonValue::String("query".into()),
    ///     description: None,
    ///     source: None,
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
            Self::ToolCalled { name, args, .. } => {
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
    /// use crate::events::SessionEvent;
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
    /// use crate::events::SessionEvent;
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

impl Default for SessionEvent {
    fn default() -> Self {
        Self::Custom {
            name: "default".into(),
            payload: JsonValue::Null,
        }
    }
}
