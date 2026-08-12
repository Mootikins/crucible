//! The typed payload contract for `SessionEventMessage`.
//!
//! [`SessionEventMessage`](crate::protocol::SessionEventMessage) carries
//! `event: String` + `data: Value`. That pair is the *transport*; this module is
//! the *contract*. [`SessionEventPayload`] is a constructor and a view over the
//! same two fields, not a replacement for them:
//!
//! ```text
//! SessionEventMessage::typed(id, TurnPayload::TextDelta { .. })  →  {event, data}
//! msg.payload()                                                 →  SessionEventPayload
//! ```
//!
//! **Adjacent tagging is the whole trick.** Every group enum carries
//! `#[serde(tag = "event", content = "data")]`, so
//! `serde_json::to_value(&TurnPayload::TextDelta { content })` is exactly
//! `{"event":"text_delta","data":{"content":"…"}}` — the two envelope fields,
//! byte-identical to what the hand-written `json!` constructors produced. Wire
//! compatibility is therefore *structural*, not asserted: recorded fixtures,
//! persisted `session.jsonl`, and every existing subscriber keep working, and
//! there is no new format for a log to be in.
//!
//! # The groups
//!
//! | Group | Count | Module |
//! |---|---|---|
//! | [`TurnPayload`] | 16 | [`turn`] |
//! | [`SetupPayload`] | 7 | [`setup`] |
//! | [`SettingsPayload`] | 18 | [`settings`] |
//! | [`JobPayload`] | 7 | [`lifecycle`] |
//! | [`ReviewPayload`] | 3 | [`lifecycle`] |
//! | [`NotificationPayload`] | 2 | [`lifecycle`] |
//! | [`WorkflowPayload`] | 8 | [`lifecycle`] |
//! | [`SystemPayload`] | 10 | [`lifecycle`] |
//!
//! Groups rather than one flat 71-variant enum because exhaustive matching over
//! 71 variants in nine consumers is 639 arms — worse than the untyped code it
//! replaces. Grouping makes the *interesting* set exhaustive and the rest one
//! arm.
//!
//! The outer enum has **no serde derive**. `#[serde(untagged)]` would work and
//! would also let a malformed payload silently deserialize as a *different*
//! group, with a useless error message. [`SessionEventPayload::from_wire`]
//! dispatches on the name explicitly instead.
//!
//! Payloadless events use **empty struct variants** (`WorkflowCompleted {}`),
//! never unit variants: a unit variant under adjacent tagging omits `data`,
//! which surfaces as `null` where today's wire has `{}`.
//!
//! # Two vocabularies, not one
//!
//! This is the **transport** vocabulary. [`SessionEvent`](crate::events::SessionEvent)
//! is the **scripting** vocabulary — Reactor dispatch, Lua tables, and markdown
//! session logs. They are disjoint by design and neither is a subset of the
//! other:
//!
//! | | `SessionEventPayload` | `SessionEvent` |
//! |---|---|---|
//! | Serialization target | RPC socket, SSE, `session.jsonl`, `assets/fixtures/*.jsonl` | markdown session logs, Lua tables |
//! | Back-compat constraint | recorded fixtures and persisted sessions on disk | `handlers/*.lua` in user kilns |
//! | Vocabulary size | 71 | 14 wire-ish + 41 internal |
//! | `tool_call` is called | `tool_call` | `tool_called` |
//! | `tool_result` is called | `tool_result` | `tool_completed` |
//! | `message_complete` is called | `message_complete` | `agent_responded` |
//!
//! Unifying them means either putting 41 internal-only variants on the wire or
//! taking 41 hooks away from Lua. [`TurnPayload::as_scripting_event`] makes the
//! ten-name overlap explicit and tested instead.
//!
//! # The reader stays tolerant; the writer becomes typed
//!
//! There is no version gate and no rewrite pass over
//! `~/.crucible/sessions/**/session.jsonl`. The typed writer introduces no new
//! format (see above), sessions are the user's plaintext data, and
//! `session.jsonl` legitimately holds two shapes — `LogEvent` lines from
//! `inject_context` and `fork`, wire-envelope lines from the broadcast path.
//! The asymmetry is correct, not debt.

