//! The turn stream: the sixteen events a session emits while a turn runs.
//!
//! This is the group [`Systems`](../../../../../docs/Meta/Analysis/Systems.md)
//! §Presentation Parity Boundary describes in prose. `TurnPayload`'s variant
//! list *is* "the same `SessionEventMessage` vocabulary with the same fields
//! populated", so a new `AgentHandle` gets correct presentation exactly when it
//! emits these.
//!
//! # Why every scalar field carries `#[serde(default)]`
//!
//! Decoding a turn payload is deliberately *total*: an absent field yields the
//! type's default rather than failing the whole event. Three independent
//! reasons, each verified rather than assumed:
//!
//! 1. **Real recordings omit fields.** Across `assets/fixtures/*.jsonl`,
//!    `message_id` is present on only 13 of 16 `message_complete` lines and 13
//!    of 16 `user_message` lines, `display` on 7 of 97 `tool_call` lines, and
//!    `terminate` on 7 of 66 `tool_result` lines. A strict decode would drop
//!    most of the corpus.
//! 2. **Every existing consumer already defaulted them.** The hand-written
//!    destructurers this type replaces read `data["tool"].as_str()
//!    .unwrap_or("tool")` and friends. Making the decode strict would be a
//!    behaviour change smuggled in under a typing change.
//! 3. **`should_persist` decides from these values.** A line whose payload this
//!    build cannot parse is still the user's transcript; failing the decode
//!    would silently drop it from `session.jsonl`.
//!
//! Serialization is unaffected — `#[serde(default)]` adds no
//! `skip_serializing_if`, so the wire form is byte-identical to what the
//! `SessionEventMessage` constructors produced before. The fields that today's
//! wire *does* omit (the token counts, the optional `tool_call` metadata) are
//! `Option`/`Vec` with an explicit `skip_serializing_if`, and
//! `rpc.rs`'s golden tests pin each shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::session_event::ScriptingEvent;
use crate::interaction::{InteractionRequest, InteractionResponse};
use crate::traits::chat::PrecognitionNoteInfo;
use crate::types::acp::FileDiff;
use crate::types::ToolDisplay;

