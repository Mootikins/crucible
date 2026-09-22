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
//! events race past us. The sequence of one turn is
//! `user_message → text_delta* → message_complete → turn_finished`.
//!
//! A turn ENDS on `turn_finished`, and that event carries the status of the
//! turn. The step takes the text of the one `message_complete` before it.
//!
//! A `turn:complete` handler can start a turn of its own on this session, so
//! the step reads nothing before the `user_message` that carries the id its
//! own send returned.

use async_trait::async_trait;
use crucible_core::protocol::session_events::{SessionEventPayload, TurnPayload};
use crucible_core::turn::TurnStatus;
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

/// `turn_finished` comes after the turn task clears the session's
/// `request_state` slot, but a plugin turn that a `turn:complete` handler
/// asked for claims the slot right after it, so a back-to-back step can
/// still see `ConcurrentRequest`. Retry briefly to absorb that window; a
/// genuinely busy session (for example a user turn in flight) still fails
/// once the budget is exhausted.
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

        // The returned message_id names the turn this step waits for. See
        // the module-level event-flow notes.
        let mut attempts = 0u32;
        let (mut rx, message_id) = loop {
            // Subscribe before send so queued events don't race past us.
            // Re-subscribe on retry so a failed attempt's buffered events
            // (from the turn we were waiting out) don't leak into ours.
            let rx = self.event_tx.subscribe();
            match self
                .agents
                .send_message(
                    &self.session_id,
                    prompt.clone(),
                    &self.event_tx,
                    false,
                    None,
                )
                .await
            {
                Ok(id) => break (rx, id),
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

        await_turn_completion(&mut rx, &self.session_id, &message_id).await
    }
}

