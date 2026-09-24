//! Unified agent event protocol.
//!
//! One event type (`TurnEvent`) flows from every agent — ACP, internal
//! genai, future backends — into the daemon's runtime. The runtime
//! aggregates the stream into `SessionEvent`s for subscribers; there is
//! no per-backend `SessionEvent` reassembly.
//!
//! Tool-loop control is event-driven: the agent emits `ToolCall`, the
//! runtime replies with a `ToolResult` on an inbound channel. The
//! runtime uses the same inbound channel to attach retrieved context
//! (`ContextAttach`). There is one channel topology, not two.
//!
//! Conversation state lives in [`tree::ConversationTree`]: scheduler-
//! owned, append-only, fanout/collect preserved as first-class ops so
//! later branching features (markdown-driven workflows, session forks)
//! do not require a separate data model.

pub mod tree;

pub use tree::{ConversationTree, NodeContent, NodeId, TurnNode};

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::traits::context_ops::ContextMessage;
use crate::traits::llm::TokenUsage;

/// Event flowing from an `Agent` to the runtime, or (for a subset of
/// variants — `ToolResult`, `ContextAttach`) from the
/// runtime back to the agent on the inbound channel.
///
/// Terminal variants: `Done`, `Error`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TurnEvent {
    /// Incremental text delta from the model.
    TextDelta(String),

    /// Reasoning/thinking delta (e.g. DeepSeek-R1, Claude thinking mode).
    Thinking(String),

    /// Model invoked a tool. Outbound only (agent → runtime).
    ///
    /// `call` is the canonical call when the agent layer classified it: an
    /// ACP call from its frames, or a Crucible tool call with the diff that
    /// the provider layer made. `None` means that the runtime classifies it
    /// from `name` and `args` as a Crucible tool, with no diff.
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<Box<crate::types::CanonicalToolCall>>,
    },

    /// Result of a tool call.
    ///
    /// - Outbound (agent → runtime): the agent observed a tool result
    ///   (e.g. ACP's tool-call update frames).
    /// - Inbound (runtime → agent): the runtime executed a tool and is
    ///   feeding the result back; the agent incorporates it into the
    ///   next LLM call.
    ToolResult {
        id: String,
        name: String,
        result: serde_json::Value,
        error: Option<String>,
    },

    /// A new canonical form of a `ToolCall` that was already emitted. An
    /// ACP agent can send the arguments or the diff of a call in a later
    /// frame, so the call can change after the agent announced it.
    /// Subscribers replace the call of the entry with this `id`.
    ///
    /// Outbound only (agent → runtime). Does not advance tool depth and
    /// does not trigger tool dispatch.
    ToolCallUpdate {
        id: String,
        call: Box<crate::types::CanonicalToolCall>,
    },

    /// Marker that all `ToolCall`s from the current chat completion
    /// have been emitted. The runtime uses this to tick tool-depth
    /// per batch rather than per individual call — models that emit
    /// parallel tool calls in one batch count as one depth tick.
    ///
    /// Outbound only (agent → runtime). Emitted by the adapter (or a
    /// native `Agent` impl) right before it waits for `ToolResult`s.
    ToolBatchEnd,

    /// Inbound only. Knowledge retrieved mid-turn (a Lua handler called
    /// `cru.context.attach`) that the agent should have available for its
    /// next LLM call.
    ///
    /// Reference material, not a user turn, so agents append it as a system
    /// message.
    ///
    /// **Append at the end; never prepend.** Inserting ahead of the existing
    /// messages invalidates the whole prompt-cache prefix, and the cost then
    /// scales with conversation length — the opposite of what a
    /// fires-often retrieval path needs.
    ///
    /// Context only: this never enters the conversation tree or the session
    /// log. History stays append-only and owned by the scheduler; forking is
    /// the only way to diverge from it.
    ContextAttach { content: String },

    /// Token usage. Typically one event per turn, near `Done`.
    Usage(TokenUsage),

    /// The agent's own view of its context window: how many tokens are
    /// currently occupying it and how large it is.
    ///
    /// Outbound only. Distinct from [`TurnEvent::Usage`], which reports what
    /// *this turn* consumed: `used` is an occupancy reading for the whole
    /// conversation and `limit` is a property of the agent, not of the turn.
    ///
    /// Only a delegated agent produces this. The internal agent's window is
    /// resolved once per session from the provider API
    /// (`server/session/mod.rs`), because its endpoint and model are known
    /// up front; a delegated agent has neither, so the window can only come
    /// from the agent itself — ACP `session/update` `usage_update` frames.
    ContextWindow { used: u64, limit: u64 },

    /// Turn finished normally. Terminal.
    Done { stop_reason: StopReason },

    /// Turn failed. Terminal.
    Error(TurnError),
}

