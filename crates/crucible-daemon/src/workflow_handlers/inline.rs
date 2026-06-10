//! `default`-type inline step handler.
//!
//! One turn of the session's configured agent drives the step: the step
//! body (after `**name**` scope interpolation) becomes the user prompt,
//! and the assistant's final response text is the step output.
//!
//! The handler reuses the full
//! [`AgentManager::send_message`][crate::agent_manager::AgentManager::send_message]
//! pathway — tool dispatch, permission handling, pre-LLM hooks — rather
//! than invoking a raw completion backend. Workflow steps therefore
//! behave identically to chat turns; the only distinction is that the
//! workflow orchestrator drives them instead of a human.
//!
//! # Event flow
//!
//! We subscribe to `event_tx` before calling `send_message` so no queued
//! events race past us. For a plain turn the sequence is simply:
//! `user_message → text_delta* → message_complete → post_llm_call`.
//!
//! Reactor `turn_complete` handlers can inject a continuation — see
//! `agent_manager/messaging/stream.rs::execute_agent_stream`. Each
//! recursion emits a fresh `message_complete` (with a new `message_id`)
//! and its own `post_llm_call` on the way back up the stack. The
//! DEEPEST stream emits the final-text `message_complete` immediately
//! followed by the first `post_llm_call`, so we track the latest seen
//! `full_response` and commit on the first `post_llm_call`. Outer
//! `post_llm_call`s arrive after we've already returned; the
//! session-level `request_state` guard blocks any unrelated turn on
//! this session while we're mid-step, so same-session foreign events
//! can't interleave.

use async_trait::async_trait;
use crucible_core::workflow::{ExecContext, StepHandler, StepOutcome};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::warn;

use crate::agent_manager::AgentManager;
use crate::protocol::SessionEventMessage;
use crate::workflow_handlers::interpolate::interpolate;

pub struct DaemonInlineHandler {
    session_id: String,
    agents: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    /// One LLM turn at a time per workflow run. A session has a single
    /// conversation: `AgentManager` rejects concurrent requests on it,
    /// and `await_turn_completion` correlates events by session alone —
    /// so parallel-group members must serialize their turns here. True
    /// turn concurrency needs sub-session dispatch (`fan`, future).
    turn_guard: tokio::sync::Mutex<()>,
}

impl DaemonInlineHandler {
    pub fn new(
        session_id: impl Into<String>,
        agents: Arc<AgentManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            agents,
            event_tx,
            turn_guard: tokio::sync::Mutex::new(()),
        }
    }
}

/// `post_llm_call` (our completion signal) is emitted slightly before
/// the turn task clears the session's `request_state` slot, so a
/// back-to-back step can transiently see `ConcurrentRequest`. Retry
/// briefly to absorb that window; a genuinely busy session (e.g. a user
/// turn in flight) still fails once the budget is exhausted.
const SEND_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);
const SEND_RETRY_BUDGET: u32 = 40;

#[async_trait]
impl StepHandler for DaemonInlineHandler {
    async fn execute(&self, ctx: &ExecContext<'_>) -> StepOutcome {
        let body = interpolate(&ctx.step.body, ctx.scope);
        if body.trim().is_empty() {
            // A body-less step contributes no agent work — skip ahead
            // rather than kicking off an empty turn. The heading alone
            // is still useful as an orchestration marker.
            return StepOutcome::Advance { output: None };
        }

        // `@agent` sub-session dispatch is Slice 5 (fan) territory. For
        // now the inline handler always runs against the session's
        // configured agent; the annotation is retained in events for
        // observability.
        if let Some(name) = ctx.step.agent.as_deref() {
            warn!(
                session_id = %self.session_id,
                step_id = %ctx.step_id,
                agent = %name,
                "`@agent` annotation not yet dispatched; using session's default agent"
            );
        }

        let prompt = compose_prompt(&ctx.step.title, &body);

        let _turn = self.turn_guard.lock().await;

        // The returned message_id is useful for observability only —
        // continuations use fresh ids and neither `post_llm_call` nor
        // `ended` carry one today, so we don't filter on it. See the
        // module-level event-flow notes.
        let mut attempts = 0u32;
        let mut rx = loop {
            // Subscribe before send so queued events don't race past us.
            // Re-subscribe on retry so a failed attempt's buffered events
            // (from the turn we were waiting out) don't leak into ours.
            let rx = self.event_tx.subscribe();
            match self
                .agents
                .send_message(&self.session_id, prompt.clone(), &self.event_tx, false, None)
                .await
            {
                Ok(_id) => break rx,
                Err(crate::agent_manager::AgentError::ConcurrentRequest(_))
                    if attempts < SEND_RETRY_BUDGET =>
                {
                    attempts += 1;
                    tokio::time::sleep(SEND_RETRY_DELAY).await;
                }
                Err(e) => {
                    return StepOutcome::Fail {
                        reason: format!("failed to start agent turn: {e}"),
                    };
                }
            }
        };

        await_turn_completion(&mut rx, &self.session_id).await
    }
}

