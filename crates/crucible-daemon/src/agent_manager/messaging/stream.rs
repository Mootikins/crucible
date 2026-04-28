//! Agent turn driver.
//!
//! Drives an `Agent::turn()` stream, emits session events, dispatches
//! tool calls, and steers the agent's continuation via the inbound
//! `mpsc<TurnEvent>` channel. The plan's "one channel topology, not
//! three" rule: `ToolResult`, `HandlerInjection`, and `DepthCapHit`
//! all arrive on the same inbound channel and drive matching
//! adapter-side behaviour.
//!
//! Three tool-loop re-entry points all share the inbound channel:
//!
//! 1. **Tool continuation** — runtime sends `ToolResult` after
//!    dispatching the agent's `ToolCall`.
//! 2. **Depth-cap exhaustion** — runtime sends `DepthCapHit`; adapter
//!    restarts the inner stream with the depth-cap final-answer prompt.
//! 3. **Handler injection** — runtime's `turn:complete` handler returns
//!    injected content; runtime re-enters `execute_agent_stream`
//!    recursively with `is_continuation = true`. (The inbound channel
//!    handles this within a single adapter turn, but handler
//!    injection happens after `Done`, so we re-enter for a fresh
//!    message_id + user_message visibility.)

use super::super::*;
use crate::agent_manager::tool_tracking::ToolCallTracker;
use crucible_core::events::InternalSessionEvent;
use crucible_core::traits::chat::{ChatToolCall, ChatToolResult};
use crucible_core::traits::llm::TokenUsage;
use crucible_core::turn::{Agent as TurnAgent, StopReason, TurnContext, TurnEvent};
use futures::StreamExt;
use std::collections::HashSet;
use tokio::sync::mpsc;

impl AgentManager {
    #[allow(clippy::ptr_arg)]
    pub(super) async fn run_reactor_handlers(
        stream_ctx: &StreamContext,
        usage: Option<&TokenUsage>,
        accumulated_response: &mut String,
        is_continuation: bool,
    ) -> Option<(String, String)> {
        // Scheduler-owned conversation tree: commit the assistant
        // response text as an Agent node. Today this is shadow state;
        // later phases flip the handle to read from the tree.
        if !accumulated_response.is_empty() {
            let mut tree = stream_ctx.conversation_tree.lock().await;
            let parent = tree.current();
            let _agent = tree.add_child_and_advance(
                parent,
                crucible_core::turn::NodeContent::Agent {
                    text: accumulated_response.clone(),
                },
            );
        }

        debug!(
            session_id = %stream_ctx.session_id,
            message_id = %stream_ctx.message_id,
            response_len = accumulated_response.len(),
            "Sending message_complete event"
        );
        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::message_complete(
                &stream_ctx.session_id,
                &stream_ctx.message_id,
                accumulated_response.clone(),
                usage,
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for message_complete event"
            );
        }

        let injection = Self::dispatch_turn_complete_handlers(
            &stream_ctx.session_id,
            &stream_ctx.message_id,
            accumulated_response,
            &stream_ctx.session_state,
            is_continuation,
        )
        .await;

