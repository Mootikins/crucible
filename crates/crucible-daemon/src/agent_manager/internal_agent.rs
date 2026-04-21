//! `InternalAgent` — the `Agent` trait implementation used by the daemon's
//! internal (genai-backed) agents.
//!
//! Wraps an `Arc<Mutex<BoxedAgentHandle>>` (typically a `GenaiAgentHandle`)
//! and drives the tool loop on the handle's `send_message_stream` /
//! `continue_with_tool_results` interface, translating its `ChatChunk`
//! output into `TurnEvent`s. ACP and daemon-proxy backends now have native
//! `Agent` impls on their concrete types; this bridge exists for backends
//! whose tool loop requires explicit continuation calls (currently just
//! the genai provider).
//!
//! ## Channel topology
//!
//! `InternalAgent` realises the plan's "one channel topology, not three"
//! rule: all three tool-loop re-entry points arrive on the same
//! inbound `mpsc<TurnEvent>`:
//!
//! - `ToolResult` — runtime dispatched the agent's tool call and is
//!   feeding the result back. `InternalAgent` calls the underlying
//!   handle's `continue_with_tool_results`.
//! - `HandlerInjection` — runtime's post-turn handler returned injection
//!   content. Restarts the inner stream with the injected content as
//!   the next user message.
//! - `DepthCapHit` — runtime reached `max_tool_depth`. Restarts the
//!   inner stream with the depth-cap prompt so the model produces a
//!   final response without further tool calls.

use std::sync::Arc;

use async_stream::stream;
use async_trait::async_trait;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatToolCall, ChatToolResult,
};
use crucible_core::turn::{
    Agent, AgentCapabilities, AgentError, NotSupported, StopReason, TurnContext, TurnError,
    TurnEvent,
};
use futures::stream::BoxStream;
use futures::StreamExt;
use tokio::sync::{mpsc, Mutex};

/// Depth-cap prompt sent back to the agent when `max_tool_depth` is
/// reached. Kept in sync with `agent_manager::messaging::TOOL_DEPTH_LIMIT_FINAL_PROMPT`.
pub const DEPTH_CAP_PROMPT: &str = "You have reached the tool call limit. Please provide your final answer based on the information gathered so far.";

// Re-use the top-level alias defined in `agent_manager::mod`.
pub(crate) use super::BoxedAgentHandle;

/// Wraps an existing `AgentHandle` and exposes it through the `Agent`
/// trait. Capabilities are supplied at construction because
/// `AgentHandle` has no first-class capability discovery.
pub struct InternalAgent {
    inner: Arc<Mutex<BoxedAgentHandle>>,
    capabilities: AgentCapabilities,
}

impl InternalAgent {
    pub fn new(inner: Arc<Mutex<BoxedAgentHandle>>, capabilities: AgentCapabilities) -> Self {
        Self {
            inner,
            capabilities,
        }
    }

    /// The baseline set of capabilities for an internal (genai) agent.
    pub fn internal_capabilities() -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: true,
            usage_reporting: true,
            cancellation: true,
            temperature_control: true,
            max_tokens_control: true,
            owns_history: false,
            modes: true,
        }
    }

    /// The baseline set of capabilities for an ACP-proxied agent.
    pub fn acp_capabilities() -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: true,
            usage_reporting: true,
            cancellation: true,
            temperature_control: false,
            max_tokens_control: false,
            // ACP agents own their own history; clearing it is not a
            // simple operation on this side of the protocol.
            owns_history: true,
            modes: true,
        }
    }

    /// Capabilities for a daemon RPC proxy (TUI-side). Capabilities are
    /// authoritatively decided by the daemon; the proxy claims the
    /// superset.
    pub fn daemon_proxy_capabilities() -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: true,
            usage_reporting: true,
            cancellation: true,
            temperature_control: true,
            max_tokens_control: true,
            owns_history: false,
            modes: true,
        }
    }
}