/// Block on `rx` until we've seen one full agent turn (including any
/// `turn_complete`-handler injection continuations) and return the
/// matching [`StepOutcome`]. Extracted so tests can drive the loop
/// directly through a `broadcast::Sender` without spinning up an
/// [`AgentManager`].
async fn await_turn_completion(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    session_id: &str,
) -> StepOutcome {
    let mut latest_response: Option<String> = None;
    loop {
        let msg = match rx.recv().await {
            Ok(m) => m,
            Err(broadcast::error::RecvError::Closed) => {
                return StepOutcome::Fail {
                    reason: "event stream closed before agent turn completed".into(),
                };
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                // Broadcast capacity exceeded — we may have dropped
                // the `message_complete` or `post_llm_call` we were
                // waiting for. Fail deterministically rather than
                // hang. Operator can increase broadcast capacity
                // or reduce concurrent subscriber load.
                return StepOutcome::Fail {
                    reason: format!(
                        "broadcast lagged {n} events while waiting for turn completion; \
                         workflow can't correlate the final response"
                    ),
                };
            }
        };
        if msg.session_id != session_id {
            continue;
        }
        match msg.event.as_str() {
            "message_complete" => {
                // Any message_complete on this session belongs to our
                // turn (concurrency on the same session is blocked by
                // `AgentManager::request_state`). Continuation turns
                // created by `turn_complete` handler injection use
                // fresh message_ids, so we don't filter — the deepest
                // turn's full_response is the one we'll ultimately
                // return.
                let full = msg
                    .data
                    .get("full_response")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                latest_response = Some(full);
            }
            "post_llm_call" => {
                // The first post_llm_call marks the end of the
                // deepest stream; subsequent ones are outer frames
                // unwinding with no new content. Commit accumulated.
                return StepOutcome::Advance {
                    output: Some(serde_json::Value::String(
                        latest_response.unwrap_or_default(),
                    )),
                };
            }
            "ended" => {
                // `ended` is emitted on abort paths (cancel, stream
                // error, timeout, empty response, permission denial).
                // Normal completion flows through `post_llm_call`.
                let reason = msg
                    .data
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                return StepOutcome::Fail { reason };
            }
            _ => continue,
        }
    }
}

