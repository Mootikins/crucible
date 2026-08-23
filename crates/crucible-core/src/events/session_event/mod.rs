//! Session event types for the Crucible event system.
//!
//! This module defines the `SessionEvent` enum the Lua bridge and the
//! file-watch bus dispatch on. Events are categorized by their source:
//!
//! - **User events**: Messages from participants
//! - **Interaction events**: Structured prompts waiting on a client
//! - **Custom events**: Named events with a JSON payload
//! - **Internal events**: File, note and enrichment signals ([`InternalSessionEvent`])
//!
//! # Example
//!
//! ```
//! use crucible_core::events::{SessionEvent, InternalSessionEvent, NoteChangeType};
//! use std::path::PathBuf;
//!
//! let event = SessionEvent::internal(InternalSessionEvent::NoteModified {
//!     path: PathBuf::from("/notes/test.md"),
//!     change_type: NoteChangeType::Content,
//! });
//!
//! assert_eq!(event.event_type(), "note_modified");
//! ```

pub mod internal;
pub mod types;

#[cfg(test)]
mod tests;

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::text::truncate_bytes;

pub use internal::InternalSessionEvent;
pub use types::{FileChangeKind, NoteChangeType};

/// The ten scripting names the transport vocabulary also has a payload for.
///
/// Both vocabularies name these events, and they spell them differently:
/// `crucible.on("tool_called")` and the web's `tool_call` SSE frame are the
/// same event. Nothing said so before, so a plugin author reading the SSE
/// stream learned the wrong name.
///
/// A closed set rather than a `&'static str` on either side, because the two
/// sides used to be two independent lists of string literals and the only thing
/// holding them together was a test that `include_str!`d two files and sliced
/// between literal markers. Now [`SessionEvent::event_type`] and
/// [`TurnPayload::as_scripting_event`](crate::protocol::session_events::TurnPayload::as_scripting_event)
/// read the same constant, so they cannot disagree and the scan is gone.
///
/// This is the **overlap**, not the whole scripting vocabulary: an event with
/// no transport payload — `custom`, every [`InternalSessionEvent`] but one —
/// keeps its literal in `event_type`. Nor is it the whole transport
/// vocabulary: only three of the ten still have a [`SessionEvent`] variant.
/// The other seven were scripting variants nothing ever constructed; plan
/// T3-B7 removed them, and the names stay here because
/// `as_scripting_event` still reports them for the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum ScriptingEvent {
    /// A participant sent a message. Transport: `user_message`.
    MessageReceived,
    /// A token arrived. Transport: `text_delta`.
    TextDelta,
    /// Reasoning text arrived. Transport: `thinking`.
    AgentThinking,
    /// The agent finished a message. Transport: `message_complete`.
    AgentResponded,
    /// A tool was called. Transport: `tool_call`.
    ToolCalled,
    /// A tool returned. Transport: `tool_result`.
    ToolCompleted,
    /// The session ended. Transport: `ended`.
    SessionEnded,
    /// A prompt is waiting on the client. Transport: `interaction_requested`.
    InteractionRequested,
    /// A prompt was answered. Transport: `interaction_completed`.
    InteractionCompleted,
    /// Precognition finished retrieving. Transport: `precognition_complete`.
    PrecognitionComplete,
}

impl ScriptingEvent {
    /// Every variant. `every_scripting_event_variant_is_listed` proves it.
    pub const ALL: &'static [Self] = &[
        Self::MessageReceived,
        Self::TextDelta,
        Self::AgentThinking,
        Self::AgentResponded,
        Self::ToolCalled,
        Self::ToolCompleted,
        Self::SessionEnded,
        Self::InteractionRequested,
        Self::InteractionCompleted,
        Self::PrecognitionComplete,
    ];

    /// The name a Lua handler registers for and reads off the event.
    ///
    /// **No wildcard arm, ever.** A new variant must fail to compile until
    /// someone names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MessageReceived => "message_received",
            Self::TextDelta => "text_delta",
            Self::AgentThinking => "agent_thinking",
            Self::AgentResponded => "agent_responded",
            Self::ToolCalled => "tool_called",
            Self::ToolCompleted => "tool_completed",
            Self::SessionEnded => "session_ended",
            Self::InteractionRequested => "interaction_requested",
            Self::InteractionCompleted => "interaction_completed",
            Self::PrecognitionComplete => "precognition_complete",
        }
    }
}