/// Turn-stream events, adjacently tagged so the enum's serialization *is* the
/// `{event, data}` pair the envelope carries.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum TurnPayload {
    UserMessage {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        content: String,
    },
    TextDelta {
        #[serde(default)]
        content: String,
    },
    Thinking {
        #[serde(default)]
        content: String,
    },
    /// A text segment that streamed before a tool call, emitted at the
    /// text→tool boundary. `message_id` is the turn id (shared with
    /// `user_message` and `message_complete`); `index` is the 0-based segment
    /// position within the turn; `content` is the segment's text (the delta
    /// accumulated since the previous boundary). Lets viewers converge on
    /// canonical per-segment bubbles across live streaming and history reload.
    /// `message_complete` still carries the WHOLE turn's accumulated text —
    /// segments are additive, not a replacement.
    SegmentComplete {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        index: usize,
        #[serde(default)]
        content: String,
    },
    /// The five token fields are absent when the provider reported no usage,
    /// and the two cache fields are absent when the provider reported no
    /// caching. `skip_serializing_if` is what keeps that distinction on the
    /// wire — a client tells "no data" from "zero" by presence, and the TUI
    /// status bar's sentinel depends on it.
    MessageComplete {
        #[serde(default)]
        message_id: String,
        #[serde(default)]
        full_response: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completion_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_read_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_creation_tokens: Option<u32>,
    },
    /// Field order is load-bearing: `serde_json` is built with
    /// `preserve_order`, so the declaration order here is the key order on the
    /// wire and in `session.jsonl`. It reproduces the insertion order of the
    /// `json!` block this variant replaced — `display` after `lua_primary_arg`,
    /// not next to `args`.
    ToolCall {
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        tool: String,
        #[serde(default)]
        args: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lua_primary_arg: Option<String>,
        /// One projection of "which argument matters", computed by
        /// `SessionEventMessage::tool_call_with_metadata` so the TUI and the web
        /// render the same answer instead of each keeping its own key-priority
        /// list. A Lua display hook's `lua_primary_arg` overrides it.
        ///
        /// Every producer in this workspace sets it. `None` means the event came
        /// from something else — a recording made before the field existed (only
        /// 7 of 97 recorded `tool_call` lines carry it), or a foreign emitter —
        /// and the consumer falls back to its own heuristic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<ToolDisplay>,
        /// Which layer granted permission without asking, if any. Rides on this
        /// event rather than a follow-up: the gate decides BEFORE the card is
        /// emitted, so a separate event would only make the marker pop in late.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto_approved: Option<String>,
        #[serde(
            default,
            deserialize_with = "lenient_diffs",
            skip_serializing_if = "Vec::is_empty"
        )]
        diffs: Vec<FileDiff>,
    },
    /// Late arguments for a tool call already announced by a prior `tool_call`.
    /// Produced when an ACP agent announces the call without `rawInput` and
    /// only supplies it in a follow-up `ToolCallUpdate` frame. Subscribers
    /// merge `args` into the existing entry keyed by `call_id`.
    ToolCallArgsUpdate {
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        args: Value,
    },
    /// Late file-diff content for a tool call already announced by a prior
    /// `tool_call`. Subscribers merge `diffs` into the existing entry keyed by
    /// `call_id`.
    ToolCallDiffUpdate {
        #[serde(default)]
        call_id: String,
        #[serde(default, deserialize_with = "lenient_diffs")]
        diffs: Vec<FileDiff>,
    },
    /// `terminate` is serialized even when `false` — an existing subscriber
    /// reads `data.terminate` unconditionally. Do NOT add
    /// `skip_serializing_if`.
    ///
    /// `result` is the nested [`ToolResultBody`] envelope, kept as a `Value`
    /// here because a recorded `tool_result` may carry any shape and the event
    /// must still decode. Use [`ToolResultBody::of`] to read it.
    ToolResult {
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        tool: String,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        terminate: bool,
    },
    Ended {
        #[serde(default)]
        reason: String,
    },
    InteractionRequested {
        #[serde(default)]
        request_id: String,
        request: InteractionRequest,
    },
    InteractionCompleted {
        #[serde(default)]
        request_id: String,
        response: InteractionResponse,
    },
    InjectionPending {
        #[serde(default)]
        content: String,
        #[serde(default)]
        position: String,
        #[serde(default)]
        is_continuation: bool,
    },
    ContextInjected {
        #[serde(default)]
        role: String,
        #[serde(default)]
        content: String,
    },
    PrecognitionComplete {
        #[serde(default)]
        notes_count: usize,
        #[serde(default)]
        query_summary: String,
        /// Absent when the search produced nothing worth carrying. The
        /// producer inserts it only when it has notes
        /// (`agent_manager/mod.rs`), so the key must stay omissible.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<PrecognitionNoteInfo>,
    },
    PostLlmCall {
        #[serde(default)]
        response_summary: String,
        #[serde(default)]
        model: String,
        #[serde(default)]
        duration_ms: u64,
        /// Always serialized, including as `null` — the producer writes
        /// `"token_count": null` today (`messaging/stream.rs`).
        #[serde(default)]
        token_count: Option<u64>,
    },
}

/// Deserialize `diffs` tolerantly: a value that is not a `Vec<FileDiff>` yields
/// an empty Vec instead of failing the whole payload.
///
/// The two consumers this replaces both did exactly that — log and carry on with
/// an empty Vec — because a tool card is still worth rendering without its
/// diffs. Making the decode strict would have turned a cosmetic degradation into
/// a dropped tool call, which is a behaviour change smuggled in under a typing
/// change.
fn lenient_diffs<'de, D>(deserializer: D) -> Result<Vec<FileDiff>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Value::deserialize(deserializer)?;
    if raw.is_null() {
        return Ok(Vec::new());
    }
    match serde_json::from_value(raw.clone()) {
        Ok(diffs) => Ok(diffs),
        Err(e) => {
            tracing::warn!(
                error = %e,
                raw = %raw,
                "session event carried a malformed `diffs` field; continuing with an empty Vec",
            );
            Ok(Vec::new())
        }
    }
}