/// Reason a turn ended, carried on `TurnEvent::Done`.
///
/// It reaches a plugin on the `turn:complete` payload and a front end on
/// `message_complete`, so a handler can tell a model that finished from a
/// model the provider cut off. The host itself reads none of it: what to do
/// about a premature stop is the plugin's decision.
///
/// Two variants this deliberately does NOT have. A stop-sequence variant,
/// because Crucible sets no stop sequence anywhere, so no provider can report
/// one. A tool-use variant, because the turn loop emits `Done` only when no
/// tool call is pending — a provider that says "tool_use" has already had its
/// calls dispatched, and the turn after them ends for some other reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(strum::EnumIter))]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Model finished naturally.
    EndTurn,
    /// Cancelled by user / caller.
    Cancelled,
    /// Turn produced nothing the user can see: no text, no thinking, no tool
    /// calls. Both agents report it — `GenaiAgentHandle` when a well-formed
    /// stream yielded no content (and on an unexpected stream close),
    /// `AcpAgentHandle` when a delegated turn ended without emitting anything.
    Empty,
    /// The provider truncated the answer at the model's OUTPUT cap. The reply
    /// stops mid-thought, and another turn continues it.
    ///
    /// This is NOT a reason to compact. `should_autocompact` compares the
    /// PROMPT tokens against the input window; this variant reports the output
    /// cap. The two numbers answer different questions, and a session that
    /// hits this one can have an almost empty context.
    MaxTokens,
    /// The model or its provider declined to answer: a safety filter, a
    /// content filter, or a delegated agent's own refusal. The same request
    /// gets the same answer, so a re-prompt of it is waste.
    Refusal,
}

impl StopReason {
    /// Every reason, for a caller that must consider all of them.
    ///
    /// `all_holds_every_stop_reason` proves the array complete by a walk over
    /// `strum::EnumIter`, which is what the compiler knows. A crate that does
    /// not depend on `strum` reads this instead.
    pub const ALL: &'static [Self] = &[
        Self::EndTurn,
        Self::Cancelled,
        Self::Empty,
        Self::MaxTokens,
        Self::Refusal,
    ];

    /// The line a renderer draws beside the reply, or `None` when the reason
    /// needs no note.
    ///
    /// **This is the only wording.** The TUI calls the function. The browser
    /// cannot, so `crucible_web::ChatEvent::MessageComplete` carries the
    /// answer as `stop_notice` and the page draws the string the daemon sent.
    /// A second wording in TypeScript is what this replaced, and the two had
    /// already drifted — a capital letter and a full stop. The gate
    /// `the_frontend_words_no_stop_reason_notice` in `crucible-web` refuses a
    /// new one.
    ///
    /// `EndTurn` says nothing because a completed answer explains itself;
    /// `Cancelled` and `Empty` already have their own paths in every
    /// renderer.
    #[must_use]
    pub fn user_notice(&self) -> Option<&'static str> {
        match self {
            Self::MaxTokens => Some("the model reached its output limit, so the reply stops here"),
            Self::Refusal => Some("the model declined to answer"),
            Self::EndTurn | Self::Cancelled | Self::Empty => None,
        }
    }
}