pub mod lifecycle;
pub mod settings;
pub mod setup;
pub mod turn;

#[cfg(test)]
mod tests;

pub use lifecycle::{
    JobPayload, NotificationPayload, ReviewPayload, SystemPayload, WorkflowPayload,
};
pub use settings::SettingsPayload;
pub use setup::{
    ContextLimitResolvedPayload, ContextLimitSource, KilnNotesIndexedPayload,
    McpServersReadyPayload, PluginsDiscoveredPayload, ProvidersListedPayload,
    SessionInitializedPayload, SetupPayload, WorkspaceIndexedPayload,
};
pub use turn::{ToolResultBody, TurnPayload};

use serde_json::Value;

/// A decoded session-event payload: one of eight groups.
///
/// Construct with [`SessionEventMessage::typed`](crate::protocol::SessionEventMessage::typed);
/// read with [`SessionEventMessage::payload`](crate::protocol::SessionEventMessage::payload).
#[derive(Clone, Debug)]
pub enum SessionEventPayload {
    Turn(TurnPayload),
    Setup(SetupPayload),
    Settings(SettingsPayload),
    Job(JobPayload),
    Review(ReviewPayload),
    Notification(NotificationPayload),
    Workflow(WorkflowPayload),
    System(SystemPayload),
}

/// Which group an event name belongs to.
///
/// The one place a wire name maps to a Rust type. `Group::of` returning `None`
/// is the forward-compatibility path: an event a newer daemon minted, or a name
/// only a recording contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Turn,
    Setup,
    Settings,
    Job,
    Review,
    Notification,
    Workflow,
    System,
}

impl Group {
    pub fn of(event: &str) -> Option<Self> {
        Some(match event {
            // Turn (16)
            "user_message"
            | "text_delta"
            | "thinking"
            | "segment_complete"
            | "message_complete"
            | "tool_call"
            | "tool_call_args_update"
            | "tool_call_diff_update"
            | "tool_result"
            | "ended"
            | "interaction_requested"
            | "interaction_completed"
            | "injection_pending"
            | "context_injected"
            | "precognition_complete"
            | "post_llm_call" => Self::Turn,
            // Setup (7)
            "session_initialized"
            | "providers_listed"
            | "context_limit_resolved"
            | "workspace_indexed"
            | "kiln_notes_indexed"
            | "plugins_discovered"
            | "mcp_servers_ready" => Self::Setup,
            // Settings (18)
            "model_switched"
            | "mode_changed"
            | "scope_changed"
            | "title_changed"
            | "thinking_budget_changed"
            | "system_prompt_changed"
            | "precognition_toggled"
            | "precognition_results_changed"
            | "temperature_changed"
            | "max_tokens_changed"
            | "max_iterations_changed"
            | "execution_timeout_changed"
            | "context_budget_changed"
            | "autocompact_threshold_changed"
            | "context_strategy_changed"
            | "context_window_changed"
            | "output_validation_changed"
            | "validation_retries_changed" => Self::Settings,
            // Job (7)
            "delegation_spawned"
            | "delegation_completed"
            | "delegation_failed"
            | "bash_job_spawned"
            | "bash_job_completed"
            | "bash_job_failed"
            | "background_job_completed" => Self::Job,
            // Review (3)
            "review_changed" | "review_gate" | "session_undo" => Self::Review,
            // Notification (2)
            "notification_added" | "notification_dismissed" => Self::Notification,
            // Workflow (8)
            "workflow.step_started"
            | "workflow.step_completed"
            | "workflow.gate_reached"
            | "workflow.gate_approved"
            | "workflow.completed"
            | "workflow.assessed"
            | "workflow.failed"
            | "workflow.cancelled" => Self::Workflow,
            // System (10)
            "file_changed"
            | "file_deleted"
            | "file_moved"
            | "classification_required"
            | "process_start"
            | "process_progress"
            | "process_complete"
            | "ui_style_changed"
            | "stream_gap"
            | "webhook:received"
            | "replay_complete" => Self::System,
            _ => return None,
        })
    }
}