impl TurnPayload {
    /// The [`ScriptingEvent`] a Lua handler sees for this transport event,
    /// where one exists.
    ///
    /// The transport and scripting vocabularies are disjoint by design (see the
    /// parent module), and they spell ten of the same events differently.
    /// `crucible.on("tool_called")` and the web's `tool_call` SSE frame are the
    /// same event; nothing said so before, so a plugin author reading the SSE
    /// stream learned the wrong name.
    ///
    /// Returning the shared type rather than a `&'static str` is what makes
    /// this agree with [`SessionEvent::event_type`](crate::events::SessionEvent)
    /// by construction: both read the name off the same constant.
    pub fn as_scripting_event(&self) -> Option<ScriptingEvent> {
        Some(match self {
            Self::UserMessage { .. } => ScriptingEvent::MessageReceived,
            Self::TextDelta { .. } => ScriptingEvent::TextDelta,
            Self::Thinking { .. } => ScriptingEvent::AgentThinking,
            Self::MessageComplete { .. } => ScriptingEvent::AgentResponded,
            Self::ToolCall { .. } => ScriptingEvent::ToolCalled,
            Self::ToolResult { .. } => ScriptingEvent::ToolCompleted,
            Self::Ended { .. } => ScriptingEvent::SessionEnded,
            Self::InteractionRequested { .. } => ScriptingEvent::InteractionRequested,
            Self::InteractionCompleted { .. } => ScriptingEvent::InteractionCompleted,
            Self::PrecognitionComplete { .. } => ScriptingEvent::PrecognitionComplete,
            Self::SegmentComplete { .. }
            | Self::ToolCallArgsUpdate { .. }
            | Self::ToolCallDiffUpdate { .. }
            | Self::InjectionPending { .. }
            | Self::ContextInjected { .. }
            | Self::PostLlmCall { .. } => return None,
        })
    }

    /// Does this event belong in `session.jsonl`?
    ///
    /// Exhaustive on purpose. A turn event that reaches a resumed transcript but
    /// not this match is invisible on resume with nothing reporting it — which
    /// is how `segment_complete` was nearly shipped unpersisted. Adding a
    /// variant above breaks this build until someone decides.
    pub fn is_persisted(&self) -> bool {
        match self {
            Self::UserMessage { .. }
            | Self::Thinking { .. }
            | Self::SegmentComplete { .. }
            | Self::MessageComplete { .. }
            | Self::ToolCall { .. }
            | Self::ToolResult { .. }
            | Self::Ended { .. }
            // What context was injected is part of the turn's record, not just
            // a live notification: without it a resumed transcript cannot say
            // which notes the answer was grounded in, and re-deriving it later
            // would report today's search results as though they were the ones
            // actually used.
            | Self::PrecognitionComplete { .. } => true,
            // `context_injected` sits semantically next to
            // `precognition_complete` and is NOT persisted here, because
            // `inject_context` already writes a `LogEvent` line for the same
            // content and persisting both would duplicate it. Flagged, not
            // changed: see the plan's open question 4.
            Self::TextDelta { .. }
            | Self::ToolCallArgsUpdate { .. }
            | Self::ToolCallDiffUpdate { .. }
            | Self::InteractionRequested { .. }
            | Self::InteractionCompleted { .. }
            | Self::InjectionPending { .. }
            | Self::ContextInjected { .. }
            | Self::PostLlmCall { .. } => false,
        }
    }
}

/// The body of a `tool_result` event's `data.result`.
///
/// Untagged because the wire form has no discriminator: success is
/// `{"result": …}` and failure is `{"error": …}`, decided by which key is
/// present (`agent_manager/messaging/tool_call.rs`). The variants are disjoint
/// on their required key, so untagged is unambiguous — but they must stay
/// disjoint. A key added to both breaks the decode silently.
///
/// [`Systems`](../../../../../docs/Meta/Analysis/Systems.md) documented this as
/// "the `{"result"|"error": …}` envelope": a two-key description of a four-key
/// reality. `spill_path` and `summary` are the other two.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ToolResultBody {
    Ok {
        /// Arbitrary JSON: a string for most tools, an object for structured
        /// ones. Never assume `as_str()`.
        result: Value,
        /// Set when the output was spilled to disk (≥10KB, spillable tool).
        /// The referenced file lives under the session dir and outlives the
        /// event, so this is the recovery path for the full output.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spill_path: Option<String>,
        /// Display summary from a `tool:display_complete` Lua hook.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    Err {
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

impl ToolResultBody {
    /// Read a `tool_result`'s `data.result`.
    ///
    /// `None` for a shape neither variant covers — a bare string, or an object
    /// with neither key. Callers keep their existing fallback rather than
    /// inventing one, because a `tool_result` whose body will not decode is
    /// still a tool result the user ran.
    pub fn of(result: &Value) -> Option<Self> {
        serde_json::from_value(result.clone()).ok()
    }

    /// The error message, if this body is a failure.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Err { error, .. } => Some(error),
            Self::Ok { .. } => None,
        }
    }
}
