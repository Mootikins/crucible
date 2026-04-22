//! Helpers bridging the legacy `ChatChunk` streaming format to
//! [`TurnEvent`]s. Used by the `Agent` impls on `GenaiAgentHandle` and
//! `AcpAgentHandle` until `ChatChunk` itself is retired.
//!
//! NOTE: scheduled for deletion together with `ChatChunk` — do not
//! grow the surface here.

use async_stream::stream;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatToolCall, ChatToolResult,
};
use crucible_core::turn::{StopReason, TurnContext, TurnError, TurnEvent};
use futures::stream::BoxStream;
use futures::StreamExt;

/// Depth-cap prompt sent back to the agent when `max_tool_depth` is
/// reached. Kept in sync with
/// `agent_manager::messaging::TOOL_DEPTH_LIMIT_FINAL_PROMPT`.
pub const DEPTH_CAP_PROMPT: &str = "You have reached the tool call limit. Please provide your final answer based on the information gathered so far.";

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

/// Drive a classic tool-loop `Agent::turn` stream by delegating to an
/// `AgentHandle`'s `send_message_stream` / `continue_with_tool_results`.
///
/// The logic — identical to what the retired `InternalAgent` did — is:
///
/// 1. Start with `send_message_stream(content)`.
/// 2. Translate each `ChatChunk` to `TurnEvent`s.
/// 3. On terminal chunk with tool calls: emit `ToolBatchEnd`, wait on
///    `ctx.inbound` for `ToolResult` events (matched by id).
/// 4. Optionally restart via `send_message_stream` on `HandlerInjection`
///    or `DepthCapHit`.
/// 5. Feed collected results back through `continue_with_tool_results`
///    and loop.
///
/// Used by `GenaiAgentHandle::Agent::turn` and the test fixture macro
/// [`impl_tool_loop_agent!`]. Will die with `ChatChunk` itself.
pub(crate) fn legacy_tool_loop_stream<'a, H: AgentHandle + ?Sized>(
    handle: &'a mut H,
    ctx: TurnContext,
) -> BoxStream<'a, TurnEvent> {
    let initial = ctx.content;
    let mut inbound = ctx.inbound;

    let body = stream! {
        let mut chat_stream = handle.send_message_stream(initial);

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

            yield TurnEvent::ToolBatchEnd;

            let Some(rx) = inbound.as_mut() else {
                yield TurnEvent::Done {
                    stop_reason: StopReason::EndTurn,
                };
                return;
            };

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
                    TurnEvent::ToolResult {
                        ref id,
                        ref name,
                        ref result,
                        ref error,
                    } => {
                        if !expected_ids.is_empty() && !expected_ids.contains(id) {
                            continue;
                        }
                        let result_str = match result {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        collected.push(ChatToolResult {
                            name: name.clone(),
                            result: result_str,
                            error: error.clone(),
                            call_id: Some(id.clone()),
                        });
                    }
                    TurnEvent::HandlerInjection { content, .. } => {
                        drop(chat_stream);
                        chat_stream = handle.send_message_stream(content);
                        continue 'turn;
                    }
                    TurnEvent::DepthCapHit { .. } => {
                        drop(chat_stream);
                        chat_stream = handle.send_message_stream(DEPTH_CAP_PROMPT.to_string());
                        continue 'turn;
                    }
                    _ => {}
                }
            }

            drop(chat_stream);
            chat_stream = handle.continue_with_tool_results(tool_calls, collected);
        }
    };

    Box::pin(body)
}