/// Every wire name this build knows, in one list.
///
/// Hand-maintained alongside `Group::of` while the enum variants are the source
/// of truth, so `tests::event_names_match_the_payload_enums` scans the group
/// enums and compares. Drift makes a real event look like a foreign one: the
/// consumer takes its passthrough arm and the feature silently disappears.
pub const EVENT_NAMES: &[&str] = &[
    // Turn
    "user_message",
    "text_delta",
    "thinking",
    "segment_complete",
    "message_complete",
    "tool_call",
    "tool_call_args_update",
    "tool_call_diff_update",
    "tool_result",
    "ended",
    "interaction_requested",
    "interaction_completed",
    "injection_pending",
    "context_injected",
    "precognition_complete",
    "post_llm_call",
    // Setup
    "session_initialized",
    "providers_listed",
    "context_limit_resolved",
    "workspace_indexed",
    "kiln_notes_indexed",
    "plugins_discovered",
    "mcp_servers_ready",
    // Settings
    "model_switched",
    "mode_changed",
    "scope_changed",
    "title_changed",
    "thinking_budget_changed",
    "system_prompt_changed",
    "precognition_toggled",
    "precognition_results_changed",
    "temperature_changed",
    "max_tokens_changed",
    "max_iterations_changed",
    "execution_timeout_changed",
    "context_budget_changed",
    "autocompact_threshold_changed",
    "context_strategy_changed",
    "context_window_changed",
    "output_validation_changed",
    "validation_retries_changed",
    // Job
    "delegation_spawned",
    "delegation_completed",
    "delegation_failed",
    "bash_job_spawned",
    "bash_job_completed",
    "bash_job_failed",
    "background_job_completed",
    // Review
    "review_changed",
    "review_gate",
    "session_undo",
    // Notification
    "notification_added",
    "notification_dismissed",
    // Workflow
    "workflow.step_started",
    "workflow.step_completed",
    "workflow.gate_reached",
    "workflow.gate_approved",
    "workflow.completed",
    "workflow.assessed",
    "workflow.failed",
    "workflow.cancelled",
    // System
    "file_changed",
    "file_deleted",
    "file_moved",
    "classification_required",
    "process_start",
    "process_progress",
    "process_complete",
    "ui_style_changed",
    "stream_gap",
    "webhook:received",
    "replay_complete",
];

/// Why a payload would not decode.
///
/// thiserror at the boundary, per the crate's error convention: callers match on
/// `UnknownEvent` (pass the raw `{event, data}` through) versus
/// `MalformedPayload` (warn) and behave differently. Both carry the raw name, so
/// nothing an unknown event knows is lost — which a `#[serde(other)]` unit
/// variant could not manage.
#[derive(Debug, thiserror::Error)]
pub enum EventDecodeError {
    #[error("unknown session event `{event}`")]
    UnknownEvent { event: String },
    #[error("malformed payload for session event `{event}`")]
    MalformedPayload {
        event: String,
        #[source]
        source: serde_json::Error,
    },
}