/// Decompose a `ChatChunk` into zero or more `TurnEvent`s. Returns any
/// tool calls the chunk carried — the caller needs them verbatim to
/// feed back through `continue_with_tool_results` once the runtime has
/// dispatched the tools.
pub(crate) fn chat_chunk_to_events(
    chunk: ChatChunk,
    events: &mut Vec<TurnEvent>,
) -> Option<Vec<ChatToolCall>> {
    if let Some(reasoning) = chunk.reasoning {
        events.push(TurnEvent::Thinking(reasoning));
    }
    if !chunk.delta.is_empty() {
        events.push(TurnEvent::TextDelta(chunk.delta));
    }
    let carried_tool_calls = chunk.tool_calls.filter(|c| !c.is_empty());
    if let Some(calls) = &carried_tool_calls {
        for call in calls {
            events.push(TurnEvent::ToolCall {
                id: call.id.clone().unwrap_or_default(),
                name: call.name.clone(),
                args: call.arguments.clone().unwrap_or(serde_json::Value::Null),
            });
        }
    }
    if let Some(results) = chunk.tool_results {
        for r in results {
            events.push(TurnEvent::ToolResult {
                id: r.call_id.unwrap_or_default(),
                name: r.name,
                result: serde_json::Value::String(r.result),
                error: r.error,
            });
        }
    }
    if let Some(usage) = chunk.usage {
        events.push(TurnEvent::Usage(usage));
    }
    carried_tool_calls
}