/// How a whole turn ended.
///
/// The daemon sends it in the `turn_finished` event and gives it to the
/// in-process caller of `send_message_notified`. [`StopReason`] tells why ONE
/// provider call stopped. This tells what happened to the turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// The turn ran to its end.
    Completed,
    /// A user stopped the turn: a `session.cancel` from any client.
    Cancelled,
    /// Plugin or system code stopped the turn, for example a `pre_llm_call`
    /// or `transform_context` handler that returned a cancel. The `error`
    /// field of `turn_finished` holds the reason.
    HandlerCancelled,
    /// The turn did not end in the time that its caller allowed.
    TimedOut,
    /// The turn stopped on an error. The `error` field holds the text.
    Failed,
}

/// Who asked for a turn.
///
/// A turn ENDS, and a `turn:complete` handler that wants more work asks for a
/// NEW turn. That turn is a normal turn: it takes admission, Precognition,
/// persistence and undo like any other. Only this field says who asked.
///
/// The serde form is the `origin` and `plugin` keys of the `user_message`
/// event: `{"origin": "plugin", "plugin": "goal"}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "origin", content = "plugin", rename_all = "snake_case")]
pub enum TurnOrigin {
    /// A person sent the message through a client.
    #[default]
    User,
    /// The plugin with this name asked for the turn.
    Plugin(String),
}

impl TurnOrigin {
    /// The name of the plugin that asked for the turn.
    pub fn plugin(&self) -> Option<&str> {
        match self {
            Self::User => None,
            Self::Plugin(name) => Some(name),
        }
    }
}

/// Does this streamed text count as something the user can see?
///
/// Whitespace-only chunks are what a provider or a delegated agent emits while
/// producing nothing, so they must not keep a turn out of [`StopReason::Empty`].
/// Both `GenaiAgentHandle` and `AcpAgentHandle` gate `produced_content` on this
/// one function rather than on two hand-written predicates: they had drifted —
/// ACP counted any chunk at all — so an agent streaming a single `"\n"`
/// reported `EndTurn` delegated and `Empty` internally. `stream.rs`'s
/// empty-response guard trims for the same reason.
#[must_use]
pub fn is_visible_content(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Non-fatal error delivered as a terminal `TurnEvent::Error`.
///
/// Distinct from [`AgentError`]: a `TurnError` is an error that happened
/// mid-stream and is delivered through the event stream; an `AgentError`
/// means the agent could not even begin a turn (e.g. connection refused
/// before any frame was sent).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, thiserror::Error)]
pub enum TurnError {
    #[error("connection error: {0}")]
    Connection(String),

    #[error("communication error: {0}")]
    Communication(String),

    #[error("agent not available: {0}")]
    AgentUnavailable(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Error starting a turn or dispatching a trait-level operation
/// (`cancel`, `switch_model`). Distinct from `TurnError` which rides
/// the event stream.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentError {
    #[error("connection error: {0}")]
    Connection(String),

    #[error("communication error: {0}")]
    Communication(String),
}

/// Typed "this capability is not supported" error.
///
/// Any `Agent` method that can be optional uses `Result<_, NotSupported>`.
/// The `AgentCapabilities` struct mirrors these so UIs can pre-filter,
/// but the setter's `Err(NotSupported)` is the authoritative response.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{capability} not supported by this agent")]
pub struct NotSupported {
    pub capability: String,
}

impl NotSupported {
    pub fn new(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
        }
    }
}

/// Static capability discovery for an agent.
///
/// UIs use these flags to grey out controls the agent cannot satisfy.
/// For runtime checks, prefer calling the method and matching on
/// `Err(NotSupported)` — capabilities are pre-filter hints, not gates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// Agent emits incremental `TextDelta` events.
    pub streaming: bool,
    /// Agent supports tool calls.
    pub tool_calls: bool,
    /// Agent emits `Thinking` events (reasoning models).
    pub thinking: bool,
    /// Agent exposes `switch_model`.
    pub model_switching: bool,
    /// Agent reports `Usage` events.
    pub usage_reporting: bool,
    /// Agent honors `cancel()`.
    pub cancellation: bool,
    /// Agent manages its own conversation history and refuses
    /// `clear_history` (e.g. ACP agents).
    pub owns_history: bool,
    /// Agent supports modes (plan / act / auto).
    pub modes: bool,
}

