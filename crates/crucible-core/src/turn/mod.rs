//! Unified agent event protocol.
//!
//! One event type (`TurnEvent`) flows from every agent — ACP, internal
//! genai, future backends — into the daemon's runtime. The runtime
//! aggregates the stream into `SessionEvent`s for subscribers; there is
//! no per-backend `ChatChunk`/`SessionEvent` reassembly.
//!
//! Tool-loop control is event-driven: the agent emits `ToolCall`, the
//! runtime replies with a `ToolResult` on an inbound channel. The
//! runtime uses the same inbound channel to inject handler output
//! (`HandlerInjection`) and to signal depth-cap exhaustion
//! (`DepthCapHit`). There is one channel topology, not three.
//!
//! Conversation state lives in [`tree::ConversationTree`]: scheduler-
//! owned, append-only, fanout/collect preserved as first-class ops so
//! later branching features (markdown-driven workflows, session forks)
//! do not require a separate data model.

pub mod tree;

pub use tree::{ConversationTree, NodeContent, NodeId, NodeMeta, TurnNode};

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::traits::context_ops::ContextMessage;
use crate::traits::llm::TokenUsage;

/// Event flowing from an `Agent` to the runtime, or (for a subset of
/// variants — `ToolResult`, `HandlerInjection`, `DepthCapHit`) from the
/// runtime back to the agent on the inbound channel.
///
/// Terminal variants: `Done`, `Error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnEvent {
    /// Incremental text delta from the model.
    TextDelta(String),

    /// Reasoning/thinking delta (e.g. DeepSeek-R1, Claude thinking mode).
    Thinking(String),

    /// Model invoked a tool. Outbound only (agent → runtime).
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
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

    /// Marker that all `ToolCall`s from the current chat completion
    /// have been emitted. The runtime uses this to tick tool-depth
    /// per batch rather than per individual call — models that emit
    /// parallel tool calls in one batch count as one depth tick.
    ///
    /// Outbound only (agent → runtime). Emitted by the adapter (or a
    /// native `Agent` impl) right before it waits for `ToolResult`s.
    ToolBatchEnd,

    /// Inbound only. The runtime's post-turn handler returned an
    /// injection; the agent should treat `content` as the next turn's
    /// user message.
    HandlerInjection { content: String, position: String },

    /// Inbound only. Maximum tool-call depth was reached; the agent
    /// should produce a final response without further tool calls.
    DepthCapHit { max_depth: usize },

    /// Token usage. Typically one event per turn, near `Done`.
    Usage(TokenUsage),

    /// Turn finished normally. Terminal.
    Done { stop_reason: StopReason },

    /// Turn failed. Terminal.
    Error(TurnError),
}

/// Reason a turn ended, carried on `TurnEvent::Done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Model finished naturally.
    EndTurn,
    /// Runtime forced a final response after `max_tool_depth` was reached.
    MaxToolDepth,
    /// Cancelled by user / caller.
    Cancelled,
    /// Model produced no text and no tool calls.
    Empty,
}

/// Non-fatal error delivered as a terminal `TurnEvent::Error`.
///
/// Distinct from [`AgentError`]: a `TurnError` is an error that happened
/// mid-stream and is delivered through the event stream; an `AgentError`
/// means the agent could not even begin a turn (e.g. connection refused
/// before any frame was sent).
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
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

    #[error("agent not available: {0}")]
    AgentUnavailable(String),

    #[error("internal error: {0}")]
    Internal(String),
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
    /// Agent supports `:set temperature`.
    pub temperature_control: bool,
    /// Agent supports `:set max_tokens`.
    pub max_tokens_control: bool,
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
    /// Inbound event channel. Runtime sends `ToolResult`,
    /// `HandlerInjection`, `DepthCapHit`. May be `None` for
    /// fire-and-forget turns that need no continuation.
    pub inbound: Option<mpsc::Receiver<TurnEvent>>,
    /// Whether this turn is a continuation (reactor handler injection
    /// follow-up) rather than a fresh user message.
    pub is_continuation: bool,
}

impl TurnContext {
    /// Build a simple turn context with no inbound channel and no
    /// scheduler-provided messages.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            messages: Vec::new(),
            inbound: None,
            is_continuation: false,
        }
    }

    /// Attach an inbound channel (for agents that need tool results).
    pub fn with_inbound(mut self, rx: mpsc::Receiver<TurnEvent>) -> Self {
        self.inbound = Some(rx);
        self
    }

    /// Mark this turn as a continuation.
    pub fn continuation(mut self) -> Self {
        self.is_continuation = true;
        self
    }

    /// Attach scheduler-flattened conversation history.
    pub fn with_messages(mut self, messages: Vec<ContextMessage>) -> Self {
        self.messages = messages;
        self
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

/// A boxed agent instance.
pub type BoxAgent = Box<dyn Agent + Send + Sync>;

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

/// Shared agent instance.
pub type SharedAgent = Arc<tokio::sync::Mutex<BoxAgent>>;

#[cfg(test)]
mod tests {
    use super::*;

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
        let ctx = TurnContext::new("hello").continuation();
        assert_eq!(ctx.content, "hello");
        assert!(ctx.is_continuation);
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
}