        if let Some((injected_content, position)) = &injection {
            info!(
                session_id = %stream_ctx.session_id,
                content_len = injected_content.len(),
                position = %position,
                "Processing handler injection"
            );

            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::new(
                    &stream_ctx.session_id,
                    "injection_pending",
                    serde_json::json!({
                        "content": injected_content,
                        "position": position,
                        "is_continuation": true,
                    }),
                ),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for injection_pending event"
                );
            }
        }

        injection
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        stream_ctx: StreamContext,
        stream_config: AgentStreamConfig,
        accumulated_response: &mut String,
        is_continuation: bool,
        mut tool_depth: usize,
        max_tool_depth: usize,
    ) {
        let ttft_local = Instant::now();
        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "execute_stream_entry", elapsed_ms = 0, "ttft");
        let Some(content) =
            Self::apply_pre_llm_call_handlers(content, &stream_ctx, &stream_config).await
        else {
            return;
        };
        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "pre_llm_done", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");

        let stream_start = Instant::now();

        // Flatten the scheduler-owned tree path to messages so the
        // agent sees the full conversation and doesn't need to hold
        // its own history.
        let flattened_messages = {
            let tree = stream_ctx.conversation_tree.lock().await;
            tree.flatten_current_path_to_context()
        };

        let (inbound_tx, inbound_rx) = mpsc::channel::<TurnEvent>(32);
        let mut turn_ctx = TurnContext::new(content)
            .with_inbound(inbound_rx)
            .with_messages(flattened_messages);
        if is_continuation {
            turn_ctx = turn_ctx.continuation();
        }

        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "before_turn_start", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");
        // Hold the handle guard for the entire turn; Agent::turn returns
        // a stream that borrows `&mut *guard`.
        let mut guard = agent.lock().await;
        let mut event_stream = match guard.turn(turn_ctx).await {
            Ok(s) => s,
            Err(e) => {
                error!(
                    session_id = %stream_ctx.session_id,
                    error = %e,
                    "Agent failed to start turn"
                );
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {e}")),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for turn-start error event"
                    );
                }
                return;
            }
        };

        // Per-batch tool tracking
        let mut tracker = ToolCallTracker::new();
        let mut blocked_tools: HashSet<String> = HashSet::new();
        let mut last_failure_key: Option<(String, String)> = None;
        let mut consecutive_failure_count = 0usize;
        let mut tool_calls_dispatched = false;

        // Batch detection: true once a ToolCall is seen in this batch,
        // reset on the next non-ToolCall event from the agent.
        let mut in_tool_batch = false;
        // When true, the current batch exceeded max_tool_depth — skip
        // dispatching its remaining ToolCalls and let the adapter restart
        // on the depth-cap prompt.
        let mut capped_this_batch = false;
        // Set once the runtime sent DepthCapHit, so the empty-response
        // branch below can surface the right error reason.
        let mut depth_cap_triggered = false;

        // Terminal state
        let mut last_usage: Option<TokenUsage> = None;
        let mut terminal_stop_reason: Option<StopReason> = None;

        let mut ttft_first_token_logged = false;
        let mut ttft_first_event_logged = false;
        while let Some(event) = event_stream.next().await {
            if !ttft_first_event_logged {
                info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "first_turn_event", elapsed_ms = ttft_local.elapsed().as_millis() as u64, kind = ?std::mem::discriminant(&event), "ttft");
                ttft_first_event_logged = true;
            }
            match event {
                TurnEvent::TextDelta(delta) => {
                    if delta.is_empty() {
                        continue;
                    }

                    // Dedup: some providers send the whole accumulated
                    // text as their final delta. Skip if it matches.
                    if !accumulated_response.is_empty() && delta == *accumulated_response {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            delta_len = delta.len(),
                            "Skipping duplicate full-text delta (matches accumulated response)"
                        );
                        continue;
                    }

                    if !ttft_first_token_logged {
                        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "first_text_delta", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");
                        ttft_first_token_logged = true;
                    }
                    accumulated_response.push_str(&delta);
                    debug!(
                        session_id = %stream_ctx.session_id,
                        delta_len = delta.len(),
                        "Sending text_delta event"
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::text_delta(&stream_ctx.session_id, &delta),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for text_delta event"
                        );
                    }
                }
                TurnEvent::Thinking(reasoning) => {
                    debug!(session_id = %stream_ctx.session_id, "Sending thinking event");
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::thinking(&stream_ctx.session_id, &reasoning),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for thinking event"
                        );
                    }
                }
                TurnEvent::ToolCall { id, name, args } => {
                    // New batch? increment depth, possibly cap.
                    if !in_tool_batch {
                        // If the depth cap already fired once and the
                        // model is *still* calling tools, hard-stop.
                        // Otherwise we'd ping-pong DepthCapHit → restart
                        // → ToolCall → DepthCapHit forever.
                        if depth_cap_triggered {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                "tool call emitted after depth-cap response; hard-stopping"
                            );
                            if !emit_event(
                                &stream_ctx.event_tx,
                                SessionEventMessage::ended(
                                    &stream_ctx.session_id,
                                    "error: max_tool_depth exceeded".to_string(),
                                ),
                            ) {
                                warn!(
                                    session_id = %stream_ctx.session_id,
                                    "No subscribers for hard-stop ended event"
                                );
                            }
                            return;
                        }

                        in_tool_batch = true;
                        if tool_depth >= max_tool_depth {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                max_tool_depth = max_tool_depth,
                                "max_tool_depth reached, forcing final response without tools"
                            );
                            if inbound_tx
                                .send(TurnEvent::DepthCapHit {
                                    max_depth: max_tool_depth,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                            capped_this_batch = true;
                            depth_cap_triggered = true;
                            continue;
                        }
                        tool_depth += 1;
                        capped_this_batch = false;
                    }

                    if capped_this_batch {
                        // Remaining ToolCalls in a capped batch are
                        // dropped; the adapter has already been told to
                        // restart and will discard them.
                        continue;
                    }

                    tool_calls_dispatched = true;

                    // Commit to scheduler-owned conversation tree
                    // (shadow state until handle.history retires).
                    {
                        let mut tree = stream_ctx.conversation_tree.lock().await;
                        let parent = tree.current();
                        tree.add_child(
                            parent,
                            crucible_core::turn::NodeContent::ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                args: args.clone(),
                            },
                        );
                    }

                    let tool_call = ChatToolCall {
                        name: name.clone(),
                        arguments: Some(args.clone()),
                        id: Some(id.clone()),
                    };

                    // Dispatch (honoring blocked list + failure tracking).
                    let mut attempt: Option<usize> = None;
                    let mut result = if blocked_tools.contains(&name) {
                        let blocked_error = format!(
                            "Tool '{}' is blocked for this stream after repeated failures.",
                            name
                        );

                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_result(
                                &stream_ctx.session_id,
                                &id,
                                &name,
                                serde_json::json!({ "error": blocked_error }),
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %name,
                                "No subscribers for blocked tool_result event"
                            );
                        }

                        Some(ChatToolResult {
                            name: name.clone(),
                            result: String::new(),
                            error: Some(blocked_error),
                            call_id: Some(id.clone()),
                        })
                    } else {
                        attempt = Some(tracker.record_call(&name, &args));
                        Self::handle_tool_call_in_stream(&stream_ctx, &tool_call).await
                    };

                    // Repeat-failure tracking / annotation.
                    let args_key =
                        serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());

                    if let Some(tool_result) = result.as_mut() {
                        if let Some(error) = tool_result.error.as_mut() {
                            let failure_key = (name.clone(), args_key.clone());
                            if last_failure_key.as_ref() == Some(&failure_key) {
                                consecutive_failure_count += 1;
                            } else {
                                consecutive_failure_count = 1;
                                last_failure_key = Some(failure_key);
                            }

                            if attempt.is_some_and(|a| a >= 3)
                                && tracker.is_repeat_failure(&name, &args, 3)
                            {
                                let attempt_val = attempt.unwrap_or_default();
                                let annotation = format!(
                                    "Attempt {}. This tool has failed {} times with identical arguments. Try a different approach.",
                                    attempt_val, attempt_val
                                );
                                if !error.contains(&annotation) {
                                    if !error.is_empty() {
                                        error.push(' ');
                                    }
                                    error.push_str(&annotation);
                                }
                            }

                            if consecutive_failure_count >= 3 {
                                blocked_tools.insert(name.clone());
                            }
                        } else {
                            last_failure_key = None;
                            consecutive_failure_count = 0;

                            if tool_depth == max_tool_depth.saturating_sub(2) {
                                tool_result.result.push_str(&format!(
                                    " [Note: You have used {} of {} available tool turns.]",
                                    tool_depth, max_tool_depth
                                ));
                            }
                        }
                    }

                    let tool_result = result.unwrap_or_else(|| ChatToolResult {
                        name: name.clone(),
                        result: String::new(),
                        error: Some("tool dispatcher returned no result".to_string()),
                        call_id: Some(id.clone()),
                    });

                    // Commit ToolResult to scheduler-owned tree before
                    // feeding it back to the adapter.
                    {
                        let mut tree = stream_ctx.conversation_tree.lock().await;
                        let parent = tree.current();
                        let result_value = serde_json::Value::String(tool_result.result.clone());
                        tree.add_child(
                            parent,
                            crucible_core::turn::NodeContent::ToolResult {
                                id: tool_result.call_id.clone().unwrap_or_else(|| id.clone()),
                                name: tool_result.name.clone(),
                                result: result_value,
                                error: tool_result.error.clone(),
                            },
                        );
                    }

                    // Feed back to the adapter so it can continue the turn.
                    let reply = TurnEvent::ToolResult {
                        id: tool_result.call_id.clone().unwrap_or_else(|| id.clone()),
                        name: tool_result.name,
                        result: serde_json::Value::String(tool_result.result),
                        error: tool_result.error,
                    };
                    if inbound_tx.send(reply).await.is_err() {
                        // Adapter dropped; end turn.
                        break;
                    }
                }
                TurnEvent::ToolResult {
                    id,
                    name,
                    result,
                    error,
                } => {
                    // The agent observed an external tool result
                    // (ACP-style). Pass through to subscribers.
                    let event_result = if let Some(err) = error {
                        serde_json::json!({ "error": err })
                    } else {
                        serde_json::json!({ "result": result })
                    };
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            &id,
                            &name,
                            event_result,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %name,
                            "No subscribers for pass-through tool_result event"
                        );
                    }
                }
                TurnEvent::ToolBatchEnd => {
                    // Adapter has finished emitting all tool calls for
                    // this batch and is about to wait for ToolResults.
                    // Reset batch tracking so the next ToolCall counts
                    // as a new batch and re-checks the depth cap.
                    in_tool_batch = false;
                    capped_this_batch = false;
                }
                TurnEvent::Usage(usage) => {
                    last_usage = Some(usage);
                }
                TurnEvent::HandlerInjection { .. } | TurnEvent::DepthCapHit { .. } => {
                    // Inbound-only variants. Adapter should not echo
                    // them, but tolerate if it ever does.
                }
                TurnEvent::Done { stop_reason } => {
                    terminal_stop_reason = Some(stop_reason);
                    break;
                }
                TurnEvent::Error(e) => {
                    error!(
                        session_id = %stream_ctx.session_id,
                        error = %e,
                        "Agent turn error"
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {e}")),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for error event"
                        );
                    }
                    return;
                }
            }
        }

        // Close the inbound channel so the adapter wakes up if still
        // waiting (it should have terminated by now).
        drop(inbound_tx);

        // Empty response handling.
        if accumulated_response.trim().is_empty() && !tool_calls_dispatched {
            let error_reason = if depth_cap_triggered {
                "error: max_tool_depth exceeded".to_string()
            } else {
                format!(
                    "error: {}",
                    crate::provider::genai_handle::EMPTY_RESPONSE_ERROR
                )
            };
            error!(
                session_id = %stream_ctx.session_id,
                "LLM stream completed with no content and no tool calls"
            );
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::ended(&stream_ctx.session_id, error_reason),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for empty-response ended event"
                );
            }
            return;
        }

        if terminal_stop_reason.is_none() {
            warn!(
                session_id = %stream_ctx.session_id,
                reason = crate::provider::genai_handle::STREAM_UNEXPECTED_END_ERROR,
                "Stream ended without terminal done event"
            );
        }

        // Emit message_complete + run turn:complete handlers. Handler
        // injection short-circuits back into a fresh execute_agent_stream
        // with a new message_id so subscribers see a clean user message
        // boundary.
        let injection = Self::run_reactor_handlers(
            &stream_ctx,
            last_usage.as_ref(),
            accumulated_response,
            is_continuation,
        )
        .await;

        if let Some((injected_content, _)) = injection {
            drop(event_stream);
            // Release the handle lock before recursing so the inner
            // invocation can re-acquire it.
            drop(guard);

            accumulated_response.clear();
            let continuation_ctx = StreamContext {
                session_id: stream_ctx.session_id.clone(),
                message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                event_tx: stream_ctx.event_tx.clone(),
                session_state: stream_ctx.session_state.clone(),
                pending_permissions: stream_ctx.pending_permissions.clone(),
                workspace_path: stream_ctx.workspace_path.clone(),
                session_dir: stream_ctx.session_dir.clone(),
                agent_stream_config: stream_ctx.agent_stream_config.clone(),
                tool_dispatcher: stream_ctx.tool_dispatcher.clone(),
                permission_override: stream_ctx.permission_override,
                conversation_tree: stream_ctx.conversation_tree.clone(),
            };

            Box::pin(Self::execute_agent_stream(
                agent,
                injected_content,
                continuation_ctx,
                stream_config.clone(),
                accumulated_response,
                true,
                tool_depth,
                max_tool_depth,
            ))
            .await;
        }

        let duration_ms = stream_start.elapsed().as_millis() as u64;
        let response_summary: String = accumulated_response.chars().take(200).collect();

        if stream_config.model.is_empty() {
            warn!(
                session_id = %stream_ctx.session_id,
                "PostLlmCall model string is empty, possible upstream issue"
            );
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::new(
                &stream_ctx.session_id,
                "post_llm_call",
                serde_json::json!({
                    "response_summary": &response_summary,
                    "model": &stream_config.model,
                    "duration_ms": duration_ms,
                    "token_count": Option::<u64>::None,
                }),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for post_llm_call event"
            );
        }

        let mut state = stream_ctx.session_state.lock().await;
        let post_event = SessionEvent::internal(InternalSessionEvent::PostLlmCall {
            response_summary: response_summary.clone(),
            model: stream_config.model.clone(),
            duration_ms,
            token_count: None,
        });
        if let Err(e) = state.reactor.emit(post_event).await {
            warn!(
                session_id = %stream_ctx.session_id,
                error = %e,
                "PostLlmCall Reactor emit failed (fail-open)"
            );
        }

        for handler in state.registry.runtime_handlers_for("post_llm_call", None) {
            let event = SessionEvent::Custom {
                name: "post_llm_call".to_string(),
                payload: serde_json::json!({
                    "response_summary": &response_summary,
                    "model": &stream_config.model,
                    "duration_ms": duration_ms,
                }),
            };
            if let Err(error) = state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
                .await
            {
                warn!(
                    session_id = %stream_ctx.session_id,
                    error = %error,
                    "post_llm_call handler error (fail-open)"
                );
            }
        }
    }

    pub(in crate::agent_manager) async fn dispatch_turn_complete_handlers(
        session_id: &str,
        message_id: &str,
        response: &str,
        session_state: &Arc<Mutex<SessionEventState>>,
        is_continuation: bool,
    ) -> Option<(String, String)> {
        use crucible_lua::ScriptHandlerResult;

        let state = session_state.lock().await;
        let handlers = state.registry.runtime_handlers_for("turn:complete", None);

        if handlers.is_empty() {
            return None;
        }

        debug!(
            session_id = %session_id,
            handler_count = handlers.len(),
            is_continuation = is_continuation,
            "Dispatching turn:complete handlers"
        );

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "response_length": response.len(),
                "is_continuation": is_continuation,
            }),
        };

        let mut pending_injection: Option<(String, String)> = None;

        for handler in handlers {
            match state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
                .await
            {
                Ok(result) => {
                    debug!(
                        session_id = %session_id,
                        handler = %handler.name,
                        result = ?result,
                        "Handler executed"
                    );

                    if let ScriptHandlerResult::Inject { content, position } = result {
                        debug!(
                            session_id = %session_id,
                            handler = %handler.name,
                            content_len = content.len(),
                            position = %position,
                            "Handler returned inject"
                        );
                        pending_injection = Some((content, position));
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %session_id,
                        handler = %handler.name,
                        error = %e,
                        "Handler failed"
                    );
                }
            }
        }

        pending_injection
    }
}