/// Inputs to one turn.
///
/// The runtime passes `content` (user message text) plus the full
/// conversation `messages` the agent should see, and holds the inbound
/// channel; the agent's `turn()` stream drains `inbound` at whatever
/// cadence its protocol requires (typically: wait for `ToolResult`
/// after emitting a `ToolCall`).
///
/// Ownership: the scheduler (e.g. daemon's `AgentManager`) owns the
/// conversation state — today as a [`ConversationTree`], flattened to
/// `messages` per turn. Agents are stateless between turns WRT
/// conversation content; any per-turn scratch (accumulated tool
/// results mid-loop) lives locally inside `turn()`'s stream body.
pub struct TurnContext {
    /// User message content for this turn.
    pub content: String,
    /// Full flattened conversation history provided by the scheduler.
    /// Includes the user's new message at the end when applicable.
    /// Empty for legacy callers that rely on agent-side state.
    pub messages: Vec<ContextMessage>,
    /// The messages in `messages` that this turn added to the history: its
    /// injected context. See [`added_messages`].
    pub injected: Vec<ContextMessage>,
    /// Inbound event channel. Runtime sends `ToolResult` and
    /// `ContextAttach`. May be `None` for fire-and-forget turns that need
    /// no tool loop.
    pub inbound: Option<mpsc::Receiver<TurnEvent>>,
}

impl TurnContext {
    /// Build a simple turn context with no inbound channel and no
    /// scheduler-provided messages.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            messages: Vec::new(),
            injected: Vec::new(),
            inbound: None,
        }
    }

    /// Attach an inbound channel (for agents that need tool results).
    pub fn with_inbound(mut self, rx: mpsc::Receiver<TurnEvent>) -> Self {
        self.inbound = Some(rx);
        self
    }

    /// Attach scheduler-flattened conversation history.
    pub fn with_messages(mut self, messages: Vec<ContextMessage>) -> Self {
        self.messages = messages;
        self
    }

    /// Attach the context that this turn injected into the history.
    pub fn with_injected(mut self, injected: Vec<ContextMessage>) -> Self {
        self.injected = injected;
        self
    }
}

/// The messages in `after` that are not in `before`, in the order of
/// `after`.
///
/// `before` is the conversation history. `after` is that history once the
/// context seam adds this turn's injected context (Precognition, `@file`
/// attachments, `transform_context` handlers). An agent that owns its own
/// history needs only this difference. Messages match on role and content,
/// so a handler that rebuilds the array without metadata does not turn old
/// history into new context.
pub fn added_messages(before: &[ContextMessage], after: &[ContextMessage]) -> Vec<ContextMessage> {
    let mut unmatched: Vec<&ContextMessage> = before.iter().collect();
    after
        .iter()
        .filter(|message| {
            match unmatched
                .iter()
                .position(|old| old.role == message.role && old.content == message.content)
            {
                Some(index) => {
                    unmatched.swap_remove(index);
                    false
                }
                None => true,
            }
        })
        .cloned()
        .collect()
}

/// Wrap System-role blocks added by a context handler, leaving historical
/// messages alone. Built-in injections already carry their own envelope.
pub fn tag_new_system_messages(before: &[ContextMessage], after: &mut [ContextMessage]) {
    use crate::traits::llm::MessageRole;

    let mut unmatched: Vec<_> = before
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .collect();
    for message in after {
        if message.role != MessageRole::System {
            continue;
        }
        if let Some(index) = unmatched.iter().position(|text| *text == message.content) {
            unmatched.swap_remove(index);
        } else if !message.content.starts_with("<system-message ") {
            let kind = message.metadata.kind.as_deref().unwrap_or("context");
            let source = message.metadata.source.as_deref().unwrap_or("lua");
            // A handler may prefix an existing tagged injection while
            // retaining metadata. Remove its old envelope before putting
            // the edited text inside the single final envelope.
            let content = if message.metadata.kind.is_some() {
                if let Some(start) = message.content.find("<system-message ") {
                    let open = message.content[start..].find('>').map(|i| start + i + 1);
                    let close = message.content.rfind("</system-message>");
                    match (open, close) {
                        (Some(open), Some(close)) if open <= close => format!(
                            "{}{}{}",
                            &message.content[..start],
                            message.content[open..close].trim_matches('\n'),
                            &message.content[close + "</system-message>".len()..]
                        ),
                        _ => message.content.clone(),
                    }
                } else {
                    message.content.clone()
                }
            } else {
                message.content.clone()
            };
            let tagged = ContextMessage::injection(kind, source, content);
            message.content = tagged.content;
            message.metadata.kind = tagged.metadata.kind;
            message.metadata.source = tagged.metadata.source;
            message.metadata.token_estimate = tagged.metadata.token_estimate;
        }
    }
}