/// Convert an inbound `TurnEvent::ToolResult` into the legacy
/// `ChatToolResult` shape the handle's `continue_with_tool_results`
/// expects.
fn tool_result_from_event(event: &TurnEvent) -> Option<ChatToolResult> {
    match event {
        TurnEvent::ToolResult {
            id,
            name,
            result,
            error,
        } => {
            let result_str = match result {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Some(ChatToolResult {
                name: name.clone(),
                result: result_str,
                error: error.clone(),
                call_id: Some(id.clone()),
            })
        }
        _ => None,
    }
}

#[async_trait]
impl Agent for InternalAgent {
    fn capabilities(&self) -> AgentCapabilities {
        self.capabilities
    }

    async fn turn(
        &mut self,
        ctx: TurnContext,
    ) -> Result<BoxStream<'static, TurnEvent>, AgentError> {
        let handle = self.inner.clone();
        let initial = ctx.content;
        let mut inbound: Option<mpsc::Receiver<TurnEvent>> = ctx.inbound;

        let body = stream! {
            // Held for the entire turn; re-entry points drop+reacquire
            // through the `continue_with_tool_results` call which needs
            // &mut self on the inner handle.
            let mut guard = handle.lock().await;
            let mut chat_stream = guard.send_message_stream(initial);

            'turn: loop {
                let mut done = false;
                let mut pending_calls: Option<Vec<ChatToolCall>> = None;

                while let Some(result) = chat_stream.next().await {
                    match result {
                        Ok(chunk) => {
                            let terminal = chunk.done;
                            let mut events = Vec::new();
                            let carried_calls = chat_chunk_to_events(chunk, &mut events);
                            for event in events {
                                yield event;
                            }
                            if terminal {
                                pending_calls = carried_calls;
                                done = true;
                                break;
                            }
                        }
                        Err(ChatError::NotSupported(_)) => {
                            // Tool continuation not supported (e.g. ACP
                            // agents): surface clean end so the caller
                            // can rely on `Done` for UI completion.
                            yield TurnEvent::Done {
                                stop_reason: StopReason::EndTurn,
                            };
                            return;
                        }
                        Err(e) => {
                            yield TurnEvent::Error(TurnError::Communication(e.to_string()));
                            return;
                        }
                    }
                }

                if !done {
                    yield TurnEvent::Done {
                        stop_reason: StopReason::Empty,
                    };
                    return;
                }

                let Some(tool_calls) = pending_calls else {
                    yield TurnEvent::Done {
                        stop_reason: StopReason::EndTurn,
                    };
                    return;
                };

                // Signal the batch boundary so the runtime can tick
                // tool-depth once per batch regardless of fan-out.
                yield TurnEvent::ToolBatchEnd;

                let Some(rx) = inbound.as_mut() else {
                    // Caller opted out of tool continuation; honour the
                    // call's visibility but stop there.
                    yield TurnEvent::Done {
                        stop_reason: StopReason::EndTurn,
                    };
                    return;
                };

                // Collect one ToolResult per outstanding tool call,
                // matched by call id. HandlerInjection / DepthCapHit
                // short-circuit the collection and restart the inner
                // stream.
                let expected_ids: std::collections::HashSet<String> = tool_calls
                    .iter()
                    .filter_map(|c| c.id.clone())
                    .collect();
                let mut collected: Vec<ChatToolResult> = Vec::with_capacity(tool_calls.len());
                while collected.len() < tool_calls.len() {
                    let Some(event) = rx.recv().await else {
                        yield TurnEvent::Done {
                            stop_reason: StopReason::Cancelled,
                        };
                        return;
                    };

                    match event {
                        TurnEvent::ToolResult { ref id, .. } => {
                            if !expected_ids.is_empty() && !expected_ids.contains(id) {
                                // Stale result from a prior batch —
                                // silently drop so it doesn't displace
                                // an expected one.
                                continue;
                            }
                            if let Some(result) = tool_result_from_event(&event) {
                                collected.push(result);
                            }
                        }
                        TurnEvent::HandlerInjection { content, .. } => {
                            drop(chat_stream);
                            chat_stream = guard.send_message_stream(content);
                            continue 'turn;
                        }
                        TurnEvent::DepthCapHit { .. } => {
                            drop(chat_stream);
                            chat_stream = guard.send_message_stream(DEPTH_CAP_PROMPT.to_string());
                            continue 'turn;
                        }
                        _ => {
                            // Non-control event from the runtime —
                            // silently drop; this channel is meant for
                            // ToolResult / HandlerInjection / DepthCapHit.
                        }
                    }
                }

                // All tool results gathered — feed back.
                drop(chat_stream);
                chat_stream = guard.continue_with_tool_results(tool_calls, collected);
            }
        };

        Ok(Box::pin(body))
    }

    async fn cancel(&self) -> Result<(), AgentError> {
        let guard = self.inner.lock().await;
        guard
            .cancel()
            .await
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    async fn switch_model(&mut self, model_id: &str) -> Result<(), NotSupported> {
        let mut guard = self.inner.lock().await;
        match guard.switch_model(model_id).await {
            Ok(()) => Ok(()),
            Err(_) => Err(NotSupported::new("switch_model")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult, ChatToolCall};
    use crucible_core::traits::llm::TokenUsage;
    use futures::stream::{self, BoxStream};

    /// Mock that returns a fixed sequence of chunks for `send_message_stream`
    /// and, on each subsequent `continue_with_tool_results`, pops from a
    /// queue of canned follow-up streams.
    struct MockHandle {
        initial: Vec<ChatResult<ChatChunk>>,
        follow_ups: std::sync::Mutex<Vec<Vec<ChatResult<ChatChunk>>>>,
    }

    impl MockHandle {
        fn new(initial: Vec<ChatChunk>) -> Self {
            Self {
                initial: initial.into_iter().map(Ok).collect(),
                follow_ups: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn with_follow_up(self, follow_up: Vec<ChatChunk>) -> Self {
            self.follow_ups
                .lock()
                .unwrap()
                .push(follow_up.into_iter().map(Ok).collect());
            self
        }
    }

    #[async_trait]
    impl AgentHandle for MockHandle {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let chunks = std::mem::take(&mut self.initial);
            Box::pin(stream::iter(chunks))
        }

        fn continue_with_tool_results(
            &mut self,
            _tool_calls: Vec<ChatToolCall>,
            _tool_results: Vec<ChatToolResult>,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let next = self.follow_ups.lock().unwrap().pop().unwrap_or_default();
            Box::pin(stream::iter(next))
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    fn terminal_text(text: &str) -> ChatChunk {
        ChatChunk {
            delta: text.to_string(),
            done: true,
            ..Default::default()
        }
    }

    fn terminal_tool_call(name: &str, id: &str) -> ChatChunk {
        ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: Some(vec![ChatToolCall {
                name: name.to_string(),
                arguments: Some(serde_json::json!({"q": 1})),
                id: Some(id.to_string()),
            }]),
            ..Default::default()
        }
    }

    async fn collect_events(mut s: BoxStream<'static, TurnEvent>) -> Vec<TurnEvent> {
        let mut out = Vec::new();
        while let Some(e) = s.next().await {
            out.push(e);
        }
        out
    }

    #[tokio::test]
    async fn plain_text_turn_yields_delta_then_done() {
        let handle: BoxedAgentHandle = Box::new(MockHandle::new(vec![terminal_text("hello")]));
        let inner = Arc::new(Mutex::new(handle));
        let mut adapter =
            InternalAgent::new(inner, InternalAgent::internal_capabilities());

        let stream = adapter
            .turn(TurnContext::new("hi"))
            .await
            .expect("turn returned stream");

        let events = collect_events(stream).await;
        assert!(
            matches!(events.as_slice(), [TurnEvent::TextDelta(t), TurnEvent::Done { .. }] if t == "hello"),
            "unexpected events: {events:#?}"
        );
    }

    #[tokio::test]
    async fn tool_call_round_trip_via_inbound_channel() {
        // Initial chunk emits a tool call. Follow-up (after ToolResult
        // comes in on the inbound channel) is a terminal text chunk.
        let handle: BoxedAgentHandle = Box::new(
            MockHandle::new(vec![terminal_tool_call("fetch", "call-1")])
                .with_follow_up(vec![terminal_text("42")]),
        );
        let inner = Arc::new(Mutex::new(handle));
        let mut adapter =
            InternalAgent::new(inner, InternalAgent::internal_capabilities());

        let (tx, rx) = mpsc::channel(4);
        let ctx = TurnContext::new("query").with_inbound(rx);

        // Drive the turn: collect the first two events, then feed back a
        // tool result on the inbound channel, then collect the rest.
        let stream = adapter.turn(ctx).await.expect("turn started");
        let handle = tokio::spawn(collect_events(stream));

        // Give the stream a chance to emit the ToolCall before we reply.
        tokio::task::yield_now().await;
        tx.send(TurnEvent::ToolResult {
            id: "call-1".into(),
            name: "fetch".into(),
            result: serde_json::json!("ok"),
            error: None,
        })
        .await
        .unwrap();
        drop(tx);

        let events = handle.await.unwrap();
        // Expect: ToolCall, then TextDelta("42"), then Done.
        assert!(
            matches!(events.first(), Some(TurnEvent::ToolCall { name, .. }) if name == "fetch"),
            "first event was not ToolCall: {:#?}",
            events.first()
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::TextDelta(t) if t == "42")),
            "missing follow-up text delta: {events:#?}"
        );
        assert!(
            matches!(events.last(), Some(TurnEvent::Done { .. })),
            "last event was not Done: {:#?}",
            events.last()
        );
    }

    #[tokio::test]
    async fn handler_injection_restarts_stream_on_same_channel() {
        // Initial stream emits a tool call; instead of a ToolResult we
        // inject. The adapter must restart the underlying stream with
        // the injection content via `send_message_stream` (NOT
        // continue_with_tool_results), and deliver the subsequent
        // terminal text.
        let handle: BoxedAgentHandle = Box::new(MockHandle {
            initial: vec![Ok(terminal_tool_call("fetch", "call-1"))],
            follow_ups: std::sync::Mutex::new(vec![]),
        });

        struct Restartable {
            first: std::sync::Mutex<Vec<ChatResult<ChatChunk>>>,
            second: std::sync::Mutex<Vec<ChatResult<ChatChunk>>>,
        }

        #[async_trait]
        impl AgentHandle for Restartable {
            fn send_message_stream(
                &mut self,
                _message: String,
            ) -> BoxStream<'static, ChatResult<ChatChunk>> {
                let mut first = self.first.lock().unwrap();
                if !first.is_empty() {
                    let chunks = std::mem::take(&mut *first);
                    return Box::pin(stream::iter(chunks));
                }
                let chunks = std::mem::take(&mut *self.second.lock().unwrap());
                Box::pin(stream::iter(chunks))
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
                Ok(())
            }
        }

        let restartable = Restartable {
            first: std::sync::Mutex::new(vec![Ok(terminal_tool_call("fetch", "call-1"))]),
            second: std::sync::Mutex::new(vec![Ok(terminal_text("post-injection"))]),
        };
        let _ = handle; // unused in this branch

        let inner: Arc<Mutex<BoxedAgentHandle>> = Arc::new(Mutex::new(Box::new(restartable)));
        let mut adapter =
            InternalAgent::new(inner.clone(), InternalAgent::internal_capabilities());

        let (tx, rx) = mpsc::channel(4);
        let ctx = TurnContext::new("ask").with_inbound(rx);
        let stream = adapter.turn(ctx).await.unwrap();
        let handle = tokio::spawn(collect_events(stream));

        tokio::task::yield_now().await;
        tx.send(TurnEvent::HandlerInjection {
            content: "follow up".into(),
            position: "after".into(),
        })
        .await
        .unwrap();
        drop(tx);

        let events = handle.await.unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::TextDelta(t) if t == "post-injection")),
            "injection did not restart stream with new content: {events:#?}"
        );
    }

    #[tokio::test]
    async fn depth_cap_hit_restarts_stream_with_final_prompt() {
        struct DepthCapMock {
            first: std::sync::Mutex<Vec<ChatResult<ChatChunk>>>,
            captured_prompt: Arc<std::sync::Mutex<Option<String>>>,
            second: std::sync::Mutex<Vec<ChatResult<ChatChunk>>>,
        }

        #[async_trait]
        impl AgentHandle for DepthCapMock {
            fn send_message_stream(
                &mut self,
                message: String,
            ) -> BoxStream<'static, ChatResult<ChatChunk>> {
                let mut first = self.first.lock().unwrap();
                if !first.is_empty() {
                    let chunks = std::mem::take(&mut *first);
                    return Box::pin(stream::iter(chunks));
                }
                *self.captured_prompt.lock().unwrap() = Some(message);
                let chunks = std::mem::take(&mut *self.second.lock().unwrap());
                Box::pin(stream::iter(chunks))
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
                Ok(())
            }
        }

        let captured = Arc::new(std::sync::Mutex::new(None));
        let mock = DepthCapMock {
            first: std::sync::Mutex::new(vec![Ok(terminal_tool_call("search", "call-1"))]),
            captured_prompt: captured.clone(),
            second: std::sync::Mutex::new(vec![Ok(terminal_text("final"))]),
        };
        let inner: Arc<Mutex<BoxedAgentHandle>> = Arc::new(Mutex::new(Box::new(mock)));
        let mut adapter =
            InternalAgent::new(inner, InternalAgent::internal_capabilities());

        let (tx, rx) = mpsc::channel(4);
        let ctx = TurnContext::new("ask").with_inbound(rx);
        let stream = adapter.turn(ctx).await.unwrap();
        let handle = tokio::spawn(collect_events(stream));

        tokio::task::yield_now().await;
        tx.send(TurnEvent::DepthCapHit { max_depth: 10 })
            .await
            .unwrap();
        drop(tx);

        let events = handle.await.unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::TextDelta(t) if t == "final")),
            "depth-cap restart did not yield final text: {events:#?}"
        );
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEPTH_CAP_PROMPT),
            "depth-cap prompt was not replayed to the agent",
        );
    }

    #[test]
    fn token_usage_becomes_usage_event() {
        let chunk = ChatChunk {
            delta: "ok".into(),
            done: true,
            usage: Some(TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 2,
                total_tokens: 3,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            }),
            ..Default::default()
        };
        let mut events = Vec::new();
        chat_chunk_to_events(chunk, &mut events);
        assert!(
            events.iter().any(|e| matches!(e, TurnEvent::Usage(_))),
            "Usage was not emitted: {events:#?}"
        );
    }
}
