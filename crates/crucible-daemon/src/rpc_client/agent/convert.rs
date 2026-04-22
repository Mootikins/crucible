//! Event conversion helpers for `DaemonAgentHandle`.
//!
//! Converts daemon `SessionEvent`s into `TurnEvent`s for the native
//! `Agent::turn` path, and routes interaction events onto a separate
//! channel for the TUI event loop.

use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::llm::TokenUsage;
use crucible_core::turn::{StopReason, TurnEvent};
use tokio::sync::mpsc;

use crate::SessionEvent;

/// Background task that routes events from daemon to appropriate channels
///
/// Uses a `watch::Receiver` for the session_id so `clear_history` can atomically
/// switch the router to a new session without restarting this task.
///
/// Routing:
/// - `interaction_requested` → parsed and forwarded on `interaction_tx`
/// - all others → forwarded on `raw_event_tx` if present (live TUI path),
///   otherwise on `streaming_tx` (consumed by `Agent::turn`)
pub(super) async fn event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    streaming_tx: mpsc::UnboundedSender<SessionEvent>,
    interaction_tx: mpsc::UnboundedSender<InteractionEvent>,
    raw_event_tx: Option<mpsc::UnboundedSender<SessionEvent>>,
    session_id_rx: tokio::sync::watch::Receiver<String>,
) {
    while let Some(event) = event_rx.recv().await {
        let current_session_id = session_id_rx.borrow().clone();
        if event.session_id != current_session_id {
            tracing::trace!(
                event_session = %event.session_id,
                expected_session = %current_session_id,
                "Filtering event from different session in router"
            );
            continue;
        }

        if event.event_type == "interaction_requested" {
            if let (Some(request_id), Some(request_data)) = (
                event.data.get("request_id").and_then(|v| v.as_str()),
                event.data.get("request"),
            ) {
                match serde_json::from_value(request_data.clone()) {
                    Ok(request) => {
                        let interaction_event = InteractionEvent {
                            request_id: request_id.to_string(),
                            request,
                        };
                        if interaction_tx.send(interaction_event).is_err() {
                            tracing::debug!("Interaction channel closed");
                            break;
                        }
                        tracing::debug!(request_id = %request_id, "Routed interaction event");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to deserialize interaction request");
                    }
                }
            } else {
                tracing::warn!("Interaction event missing request_id or request data");
            }
        } else if let Some(raw_tx) = raw_event_tx.as_ref() {
            if raw_tx.send(event).is_err() {
                tracing::debug!("Raw event channel closed");
                break;
            }
        } else if streaming_tx.send(event).is_err() {
            tracing::debug!("Streaming channel closed");
            break;
        }
    }
    tracing::debug!("Event router task ended");
}

/// Extract token usage from a `message_complete` event's data.
fn token_usage_from_event(event: &SessionEvent) -> Option<TokenUsage> {
    let total = event.data.get("total_tokens")?.as_u64()?;
    let prompt = event
        .data
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion = event
        .data
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some(TokenUsage {
        prompt_tokens: prompt as u32,
        completion_tokens: completion as u32,
        total_tokens: total as u32,
        cache_read_tokens: event
            .data
            .get("cache_read_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        cache_creation_tokens: event
            .data
            .get("cache_creation_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    })
}

/// Convert a `SessionEvent` into zero or more `TurnEvent`s.
///
/// Daemon-proxy path: the daemon runs the tool loop internally, so the
/// client only observes events and never replies on an inbound channel.
/// `message_complete` and `ended` both map to a terminal `Done` — whichever
/// comes first finishes the turn.
///
/// An `ended` event whose `reason` starts with `"error: "` maps to
/// `TurnEvent::Error(TurnError::Communication(...))` instead of `Done`.
pub(super) fn session_event_to_turn_events(event: &SessionEvent) -> Vec<TurnEvent> {
    use crucible_core::turn::TurnError;

    match event.event_type.as_str() {
        "text_delta" => event
            .data
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| vec![TurnEvent::TextDelta(c.to_string())])
            .unwrap_or_default(),
        "thinking" => event
            .data
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| vec![TurnEvent::Thinking(c.to_string())])
            .unwrap_or_default(),
        "tool_call" => {
            let Some(tool) = event.data.get("tool").and_then(|v| v.as_str()) else {
                return Vec::new();
            };
            let id = event
                .data
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let args = event
                .data
                .get("args")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            vec![TurnEvent::ToolCall {
                id,
                name: tool.to_string(),
                args,
            }]
        }
        "tool_result" => {
            let Some(result_val) = event.data.get("result") else {
                return Vec::new();
            };
            let id = event
                .data
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = event
                .data
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let error = result_val
                .get("error")
                .and_then(|e| e.as_str())
                .map(String::from);
            vec![TurnEvent::ToolResult {
                id,
                name,
                result: result_val.clone(),
                error,
            }]
        }
        "message_complete" => {
            let mut events = Vec::new();
            if let Some(usage) = token_usage_from_event(event) {
                events.push(TurnEvent::Usage(usage));
            }
            events.push(TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            });
            events
        }
        "ended" => {
            let reason = event
                .data
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if let Some(inner) = reason.strip_prefix("error: ") {
                // Strip any leading ChatError Display prefix so the event
                // surfaces a clean single message.
                const PREFIXES: &[&str] = &[
                    "Connection error: ",
                    "Communication error: ",
                    "Mode change error: ",
                    "Command execution failed: ",
                    "Invalid input: ",
                    "Agent not available: ",
                    "Internal error: ",
                    "Invalid mode: ",
                    "Operation not supported: ",
                ];
                let stripped = PREFIXES
                    .iter()
                    .find_map(|p| inner.strip_prefix(p))
                    .unwrap_or(inner);
                vec![TurnEvent::Error(TurnError::Communication(
                    stripped.to_string(),
                ))]
            } else {
                vec![TurnEvent::Done {
                    stop_reason: StopReason::EndTurn,
                }]
            }
        }
        _ => {
            tracing::debug!("Unknown session event type: {}", event.event_type);
            Vec::new()
        }
    }
}