/// Block on `rx` until the turn that `message_id` names is over, and return
/// the matching [`StepOutcome`]. Extracted so tests can drive the loop
/// directly through a `broadcast::Sender` without spinning up an
/// [`AgentManager`].
async fn await_turn_completion(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    session_id: &str,
    message_id: &str,
) -> StepOutcome {
    let mut latest_response: Option<String> = None;
    let mut ours = false;
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
                // the `message_complete` or the `turn_finished` we were
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
        // Nothing before our own turn: a `turn:complete` handler can start a
        // turn of its own, and its events can reach this buffer.
        if !ours {
            ours = matches!(
                msg.payload(),
                Ok(SessionEventPayload::Turn(TurnPayload::UserMessage { message_id: id, .. }))
                    if id == message_id
            );
            continue;
        }
        match msg.event.as_str() {
            "message_complete" => {
                // A turn has one message_complete, and concurrency on the
                // same session is blocked by `AgentManager::request_state`,
                // so this one belongs to our turn.
                let full = msg
                    .data
                    .get("full_response")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                latest_response = Some(full);
            }
            // The one event that ends a whole turn. It carries the status,
            // so this reads no free-form text of `ended`.
            "turn_finished" => {
                return match msg.payload() {
                    Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished {
                        status: TurnStatus::Completed,
                        ..
                    })) => StepOutcome::Advance {
                        output: Some(serde_json::Value::String(
                            latest_response.unwrap_or_default(),
                        )),
                    },
                    Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished {
                        status,
                        error,
                        ..
                    })) => StepOutcome::Fail {
                        reason: error.unwrap_or_else(|| format!("the turn ended: {status:?}")),
                    },
                    _ => StepOutcome::Fail {
                        reason: format!(
                            "the daemon sent a turn_finished event that does not decode: {}",
                            msg.data
                        ),
                    },
                };
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
    /// The id of the turn the tests wait for.
    const OURS: &str = "msg-ours";

    fn msg(event: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new(SID, event, data)
    }

    /// The `user_message` that opens the turn the tests below wait for.
    fn opening() -> SessionEventMessage {
        msg(
            "user_message",
            json!({ "message_id": OURS, "content": "go" }),
        )
    }

    fn message_complete(id: &str, text: &str) -> SessionEventMessage {
        msg(
            "message_complete",
            json!({ "message_id": id, "full_response": text }),
        )
    }

    fn turn_finished(status: &str, error: Option<&str>) -> SessionEventMessage {
        msg("turn_finished", json!({ "status": status, "error": error }))
    }

    #[tokio::test]
    async fn plain_turn_returns_final_text() {
        let (tx, mut rx) = broadcast::channel(16);
        tx.send(opening()).unwrap();
        tx.send(message_complete("msg-a", "answer")).unwrap();
        tx.send(turn_finished("completed", None)).unwrap();

        let outcome = await_turn_completion(&mut rx, SID, OURS).await;
        match outcome {
            StepOutcome::Advance { output } => {
                assert_eq!(output, Some(json!("answer")));
            }
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    /// `post_llm_call` used to end the step. It is telemetry, it comes once
    /// per provider call, and it says nothing about how the turn ended, so
    /// the step now waits for `turn_finished`.
    #[tokio::test]
    async fn post_llm_call_does_not_end_the_step() {
        let (tx, mut rx) = broadcast::channel(16);
        tx.send(opening()).unwrap();
        tx.send(message_complete("msg-a", "answer")).unwrap();
        tx.send(msg("post_llm_call", json!({}))).unwrap();
        tx.send(turn_finished("completed", None)).unwrap();

        match await_turn_completion(&mut rx, SID, OURS).await {
            StepOutcome::Advance { output } => assert_eq!(output, Some(json!("answer"))),
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    /// `ended` says why a turn stopped early, in free-form text. The step
    /// reads the status of `turn_finished` instead.
    #[tokio::test]
    async fn a_failed_turn_fails_the_step_with_the_error_text() {
        let (tx, mut rx) = broadcast::channel(8);
        tx.send(opening()).unwrap();
        tx.send(msg("ended", json!({ "reason": "error: backend down" })))
            .unwrap();
        tx.send(turn_finished("failed", Some("backend down")))
            .unwrap();

        match await_turn_completion(&mut rx, SID, OURS).await {
            StepOutcome::Fail { reason } => assert_eq!(reason, "backend down"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    /// A user cancel is not a completed step either.
    /// A `turn:complete` handler can start a turn of its own. Its events can
    /// already sit in this buffer, and the step must not end on them.
    #[tokio::test]
    async fn the_end_of_another_turn_does_not_end_the_step() {
        let (tx, mut rx) = broadcast::channel(16);
        tx.send(msg(
            "user_message",
            json!({ "message_id": "msg-plugin", "content": "keep going", "origin": "plugin" }),
        ))
        .unwrap();
        tx.send(message_complete("msg-plugin", "the plugin's answer"))
            .unwrap();
        tx.send(turn_finished("failed", Some("the plugin's turn failed")))
            .unwrap();
        tx.send(opening()).unwrap();
        tx.send(message_complete("msg-a", "ours")).unwrap();
        tx.send(turn_finished("completed", None)).unwrap();

        match await_turn_completion(&mut rx, SID, OURS).await {
            StepOutcome::Advance { output } => assert_eq!(output, Some(json!("ours"))),
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_cancelled_turn_fails_the_step() {
        let (tx, mut rx) = broadcast::channel(8);
        tx.send(opening()).unwrap();
        tx.send(turn_finished("cancelled", None)).unwrap();

        match await_turn_completion(&mut rx, SID, OURS).await {
            StepOutcome::Fail { reason } => assert!(reason.contains("Cancelled"), "{reason}"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn closed_channel_fails_with_clear_reason() {
        let (tx, mut rx) = broadcast::channel(4);
        drop(tx);

        let outcome = await_turn_completion(&mut rx, SID, OURS).await;
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
        let _ = tx.send(turn_finished("completed", None));

        let outcome = await_turn_completion(&mut rx, SID, OURS).await;
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
            "turn_finished",
            json!({ "status": "failed" }),
        ))
        .unwrap();
        tx.send(opening()).unwrap();
        tx.send(message_complete("msg-a", "ours")).unwrap();
        tx.send(turn_finished("completed", None)).unwrap();

        let outcome = await_turn_completion(&mut rx, SID, OURS).await;
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
        tx.send(opening()).unwrap();
        tx.send(msg("text_delta", json!({ "content": "par" })))
            .unwrap();
        tx.send(msg("text_delta", json!({ "content": "tial" })))
            .unwrap();
        tx.send(msg("tool_call", json!({ "tool": "x", "args": {} })))
            .unwrap();
        tx.send(msg("tool_result", json!({ "tool": "x", "result": "ok" })))
            .unwrap();
        tx.send(message_complete("msg-a", "done")).unwrap();
        tx.send(turn_finished("completed", None)).unwrap();

        let outcome = await_turn_completion(&mut rx, SID, OURS).await;
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