impl SessionEventPayload {
    /// The `{event, data}` pair this payload is.
    ///
    /// Infallible by construction: adjacent tagging always produces an object
    /// with a string tag, and no group nests a map with non-string keys.
    pub fn to_wire(&self) -> (String, Value) {
        let v = match self {
            Self::Turn(p) => serde_json::to_value(p),
            Self::Setup(p) => serde_json::to_value(p),
            Self::Settings(p) => serde_json::to_value(p),
            Self::Job(p) => serde_json::to_value(p),
            Self::Review(p) => serde_json::to_value(p),
            Self::Notification(p) => serde_json::to_value(p),
            Self::Workflow(p) => serde_json::to_value(p),
            Self::System(p) => serde_json::to_value(p),
        }
        .expect("payload groups serialize infallibly: no maps with non-string keys");

        let mut obj = match v {
            Value::Object(o) => o,
            other => unreachable!("adjacent tagging always yields an object, got {other}"),
        };
        let event = obj
            .remove("event")
            .and_then(|e| match e {
                Value::String(s) => Some(s),
                _ => None,
            })
            .expect("adjacent tag is always a string");
        // `None` only for a unit variant, which this module forbids — empty
        // struct variants exist so `data` stays `{}` rather than `null`.
        let data = obj.remove("data").unwrap_or(Value::Null);
        (event, data)
    }

    /// Decode a wire `{event, data}` pair.
    pub fn from_wire(event: &str, data: &Value) -> Result<Self, EventDecodeError> {
        let group = Group::of(event).ok_or_else(|| EventDecodeError::UnknownEvent {
            event: event.to_string(),
        })?;
        let composite = serde_json::json!({ "event": event, "data": data });
        let malformed = |source: serde_json::Error| EventDecodeError::MalformedPayload {
            event: event.to_string(),
            source,
        };
        Ok(match group {
            Group::Turn => Self::Turn(serde_json::from_value(composite).map_err(malformed)?),
            Group::Setup => Self::Setup(serde_json::from_value(composite).map_err(malformed)?),
            Group::Settings => {
                Self::Settings(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::Job => Self::Job(serde_json::from_value(composite).map_err(malformed)?),
            Group::Review => Self::Review(serde_json::from_value(composite).map_err(malformed)?),
            Group::Notification => {
                Self::Notification(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::Workflow => {
                Self::Workflow(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::System => Self::System(serde_json::from_value(composite).map_err(malformed)?),
        })
    }

    /// Does this event belong in `session.jsonl`?
    ///
    /// The Turn and Settings groups make a per-variant decision (see
    /// [`TurnPayload::is_persisted`] and [`SettingsPayload::is_persisted`]); the
    /// rest are live notifications whose state is recoverable from the session
    /// record.
    ///
    /// `session_initialized` is here because a session's starting model is
    /// otherwise unrecoverable on resume: it is emitted, but was never
    /// persisted, while `model_switched` was — so a session that switched models
    /// had an attributable second half and an unattributable first half.
    ///
    /// It is persisted **only when the model is known**. The setup task runs
    /// before `session.configure_agent`, so it "almost always observes `None`
    /// here and the event carries empty strings"
    /// (`server/session/mod.rs`) — `assets/fixtures/demo.jsonl` records
    /// `"model":""`. Persisting that is worse than persisting nothing, because
    /// an empty model looks like an answer. Emitting the event after the model
    /// resolves is the real fix and belongs to the setup task, not here.
    pub fn is_persisted(&self) -> bool {
        match self {
            Self::Turn(t) => t.is_persisted(),
            Self::Settings(s) => s.is_persisted(),
            Self::Setup(SetupPayload::SessionInitialized(p)) => !p.model.is_empty(),
            Self::Setup(_)
            | Self::Job(_)
            | Self::Review(_)
            | Self::Notification(_)
            | Self::Workflow(_)
            | Self::System(_) => false,
        }
    }
}

macro_rules! impl_from_group {
    ($($ty:ident => $variant:ident),* $(,)?) => {
        $(impl From<$ty> for SessionEventPayload {
            fn from(p: $ty) -> Self {
                Self::$variant(p)
            }
        })*
    };
}

impl_from_group! {
    TurnPayload => Turn,
    SetupPayload => Setup,
    SettingsPayload => Settings,
    JobPayload => Job,
    ReviewPayload => Review,
    NotificationPayload => Notification,
    WorkflowPayload => Workflow,
    SystemPayload => System,
}
