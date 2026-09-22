//! `default`-type inline step handler.
//!
//! One turn of the session's configured agent drives the step: the step
//! body (after `**name**` scope interpolation) becomes the user prompt,
//! and the assistant's final response text is the step output.
//!
//! The handler reuses the full
//! [`AgentManager::send_message_notified`][crate::agent_manager::AgentManager::send_message_notified]
//! pathway — tool dispatch, permission handling, pre-LLM hooks — rather
//! than invoking a raw completion backend. Workflow steps therefore
//! behave identically to chat turns; the only distinction is that the
//! workflow orchestrator drives them instead of a human.
//!
//! # How the step learns that its turn is over
//!
//! `send_message_notified` returns a oneshot that resolves once, with the
//! [`TurnOutcome`] of the turn this step started: its status, its final text
//! and its error. The step reads no events, so a turn that a `turn:complete`
//! handler starts cannot reach it.

use async_trait::async_trait;
use crucible_core::turn::TurnStatus;
use crucible_core::workflow::{ExecContext, StepHandler, StepOutcome};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};
use tracing::warn;

use crate::agent_manager::{AgentManager, TurnOutcome};
use crate::protocol::SessionEventMessage;
use crate::workflow_handlers::interpolate::interpolate;

pub struct DaemonInlineHandler {
    session_id: String,
    agents: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    /// One LLM turn at a time per workflow run. A session has a single
    /// conversation, and `AgentManager` rejects concurrent requests on it,
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

/// The turn task clears the session's `request_state` slot before it reports
/// the outcome, but a plugin turn that a `turn:complete` handler asked for
/// claims the slot right after it, so a back-to-back step can still see
/// `ConcurrentRequest`. Retry briefly to absorb that window; a genuinely busy
/// session (for example a user turn in flight) still fails once the budget is
/// exhausted.
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

        let mut attempts = 0u32;
        let completion_rx = loop {
            match self
                .agents
                .send_message_notified(
                    &self.session_id,
                    prompt.clone(),
                    &self.event_tx,
                    false,
                    None,
                )
                .await
            {
                Ok((_message_id, rx)) => break rx,
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

        outcome_to_step(completion_rx.await)
    }
}

/// Map the terminal outcome of the step's turn to a [`StepOutcome`].
fn outcome_to_step(outcome: Result<TurnOutcome, oneshot::error::RecvError>) -> StepOutcome {
    let Ok(outcome) = outcome else {
        return StepOutcome::Fail {
            reason: "the agent turn ended without reporting an outcome".into(),
        };
    };
    match outcome.status {
        TurnStatus::Completed => StepOutcome::Advance {
            output: Some(serde_json::Value::String(outcome.final_text)),
        },
        status => StepOutcome::Fail {
            reason: outcome
                .error
                .unwrap_or_else(|| format!("the turn ended: {status:?}")),
        },
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

    fn outcome(status: TurnStatus, final_text: &str, error: Option<&str>) -> TurnOutcome {
        TurnOutcome {
            status,
            final_text: final_text.to_string(),
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn a_completed_turn_gives_its_final_text_to_the_step() {
        match outcome_to_step(Ok(outcome(TurnStatus::Completed, "answer", None))) {
            StepOutcome::Advance { output } => assert_eq!(output, Some(json!("answer"))),
            other => panic!("expected Advance, got {other:?}"),
        }
    }

    /// A failed turn carries its error text, so the step repeats it.
    #[test]
    fn a_failed_turn_fails_the_step_with_the_error_text() {
        match outcome_to_step(Ok(outcome(
            TurnStatus::Failed,
            "partial",
            Some("backend down"),
        ))) {
            StepOutcome::Fail { reason } => assert_eq!(reason, "backend down"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    /// A user cancel is not a completed step either, and it carries no error
    /// text, so the status names the reason.
    #[test]
    fn a_cancelled_turn_fails_the_step() {
        match outcome_to_step(Ok(outcome(TurnStatus::Cancelled, "partial", None))) {
            StepOutcome::Fail { reason } => assert!(reason.contains("Cancelled"), "{reason}"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    /// The turn task drops the sender only if it dies before it reports. The
    /// step fails with a reason of its own rather than hangs.
    #[tokio::test]
    async fn a_dropped_completion_channel_fails_the_step() {
        let (tx, rx) = oneshot::channel::<TurnOutcome>();
        drop(tx);
        match outcome_to_step(rx.await) {
            StepOutcome::Fail { reason } => {
                assert!(reason.contains("without reporting"), "{reason}")
            }
            other => panic!("expected Fail, got {other:?}"),
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
