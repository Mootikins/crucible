//! Event conversion helpers for `DaemonAgentHandle`.
//!
//! Converts daemon `SessionEvent`s into `TurnEvent`s for the native
//! `Agent::turn` path, and routes interaction events onto a separate
//! channel for the TUI event loop.

use crucible_core::interaction::InteractionEvent;
use crucible_core::protocol::session_events::{SessionEventPayload, ToolResultBody, TurnPayload};
use crucible_core::traits::llm::TokenUsage;
use crucible_core::turn::{StopReason, TurnEvent, TurnStatus};
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

        // Routing is decided by the NAME, before the payload is decoded: an
        // `interaction_requested` whose payload will not decode belongs on this
        // side channel with a warning, not forwarded to a consumer that cannot
        // use it.
        if event.event == "interaction_requested" {
            match event.payload() {
                Ok(SessionEventPayload::Turn(TurnPayload::InteractionRequested {
                    request_id,
                    request,
                })) => {
                    let interaction_event = InteractionEvent {
                        request_id: request_id.clone(),
                        request,
                    };
                    if interaction_tx.send(interaction_event).is_err() {
                        tracing::debug!("Interaction channel closed");
                        break;
                    }
                    tracing::debug!(request_id = %request_id, "Routed interaction event");
                }
                Ok(_) => unreachable!("`interaction_requested` is a Turn event"),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to deserialize interaction request");
                }
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

/// Token usage from a decoded `message_complete`.
///
/// `total_tokens` is the gate — absent means the provider reported no usage at
/// all, and a `TurnEvent::Usage` of zeroes would read as "0 tokens used" rather
/// than "no data". `prompt`/`completion` default to zero because a provider can
/// report a total without the split.
fn token_usage(
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
    cache_read_tokens: Option<u32>,
    cache_creation_tokens: Option<u32>,
) -> Option<TokenUsage> {
    Some(TokenUsage {
        prompt_tokens: prompt_tokens.unwrap_or(0),
        completion_tokens: completion_tokens.unwrap_or(0),
        total_tokens: total_tokens?,
        cache_read_tokens,
        cache_creation_tokens,
    })
}

/// Convert a `SessionEvent` into zero or more `TurnEvent`s.
///
/// Daemon-proxy path: the daemon runs the tool loop internally, so the
/// client only observes events and never replies on an inbound channel.
/// Only `turn_finished` ends the turn. `message_complete` seals one reply and
/// gives the usage. A failed or timed-out turn maps to `TurnEvent::Error`;
/// every other status maps to `Done`.
pub(super) fn session_event_to_turn_events(event: &SessionEvent) -> Vec<TurnEvent> {
    use crucible_core::turn::TurnError;

    let turn = match event.payload() {
        Ok(SessionEventPayload::Turn(turn)) => turn,
        // Every other group is session state, not turn content.
        Ok(_) => return Vec::new(),
        Err(e) => {
            tracing::debug!(error = %e, "no turn events for this session event");
            return Vec::new();
        }
    };

    // Exhaustive: a new turn event has to make a decision here rather than
    // vanishing into a `_` arm.
    match turn {
        TurnPayload::TextDelta { content } => {
            if content.is_empty() {
                Vec::new()
            } else {
                vec![TurnEvent::TextDelta(content)]
            }
        }
        TurnPayload::Thinking { content } => {
            if content.is_empty() {
                Vec::new()
            } else {
                vec![TurnEvent::Thinking(content)]
            }
        }
        TurnPayload::ToolCall {
            call_id,
            tool,
            args,
            display,
            ..
        } => {
            // A tool call with no name cannot be rendered or matched against a
            // permission rule, so it is dropped rather than shown as `""`.
            if tool.is_empty() {
                return Vec::new();
            }
            vec![TurnEvent::ToolCall {
                id: call_id,
                name: tool,
                args,
                call: display,
            }]
        }
        TurnPayload::ToolResult {
            call_id,
            tool,
            result,
            ..
        } => {
            // `result` defaults to `null` when the key is absent, and a result
            // event with no result is nothing to render.
            if result.is_null() {
                return Vec::new();
            }
            let error = ToolResultBody::of(&result)
                .as_ref()
                .and_then(|b| b.error())
                .map(String::from);
            vec![TurnEvent::ToolResult {
                id: call_id,
                name: tool,
                result,
                error,
            }]
        }
        TurnPayload::MessageComplete {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            ..
        } => {
            let mut events = Vec::new();
            if let Some(usage) = token_usage(
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            ) {
                events.push(TurnEvent::Usage(usage));
            }
            // The stop reason arrives again on `turn_finished`, which is
            // where the turn ends.
            events
        }
        TurnPayload::TurnFinished {
            status,
            stop_reason,
            error,
        } => {
            let error = || {
                error
                    .clone()
                    .unwrap_or_else(|| format!("the turn ended: {status:?}"))
            };
            match status {
                // The daemon's own reason, when it sent one. This path used
                // to fabricate `EndTurn`, so a proxied turn reported a
                // natural completion for a truncation the daemon had named.
                TurnStatus::Completed => vec![TurnEvent::Done {
                    stop_reason: stop_reason.unwrap_or(StopReason::EndTurn),
                }],
                TurnStatus::Cancelled => vec![TurnEvent::Done {
                    stop_reason: StopReason::Cancelled,
                }],
                // A handler stopped the turn on purpose: an end, not an
                // error of the connection.
                TurnStatus::HandlerCancelled => vec![TurnEvent::Done {
                    stop_reason: StopReason::Refusal,
                }],
                TurnStatus::Failed | TurnStatus::TimedOut => {
                    vec![TurnEvent::Error(TurnError::Communication(error()))]
                }
            }
        }
        // `user_message` is the client's own input echoed back.
        TurnPayload::UserMessage { .. }
        // Segments are additive over `message_complete`'s full text; a
        // `TurnEvent` consumer accumulates deltas and would double-count.
        | TurnPayload::SegmentComplete { .. }
        // Merge-into-existing-card updates with no `TurnEvent` equivalent.
        | TurnPayload::ToolCallUpdate { .. }
        // Interactions ride a separate channel (see `event_router`).
        | TurnPayload::InteractionRequested { .. }
        | TurnPayload::InteractionCompleted { .. }
        // Context plumbing and telemetry: presentation, not turn content.
        | TurnPayload::ContextInjected { .. }
        | TurnPayload::PrecognitionComplete { .. }
        | TurnPayload::PostLlmCall { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::turn::{StopReason, TurnError};
    use serde_json::json;

    fn event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent::new("test".to_string(), event_type.to_string(), data)
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

    /// A tool call with no name cannot be rendered or matched against a
    /// permission rule. Under typing this is a decoded payload with an empty
    /// `tool`, dropped explicitly rather than by a missing-key check.
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
        // an empty Vec so the rest of the TurnEvent stays usable. The
        // tolerance lives on `CanonicalToolCall::diffs` (`lenient_diffs`),
        // shared with the TUI translator.
        let out = session_event_to_turn_events(&event(
            "tool_call",
            json!({
                "call_id": "tc-1",
                "tool": "edit_file",
                "args": {},
                "display": {"kind": "file_edit", "tool": "edit_file", "diffs": "this is not a list"}
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall {
                call: Some(call), ..
            }] => assert!(call.diffs.is_empty()),
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
                "display": {"kind": "file_edit", "tool": "edit_file", "diffs": [{
                    "path": "/tmp/foo.rs",
                    "old_content": "fn old() {}",
                    "new_content": "fn new() {}"
                }]}
            }),
        ));
        match out.as_slice() {
            [TurnEvent::ToolCall {
                call: Some(call), ..
            }] => {
                assert_eq!(call.diffs.len(), 1);
                assert_eq!(call.diffs[0].path, "/tmp/foo.rs");
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
    fn message_complete_yields_usage_and_does_not_end_the_turn() {
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({
                "prompt_tokens": 200,
                "completion_tokens": 80,
                "total_tokens": 280,
                "stop_reason": "end_turn"
            }),
        ));
        match out.as_slice() {
            [TurnEvent::Usage(usage)] => {
                assert_eq!(usage.prompt_tokens, 200);
                assert_eq!(usage.completion_tokens, 80);
                assert_eq!(usage.total_tokens, 280);
                assert!(usage.cache_read_tokens.is_none());
                assert!(usage.cache_creation_tokens.is_none());
            }
            other => panic!("expected only Usage, got {other:?}"),
        }
    }

    #[test]
    fn message_complete_without_token_fields_yields_nothing() {
        let out = session_event_to_turn_events(&event(
            "message_complete",
            json!({ "message_id": "m-1" }),
        ));
        assert!(out.is_empty(), "got {out:?}");
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
            [TurnEvent::Usage(usage)] => {
                assert_eq!(usage.cache_read_tokens, Some(800));
                assert_eq!(usage.cache_creation_tokens, Some(150));
            }
            other => panic!("expected Usage, got {other:?}"),
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
            [TurnEvent::Usage(usage)] => {
                assert_eq!(usage.total_tokens, 500);
                assert_eq!(usage.prompt_tokens, 0);
                assert_eq!(usage.completion_tokens, 0);
            }
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    /// Each status of `turn_finished` has one ending.
    #[test]
    fn turn_finished_maps_each_status_to_its_ending() {
        let finished = |status: &str, stop_reason: Option<&str>, error: Option<&str>| {
            session_event_to_turn_events(&event(
                "turn_finished",
                json!({ "status": status, "stop_reason": stop_reason, "error": error }),
            ))
        };
        let done = |out: Vec<TurnEvent>| match out.as_slice() {
            [TurnEvent::Done { stop_reason }] => *stop_reason,
            other => panic!("expected single Done, got {other:?}"),
        };
        assert_eq!(
            done(finished("completed", Some("max_tokens"), None)),
            StopReason::MaxTokens
        );
        assert_eq!(done(finished("completed", None, None)), StopReason::EndTurn);
        assert_eq!(
            done(finished("cancelled", None, None)),
            StopReason::Cancelled
        );
        assert_eq!(
            done(finished(
                "handler_cancelled",
                None,
                Some("stopped by a hook")
            )),
            StopReason::Refusal
        );
        for status in ["failed", "timed_out"] {
            match finished(status, None, Some("the provider is down")).as_slice() {
                [TurnEvent::Error(TurnError::Communication(msg))] => {
                    assert_eq!(msg, "the provider is down", "status {status}");
                }
                other => panic!("status {status}: expected an error, got {other:?}"),
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
