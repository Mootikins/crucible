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
            let diffs = match event.data.get("diffs") {
                Some(raw) => match serde_json::from_value(raw.clone()) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        tracing::warn!(
                            target: "rpc_client",
                            error = %err,
                            tool = ?event.data.get("tool"),
                            "tool_call event carried a malformed `diffs` field; \
                             ignoring and continuing with empty Vec",
                        );
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            vec![TurnEvent::ToolCall {
                id,
                name: tool.to_string(),
                args,
                diffs,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::turn::{StopReason, TurnError};
    use serde_json::json;

    fn event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent {
            session_id: "test".to_string(),
            event_type: event_type.to_string(),
            data,
        }
    }

    #[test]
    fn text_delta_maps_to_text_delta_event() {
        let out =
            session_event_to_turn_events(&event("text_delta", json!({ "content": "Hello world" })));
        match out.as_slice() {
            [TurnEvent::TextDelta(s)] => assert_eq!(s, "Hello world"),
            other => panic!("expected single TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn thinking_maps_to_thinking_event() {
        let out = session_event_to_turn_events(&event(
            "thinking",
            json!({ "content": "Let me think..." }),
        ));
        match out.as_slice() {
            [TurnEvent::Thinking(s)] => assert_eq!(s, "Let me think..."),
            other => panic!("expected single Thinking, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_maps_to_tool_call_event() {
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({
                "call_id": "tc-123",
                "tool": "search",
                "args": { "query": "rust async" }
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall { id, name, args, .. }] => {
                assert_eq!(id, "tc-123");
                assert_eq!(name, "search");
                assert_eq!(args, &json!({ "query": "rust async" }));
            }
            other => panic!("expected single ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_without_call_id_defaults_to_empty_string() {
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({ "tool": "search", "args": { "query": "test" } }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall { id, name, args, .. }] => {
                assert_eq!(id, "");
                assert_eq!(name, "search");
                assert_eq!(args, &json!({ "query": "test" }));
            }
            other => panic!("expected single ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_without_args_defaults_to_null() {
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({ "call_id": "tc-1", "tool": "list_files" }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall { name, args, .. }] => {
                assert_eq!(name, "list_files");
                assert_eq!(args, &serde_json::Value::Null);
            }
            other => panic!("expected single ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_without_tool_name_is_dropped() {
        let out = session_event_to_turn_events(&event("tool_call", json!({ "call_id": "tc-1" })));
        assert!(out.is_empty());
    }

    #[test]
    fn tool_call_with_malformed_diffs_falls_back_to_empty_vec() {
        // Wire-protocol drift safety: if the daemon ever sends a `diffs`
        // field that isn't a Vec<FileDiff> (older clients, schema bugs,
        // hand-edited replay logs), we must not panic — just log and emit
        // an empty Vec so the rest of the TurnEvent stays usable.
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({
                "call_id": "tc-1",
                "tool": "edit_file",
                "args": {},
                "diffs": "this is not a list"
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall { diffs, .. }] => assert!(diffs.is_empty()),
            other => panic!("expected single ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_call_with_well_formed_diffs_passes_through() {
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({
                "call_id": "tc-1",
                "tool": "edit_file",
                "args": {},
                "diffs": [{
                    "path": "/tmp/foo.rs",
                    "old_content": "fn old() {}",
                    "new_content": "fn new() {}"
                }]
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall { diffs, .. }] => {
                assert_eq!(diffs.len(), 1);
                assert_eq!(diffs[0].path, "/tmp/foo.rs");
            }
            other => panic!("expected single ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_preserves_object_value_and_routes_error_field() {
        let out = session_event_to_turn_events(&event(
            "tool_result",
            json!({
                "call_id": "tc-denied",
                "tool": "bash",
                "result": { "error": "User denied permission" }
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolResult {
                id,
                name,
                result,
                error,
            }] => {
                assert_eq!(id, "tc-denied");
                assert_eq!(name, "bash");
                assert_eq!(result, &json!({ "error": "User denied permission" }));
                assert_eq!(error.as_deref(), Some("User denied permission"));
            }
            other => panic!("expected single ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_without_error_has_none_error_field() {
        let out = session_event_to_turn_events(&event(
            "tool_result",
            json!({
                "call_id": "tc-ok",
                "tool": "read_file",
                "result": "file contents"
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolResult {
                name,
                result,
                error,
                ..
            }] => {
                assert_eq!(name, "read_file");
                assert_eq!(result, &json!("file contents"));
                assert!(error.is_none());
            }
            other => panic!("expected single ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_without_result_field_is_dropped() {
        let out = session_event_to_turn_events(&event(
            "tool_result",
            json!({ "call_id": "tc-1", "tool": "read_file" }),
        ));
        assert!(out.is_empty());
    }

    #[test]
    fn message_complete_yields_done_with_usage() {
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({
                "prompt_tokens": 200,
                "completion_tokens": 80,
                "total_tokens": 280
            }),
        ));
        match out.as_slice() {
            [TurnEvent::Usage(usage), TurnEvent::Done { stop_reason }] => {
                assert_eq!(usage.prompt_tokens, 200);
                assert_eq!(usage.completion_tokens, 80);
                assert_eq!(usage.total_tokens, 280);
                assert!(usage.cache_read_tokens.is_none());
                assert!(usage.cache_creation_tokens.is_none());
                assert_eq!(*stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected Usage + Done, got {other:?}"),
        }
    }

    #[test]
    fn message_complete_without_token_fields_yields_only_done() {
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({ "message_id": "m-1" }),
        ));
        match out.as_slice() {
            [TurnEvent::Done { stop_reason }] => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected single Done, got {other:?}"),
        }
    }

    #[test]
    fn message_complete_extracts_cache_tokens() {
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({
                "prompt_tokens": 1000,
                "completion_tokens": 200,
                "total_tokens": 1200,
                "cache_read_tokens": 800,
                "cache_creation_tokens": 150
            }),
        ));
        match out.as_slice() {
            [TurnEvent::Usage(usage), TurnEvent::Done { .. }] => {
                assert_eq!(usage.cache_read_tokens, Some(800));
                assert_eq!(usage.cache_creation_tokens, Some(150));
            }
            other => panic!("expected Usage + Done, got {other:?}"),
        }
    }

    #[test]
    fn message_complete_defaults_missing_prompt_completion_to_zero() {
        // total_tokens present but prompt/completion missing → should still extract
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({ "total_tokens": 500 }),
        ));
        match out.as_slice() {
            [TurnEvent::Usage(usage), TurnEvent::Done { .. }] => {
                assert_eq!(usage.total_tokens, 500);
                assert_eq!(usage.prompt_tokens, 0);
                assert_eq!(usage.completion_tokens, 0);
            }
            other => panic!("expected Usage + Done, got {other:?}"),
        }
    }

    #[test]
    fn ended_without_error_prefix_yields_done() {
        let out = session_event_to_turn_events(&event("ended", json!({ "reason": "complete" })));
        match out.as_slice() {
            [TurnEvent::Done { stop_reason }] => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected single Done, got {other:?}"),
        }
    }

    #[test]
    fn ended_with_bare_error_prefix_surfaces_communication_error() {
        let out = session_event_to_turn_events(&event(
            "ended",
            json!({ "reason": "error: connection refused" }),
        ));
        match out.as_slice() {
            [TurnEvent::Error(TurnError::Communication(msg))] => {
                assert_eq!(msg, "connection refused");
            }
            other => panic!("expected single Communication error, got {other:?}"),
        }
    }

    #[test]
    fn ended_strips_chat_error_display_prefixes() {
        // Each PREFIX listed in `session_event_to_turn_events` should be
        // stripped from the inner error reason so the event surfaces a
        // clean single message.
        let cases = &[
            ("error: Connection error: refused", "refused"),
            ("error: Communication error: LLM timeout", "LLM timeout"),
            ("error: Mode change error: bad mode", "bad mode"),
            ("error: Command execution failed: exit 1", "exit 1"),
            ("error: Invalid input: missing field", "missing field"),
            ("error: Agent not available: down", "down"),
            ("error: Internal error: panic", "panic"),
            ("error: Invalid mode: debug", "debug"),
            (
                "error: Operation not supported: switch_model",
                "switch_model",
            ),
        ];
        for (reason, expected) in cases {
            let out = session_event_to_turn_events(&event("ended", json!({ "reason": reason })));
            match out.as_slice() {
                [TurnEvent::Error(TurnError::Communication(msg))] => {
                    assert_eq!(msg, expected, "reason {reason:?}");
                }
                other => panic!("reason {reason:?}: expected Communication error, got {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_event_type_yields_empty() {
        let out = session_event_to_turn_events(&event("unknown_event", json!({})));
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_text_delta_without_content_yields_empty() {
        let out = session_event_to_turn_events(&event("text_delta", json!({})));
        assert!(out.is_empty());
    }

    #[test]
    fn interaction_events_are_not_translated() {
        // interaction_requested and precognition_complete ride on the raw
        // SessionEvent stream to the TUI; they must not produce TurnEvents.
        let out = session_event_to_turn_events(&event(
            "interaction_requested",
            json!({ "request_id": "r-1" }),
        ));
        assert!(out.is_empty());

        let out = session_event_to_turn_events(&event(
            "precognition_complete",
            json!({ "notes_count": 2 }),
        ));
        assert!(out.is_empty());
    }
}