impl std::fmt::Display for ScriptingEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Events that flow through a session — the **scripting** vocabulary.
///
/// This type is **not** wire-facing. Nothing serializes it onto the RPC wire;
/// its one serialization target is Lua tables
/// (`crucible-lua/src/handlers/conversion.rs`). Its other use is
/// dispatch on the file-watch bus (`events::emitter`).
///
/// The transport vocabulary is
/// [`SessionEventPayload`](crate::protocol::session_events::SessionEventPayload),
/// carried by `SessionEventMessage`. The two are disjoint by design and spell
/// some of the same events differently; see
/// [`TurnPayload::as_scripting_event`](crate::protocol::session_events::TurnPayload::as_scripting_event)
/// for the mapping, and that module's doc for why unifying them is not on the
/// table.
///
/// Internal pipeline events live in [`InternalSessionEvent`] and are wrapped via
/// the `Internal` variant for reactor dispatch.
///
/// This enum once mirrored the transport vocabulary with eleven more variants
/// (`ToolCalled`, `TextDelta`, `DelegationSpawned`, ...). No producer existed
/// for any of them: the daemon raises Lua stages as `Custom` events and the
/// turn loop speaks `SessionEventMessage`. Plan T3-B7 removed them.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum SessionEvent {
    /// Message received from a participant.
    MessageReceived {
        /// The message content.
        content: String,
        /// Identifier of the participant who sent the message.
        participant_id: String,
    },

    /// Agent/tool requests structured user interaction.
    InteractionRequested {
        /// Unique ID for correlating request with response.
        request_id: String,
        /// The interaction request details.
        request: crate::interaction::InteractionRequest,
    },

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
    /// - Handler registration (e.g., `bus.on("message_received", ...)`)
    /// - Event filtering in queries
    /// - Logging and debugging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::MessageReceived { .. } => ScriptingEvent::MessageReceived.as_str(),
            Self::InteractionRequested { .. } => ScriptingEvent::InteractionRequested.as_str(),
            Self::Custom { .. } => "custom",
            Self::Internal(inner) => inner.event_type(),
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
    ///
    /// let event = SessionEvent::MessageReceived {
    ///     content: "hi".into(),
    ///     participant_id: "user".into(),
    /// };
    /// assert_eq!(event.type_name(), "MessageReceived");
    /// ```
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::MessageReceived { .. } => "MessageReceived",
            Self::InteractionRequested { .. } => "InteractionRequested",
            Self::Custom { .. } => "Custom",
            Self::Internal(inner) => inner.type_name(),
        }
    }

    /// Get a summary of this event's content.
    ///
    /// Returns a concise string describing the event's key fields, suitable for
    /// logging and debugging. Free-text fields are cut to `max_len` characters.
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::events::SessionEvent;
    /// use serde_json::json;
    ///
    /// let event = SessionEvent::Custom {
    ///     name: "tool_called".into(),
    ///     payload: json!({"tool": "search"}),
    /// };
    /// let summary = event.summary(100);
    /// assert!(summary.contains("name=tool_called"));
    /// ```
    pub fn summary(&self, max_len: usize) -> String {
        match self {
            Self::MessageReceived {
                content,
                participant_id,
            } => {
                format!("from={}, content_len={}", participant_id, content.len())
            }
            Self::InteractionRequested {
                request_id,
                request,
            } => {
                format!("id={}, kind={}", request_id, request.kind())
            }
            Self::Custom { name, payload } => {
                format!(
                    "name={}, payload_size={}",
                    truncate_bytes(name, max_len),
                    payload.to_string().len()
                )
            }
            Self::Internal(inner) => inner.summary(max_len),
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