fn compose_prompt(title: &str, body: &str) -> String {
    let body = body.trim();
    if title.is_empty() {
        body.to_string()
    } else {
        format!("# {title}\n\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SID: &str = "test-session";

    fn msg(event: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new(SID, event, data)
    }

    fn message_complete(id: &str, text: &str) -> SessionEventMessage {
        msg(
            "message_complete",
            json!({ "message_id": id, "full_response": text }),
        )
    }

    fn post_llm_call() -> SessionEventMessage {
        msg("post_llm_call", json!({}))
    }

    #[tokio::test]
    async fn plain_turn_returns_final_text() {
        let (tx, mut rx) = broadcast::channel(16);
        tx.send(message_complete("msg-a", "answer")).unwrap();
        tx.send(post_llm_call()).unwrap();

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Advance { output } => {
                assert_eq!(output, Some(json!("answer")));
            }
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn injection_chain_commits_deepest_response() {
        // Mirrors the production event order when a turn_complete
        // handler injects a continuation: each recursion emits its own
        // message_complete, the deepest post_llm_call fires first, and
        // subsequent post_llm_calls (from outer frames unwinding) must
        // not override the captured response.
        let (tx, mut rx) = broadcast::channel(32);
        tx.send(message_complete("msg-a", "initial")).unwrap();
        tx.send(msg("injection_pending", json!({ "content": "more" })))
            .unwrap();
        tx.send(message_complete("msg-b", "middle")).unwrap();
        tx.send(msg("injection_pending", json!({ "content": "even more" })))
            .unwrap();
        tx.send(message_complete("msg-c", "final answer")).unwrap();
        tx.send(post_llm_call()).unwrap(); // deepest — we should commit here
        tx.send(post_llm_call()).unwrap(); // outer — not reached
        tx.send(post_llm_call()).unwrap(); // outermost — not reached

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Advance { output } => {
                assert_eq!(output, Some(json!("final answer")));
            }
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ended_event_fails_with_reason() {
        let (tx, mut rx) = broadcast::channel(8);
        tx.send(msg("ended", json!({ "reason": "error: backend down" })))
            .unwrap();

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Fail { reason } => assert_eq!(reason, "error: backend down"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn closed_channel_fails_with_clear_reason() {
        let (tx, mut rx) = broadcast::channel(4);
        drop(tx);

        let outcome = await_turn_completion(&mut rx, SID).await;
        assert!(
            matches!(outcome, StepOutcome::Fail { reason } if reason.contains("event stream closed")),
            "expected close-specific Fail"
        );
    }

    #[tokio::test]
    async fn lagged_receiver_fails_rather_than_hangs() {
        // Capacity 2; publish 5 events before the receiver reads any.
        // The subscribe must happen BEFORE the first send — matching
        // the production invariant — and then we overflow.
        let (tx, mut rx) = broadcast::channel(2);
        for _ in 0..5 {
            let _ = tx.send(msg("text_delta", json!({ "content": "..." })));
        }
        // Now emit the completion the handler is actually waiting for;
        // by this point it's been dropped from the queue.
        let _ = tx.send(post_llm_call());

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Fail { reason } => {
                assert!(
                    reason.contains("lagged"),
                    "expected lag-specific reason, got {reason:?}"
                );
            }
            other => panic!("expected Fail on lag, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn foreign_session_events_are_ignored() {
        let (tx, mut rx) = broadcast::channel(16);
        // Event on a different session — must be skipped.
        tx.send(SessionEventMessage::new(
            "other-session",
            "post_llm_call",
            json!({}),
        ))
        .unwrap();
        tx.send(message_complete("msg-a", "ours")).unwrap();
        tx.send(post_llm_call()).unwrap();

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Advance { output } => assert_eq!(output, Some(json!("ours"))),
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn intermediate_events_between_completions_dont_clobber() {
        // Text deltas and tool calls interleave with message_completes
        // in real streams; make sure we don't confuse them for end
        // signals or fresh responses.
        let (tx, mut rx) = broadcast::channel(32);
        tx.send(msg("user_message", json!({}))).unwrap();
        tx.send(msg("text_delta", json!({ "content": "par" })))
            .unwrap();
        tx.send(msg("text_delta", json!({ "content": "tial" })))
            .unwrap();
        tx.send(msg("tool_call", json!({ "tool": "x", "args": {} })))
            .unwrap();
        tx.send(msg("tool_result", json!({ "tool": "x", "result": "ok" })))
            .unwrap();
        tx.send(message_complete("msg-a", "done")).unwrap();
        tx.send(post_llm_call()).unwrap();

        let outcome = await_turn_completion(&mut rx, SID).await;
        match outcome {
            StepOutcome::Advance { output } => assert_eq!(output, Some(json!("done"))),
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    #[test]
    fn compose_prompt_with_title() {
        assert_eq!(compose_prompt("Plan", "analyze X"), "# Plan\n\nanalyze X");
    }

    #[test]
    fn compose_prompt_without_title() {
        assert_eq!(compose_prompt("", "just body"), "just body");
    }
}
