//! Event conversion helpers for `DaemonAgentHandle`.
//!
//! Converts daemon `SessionEvent`s into `ChatChunk`s for the streaming
//! interface, and routes interaction events onto a separate channel for the
//! TUI event loop.

use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::chat::{ChatChunk, ChatToolCall, ChatToolResult, PrecognitionNoteInfo};
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
///   otherwise on `streaming_tx` (oneshot / ChatChunk path)
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

/// Convert a `text_delta` event to a ChatChunk.
fn convert_text_delta(event: &SessionEvent) -> Option<ChatChunk> {
    let content = event.data.get("content")?.as_str()?;
    Some(ChatChunk {
        delta: content.to_string(),
        ..Default::default()
    })
}

/// Convert a `thinking` event to a ChatChunk.
fn convert_thinking(event: &SessionEvent) -> Option<ChatChunk> {
    let content = event.data.get("content")?.as_str()?;
    Some(ChatChunk {
        reasoning: Some(content.to_string()),
        ..Default::default()
    })
}

/// Convert a `tool_call` event to a ChatChunk.
fn convert_tool_call(event: &SessionEvent) -> Option<ChatChunk> {
    let call_id = event.data.get("call_id").and_then(|v| v.as_str());
    let tool = event.data.get("tool")?.as_str()?;
    let args = event.data.get("args").cloned();

    Some(ChatChunk {
        tool_calls: Some(vec![ChatToolCall {
            name: tool.to_string(),
            arguments: args,
            id: call_id.map(String::from),
        }]),
        ..Default::default()
    })
}

/// Convert a `tool_result` event to a ChatChunk.
fn convert_tool_result(event: &SessionEvent) -> Option<ChatChunk> {
    let tool_name = event.data.get("tool").and_then(|v| v.as_str());
    let call_id = event.data.get("call_id").and_then(|v| v.as_str());
    let result = event.data.get("result")?;

    let call_id_str = call_id.map(String::from);
    let name = tool_name
        .map(String::from)
        .or_else(|| call_id_str.clone())
        .unwrap_or_else(|| "tool".to_string());

    let error = result
        .get("error")
        .and_then(|e| e.as_str())
        .map(String::from);

    let result_str = if error.is_some() {
        String::new()
    } else if result.is_string() {
        result.as_str().unwrap_or("").to_string()
    } else if let Some(inner) = result.get("result").and_then(|r| r.as_str()) {
        inner.to_string()
    } else {
        result.to_string()
    };

    Some(ChatChunk {
        tool_results: Some(vec![ChatToolResult {
            name,
            result: result_str,
            error,
            call_id: call_id_str,
        }]),
        ..Default::default()
    })
}

/// Convert a `message_complete` event to a ChatChunk with optional token usage.
fn convert_message_complete(event: &SessionEvent) -> Option<ChatChunk> {
    let usage = event
        .data
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .map(|total| {
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
            crucible_core::traits::llm::TokenUsage {
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
            }
        });
    Some(ChatChunk {
        done: true,
        usage,
        ..Default::default()
    })
}

/// Convert an `ended` event to a terminal ChatChunk.
fn convert_ended() -> ChatChunk {
    ChatChunk {
        delta: String::new(),
        done: true,
        tool_calls: None,
        tool_results: None,
        reasoning: None,
        usage: None,
        precognition_notes_count: None,
        precognition_notes: None,
    }
}

/// Convert a `precognition_complete` event to a ChatChunk.
fn convert_precognition_complete(event: &SessionEvent) -> Option<ChatChunk> {
    let notes_count = event
        .data
        .get("notes_count")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let precognition_notes = event
        .data
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<PrecognitionNoteInfo>>(v.clone()).ok());
    Some(ChatChunk {
        precognition_notes_count: notes_count,
        precognition_notes,
        ..Default::default()
    })
}

/// Convert a SessionEvent to a ChatChunk
///
/// Thin dispatcher that delegates to per-event-family helpers:
/// - `text_delta` / `thinking` → streaming content
/// - `tool_call` / `tool_result` → tool events
/// - `message_complete` / `ended` → completion signals
/// - `precognition_complete` → knowledge graph events
pub(super) fn session_event_to_chat_chunk(event: &SessionEvent) -> Option<ChatChunk> {
    match event.event_type.as_str() {
        "text_delta" => convert_text_delta(event),
        "thinking" => convert_thinking(event),
        "tool_call" => convert_tool_call(event),
        "tool_result" => convert_tool_result(event),
        "message_complete" => convert_message_complete(event),
        "ended" => Some(convert_ended()),
        "precognition_complete" => convert_precognition_complete(event),
        _ => {
            tracing::debug!("Unknown session event type: {}", event.event_type);
            None
        }
    }
}