/// A unified agent.
///
/// Variation between agent kinds (ACP, internal genai, future backends)
/// lives in `TurnEvent` variants, not in trait-method surface area.
/// New kinds add new event handlers; they do not add trait methods.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Static capability discovery.
    fn capabilities(&self) -> AgentCapabilities;

    /// Run one turn. Returns an outbound event stream terminating in
    /// `Done` or `Error`. The runtime may steer the agent's
    /// continuation by sending events on the inbound channel carried
    /// in `ctx`.
    ///
    /// The stream borrows `&mut self` so the stream body can mutate
    /// agent state in-place (append to history, update indices, etc.)
    /// without needing interior mutability. Callers must keep the
    /// mutex guard / `&mut` alive for the duration of the stream.
    async fn turn<'a>(
        &'a mut self,
        ctx: TurnContext,
    ) -> Result<BoxStream<'a, TurnEvent>, AgentError>;

    /// Cancel an in-flight turn.
    async fn cancel(&self) -> Result<(), AgentError>;

    /// Switch the active model. Agents that don't expose model
    /// switching return `Err(NotSupported)` and set
    /// `capabilities.model_switching = false`.
    async fn switch_model(&mut self, model_id: &str) -> Result<(), NotSupported>;
}

/// Convenience macro for test fixtures that need to satisfy the
/// [`Agent`] supertrait bound on [`crate::traits::chat::AgentHandle`]
/// but never have their `Agent::turn` called in tests. Emits an impl
/// that returns `Done{Empty}` immediately and `NotSupported` for
/// `switch_model`.
///
/// Usage:
/// ```ignore
/// crucible_core::impl_noop_agent!(MyMockHandle);
/// ```
#[macro_export]
macro_rules! impl_noop_agent {
    ($ty:ty) => {
        #[async_trait::async_trait]
        impl $crate::turn::Agent for $ty {
            fn capabilities(&self) -> $crate::turn::AgentCapabilities {
                $crate::turn::AgentCapabilities::default()
            }

            async fn turn<'a>(
                &'a mut self,
                _ctx: $crate::turn::TurnContext,
            ) -> Result<
                futures::stream::BoxStream<'a, $crate::turn::TurnEvent>,
                $crate::turn::AgentError,
            > {
                Ok(Box::pin(futures::stream::iter(vec![
                    $crate::turn::TurnEvent::Done {
                        stop_reason: $crate::turn::StopReason::Empty,
                    },
                ])))
            }

            async fn cancel(&self) -> Result<(), $crate::turn::AgentError> {
                Ok(())
            }

            async fn switch_model(
                &mut self,
                _model_id: &str,
            ) -> Result<(), $crate::turn::NotSupported> {
                Err($crate::turn::NotSupported::new("switch_model"))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_system_context_is_tagged_without_retagging_history() {
        let old = ContextMessage::system("stable context");
        let mut after = vec![old.clone(), ContextMessage::system("plugin context")];
        tag_new_system_messages(std::slice::from_ref(&old), &mut after);
        assert_eq!(after[0], old);
        assert_eq!(after[1].metadata.kind.as_deref(), Some("context"));
        assert_eq!(after[1].metadata.source.as_deref(), Some("lua"));
        assert!(after[1]
            .content
            .starts_with("<system-message kind=\"context\" source=\"lua\">"));
        tag_new_system_messages(&[old], &mut after);
        assert_eq!(after[1].content.matches("<system-message").count(), 1);
    }

    #[test]
    fn edited_tagged_context_keeps_one_envelope() {
        let original = ContextMessage::injection("precognition", "daemon", "a note");
        let mut edited = original.clone();
        edited.content = format!("[redacted] {}", edited.content);
        tag_new_system_messages(&[], std::slice::from_mut(&mut edited));
        assert_eq!(edited.content.matches("<system-message ").count(), 1);
        assert!(edited.content.contains("[redacted] a note"));
    }

    /// This turn's Precognition goes in front of the whole history. Old
    /// system context from the history is not new, and only the new block
    /// comes back.
    #[test]
    fn added_messages_are_the_new_messages_only() {
        let history = vec![
            ContextMessage::system("OLD CONTEXT"),
            ContextMessage::user("first question"),
            ContextMessage::assistant("first answer"),
            ContextMessage::user("second question"),
        ];
        let mut transformed = vec![ContextMessage::system("NEW CONTEXT")];
        transformed.extend(history.iter().cloned());

        assert_eq!(
            added_messages(&history, &transformed),
            vec![ContextMessage::system("NEW CONTEXT")]
        );
    }

    /// A message that repeats one in the history is new when it occurs more
    /// often than in the history.
    #[test]
    fn a_repeated_message_counts_once_per_occurrence() {
        let history = vec![ContextMessage::system("X")];
        let transformed = vec![ContextMessage::system("X"), ContextMessage::system("X")];

        assert_eq!(
            added_messages(&history, &transformed),
            vec![ContextMessage::system("X")]
        );
    }

    /// A handler that drops the metadata of a history message does not make
    /// the message new.
    #[test]
    fn a_history_message_without_its_metadata_is_not_new() {
        let history = vec![ContextMessage::system("OLD").with_tag("kept")];

        assert!(added_messages(&history, &[ContextMessage::system("OLD")]).is_empty());
    }

    /// The predicate both agent handles gate `StopReason::Empty` on. Unicode
    /// whitespace counts as blank because `str::trim` uses `White_Space`, and
    /// a turn made of non-breaking spaces showed the user nothing either.
    #[test]
    fn only_non_whitespace_text_counts_as_visible_content() {
        assert!(is_visible_content("hi"));
        assert!(is_visible_content("  hi  "));
        assert!(!is_visible_content(""));
        assert!(!is_visible_content("\n"));
        assert!(!is_visible_content(" \t\r\n "));
        assert!(!is_visible_content("\u{00a0}"));
    }

    #[test]
    fn not_supported_carries_capability_name() {
        let err = NotSupported::new("switch_model");
        assert_eq!(err.capability, "switch_model");
        assert!(err.to_string().contains("switch_model"));
    }

    #[test]
    fn capabilities_default_is_all_false() {
        let caps = AgentCapabilities::default();
        assert!(!caps.streaming);
        assert!(!caps.tool_calls);
        assert!(!caps.thinking);
        assert!(!caps.model_switching);
        assert!(!caps.owns_history);
    }

    #[test]
    fn turn_context_builder() {
        let ctx = TurnContext::new("hello");
        assert_eq!(ctx.content, "hello");
        assert!(ctx.inbound.is_none());
    }

    #[test]
    fn turn_event_roundtrip_json() {
        // Ensures the wire format stays stable — used on RPC.
        let e = TurnEvent::TextDelta("hello".into());
        let s = serde_json::to_string(&e).unwrap();
        let r: TurnEvent = serde_json::from_str(&s).unwrap();
        match r {
            TurnEvent::TextDelta(t) => assert_eq!(t, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn turn_error_variants_have_context() {
        let e = TurnError::Communication("boom".into());
        assert!(e.to_string().contains("boom"));
    }

    /// A reason missing from `ALL` is a reason the cross-language wording gate
    /// never looks at, so a new notice could reach the frontend unguarded.
    /// `EnumIter` walks what the compiler knows, so the array cannot fall
    /// behind the enum.
    #[test]
    fn all_holds_every_stop_reason() {
        use strum::IntoEnumIterator;

        let walked: Vec<StopReason> = StopReason::iter().collect();
        assert!(!walked.is_empty(), "EnumIter walked nothing");
        assert_eq!(
            walked,
            StopReason::ALL.to_vec(),
            "StopReason::ALL and the enum disagree"
        );
    }
}
