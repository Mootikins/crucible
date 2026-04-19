use super::super::*;
use crate::agent_manager::tool_tracking::ToolCallTracker;
use crucible_core::events::InternalSessionEvent;
use std::collections::HashSet;

use super::TOOL_DEPTH_LIMIT_FINAL_PROMPT;

impl AgentManager {
    pub(super) async fn emit_stream_events(
        stream_ctx: &StreamContext,
        chunk: &crucible_core::traits::chat::ChatChunk,
        accumulated_response: &mut String,
    ) {
        // Emit thinking before text_delta — thinking logically precedes the response
        // it produced. Matches the TUI direct-stream path which already has this order.
        if let Some(reasoning) = &chunk.reasoning {
            debug!(session_id = %stream_ctx.session_id, "Sending thinking event");
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::thinking(&stream_ctx.session_id, reasoning),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for thinking event"
                );
            }
        }

        if !chunk.delta.is_empty() {
            if !accumulated_response.is_empty() && chunk.delta == *accumulated_response {
                debug!(
                    session_id = %stream_ctx.session_id,
                    delta_len = chunk.delta.len(),
                    "Skipping duplicate full-text delta (matches accumulated response)"
                );
            } else {
                accumulated_response.push_str(&chunk.delta);
                debug!(
                    session_id = %stream_ctx.session_id,
                    delta_len = chunk.delta.len(),
                    "Sending text_delta event"
                );
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::text_delta(&stream_ctx.session_id, &chunk.delta),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for text_delta event"
                    );
                }
            }
        }

        if !chunk.done {
            if let Some(tool_calls) = &chunk.tool_calls {
                for tool_call in tool_calls {
                    let _ = Self::handle_tool_call_in_stream(stream_ctx, tool_call).await;
                }
            }
        }

        if let Some(tool_results) = &chunk.tool_results {
            for tool_result in tool_results {
                let call_id = uuid::Uuid::new_v4().to_string();
                let result = if let Some(err) = &tool_result.error {
                    serde_json::json!({ "error": err })
                } else {
                    serde_json::json!({ "result": tool_result.result })
                };

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        &call_id,
                        &tool_result.name,
                        result,
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_result.name,
                        "No subscribers for tool_result event"
                    );
                }
            }
        }
    }

    #[allow(clippy::ptr_arg)]
    pub(super) async fn run_reactor_handlers(
        stream_ctx: &StreamContext,
        chunk: &crucible_core::traits::chat::ChatChunk,
        accumulated_response: &mut String,
        is_continuation: bool,
    ) -> Option<(String, String)> {
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
                chunk.usage.as_ref(),
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

    pub(super) async fn emit_max_tool_depth_hard_stop(
        stream_ctx: &StreamContext,
        max_tool_depth: usize,
    ) {
        warn!(
            session_id = %stream_ctx.session_id,
            max_tool_depth = max_tool_depth,
            "max_tool_depth exceeded"
        );
        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::ended(&stream_ctx.session_id, "error: max_tool_depth exceeded"),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for max_tool_depth ended event"
            );
        }
    }

    pub(super) async fn stream_forced_final_response_at_depth_limit(
        agent_guard: &mut tokio::sync::MutexGuard<'_, BoxedAgentHandle>,
        stream_ctx: &StreamContext,
        accumulated_response: &mut String,
    ) -> Option<crucible_core::traits::chat::ChatChunk> {
        let response_len_before = accumulated_response.len();
        let mut forced_terminal_chunk: Option<crucible_core::traits::chat::ChatChunk> = None;
        let mut forced_stream =
            agent_guard.send_message_stream(TOOL_DEPTH_LIMIT_FINAL_PROMPT.to_string());

        while let Some(forced_result) = forced_stream.next().await {
            match forced_result {
                Ok(chunk) => {
                    let mut forced_chunk = chunk.clone();
                    forced_chunk.tool_calls = None;
                    forced_chunk.tool_results = None;
                    Self::emit_stream_events(stream_ctx, &forced_chunk, accumulated_response).await;

                    if forced_chunk.done {
                        forced_terminal_chunk = Some(forced_chunk);
                        break;
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %stream_ctx.session_id,
                        error = %e,
                        "Forced final response stream failed at tool depth limit"
                    );
                    return None;
                }
            }
        }

        if accumulated_response.len() == response_len_before {
            warn!(
                session_id = %stream_ctx.session_id,
                "Forced final response produced no text at tool depth limit"
            );
            return None;
        }

        if forced_terminal_chunk.is_none() {
            warn!(
                session_id = %stream_ctx.session_id,
                "Forced final response stream ended without terminal chunk"
            );
        }

        forced_terminal_chunk
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
        let Some(content) =
            Self::apply_pre_llm_call_handlers(content, &stream_ctx, &stream_config).await
        else {
            return;
        };

        let stream_start = Instant::now();
        let mut agent_guard = agent.lock().await;
        let mut stream = agent_guard.send_message_stream(content);
        let mut tracker = ToolCallTracker::new();
        let mut blocked_tools: HashSet<String> = HashSet::new();
        let mut last_failure_key: Option<(String, String)> = None;
        let mut consecutive_failure_count = 0usize;
        let mut tool_calls_dispatched = false;
        let mut terminal_chunk: Option<crucible_core::traits::chat::ChatChunk> = None;

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    if chunk
                        .tool_calls
                        .as_ref()
                        .is_some_and(|calls| !calls.is_empty())
                    {
                        tool_calls_dispatched = true;
                    }

                    Self::emit_stream_events(&stream_ctx, &chunk, accumulated_response).await;

                    if chunk.done {
                        if let Some(tool_calls) = chunk.tool_calls.clone() {
                            if tool_depth < max_tool_depth {
                                tool_depth += 1;
                                let mut tool_results = Vec::new();
                                for tool_call in &tool_calls {
                                    let args = tool_call
                                        .arguments
                                        .clone()
                                        .unwrap_or(serde_json::Value::Null);

                                    let mut attempt: Option<usize> = None;
                                    let mut result = if blocked_tools.contains(&tool_call.name) {
                                        let call_id = tool_call
                                            .id
                                            .clone()
                                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                                        let blocked_error = format!(
                                            "Tool '{}' is blocked for this stream after repeated failures.",
                                            tool_call.name
                                        );

                                        if !emit_event(
                                            &stream_ctx.event_tx,
                                            SessionEventMessage::tool_result(
                                                &stream_ctx.session_id,
                                                &call_id,
                                                &tool_call.name,
                                                serde_json::json!({ "error": blocked_error }),
                                            ),
                                        ) {
                                            warn!(
                                                session_id = %stream_ctx.session_id,
                                                tool = %tool_call.name,
                                                "No subscribers for blocked tool_result event"
                                            );
                                        }

                                        Some(crucible_core::traits::chat::ChatToolResult {
                                            name: tool_call.name.clone(),
                                            result: String::new(),
                                            error: Some(blocked_error),
                                            call_id: Some(call_id),
                                        })
                                    } else {
                                        attempt = Some(tracker.record_call(&tool_call.name, &args));
                                        Self::handle_tool_call_in_stream(&stream_ctx, tool_call)
                                            .await
                                    };

                                    let args_key = serde_json::to_string(&args)
                                        .unwrap_or_else(|_| "null".to_string());

                                    if let Some(tool_result) = result.as_mut() {
                                        if let Some(error) = tool_result.error.as_mut() {
                                            let failure_key =
                                                (tool_call.name.clone(), args_key.clone());
                                            if last_failure_key.as_ref() == Some(&failure_key) {
                                                consecutive_failure_count += 1;
                                            } else {
                                                consecutive_failure_count = 1;
                                                last_failure_key = Some(failure_key.clone());
                                            }

                                            if attempt.is_some_and(|attempt| attempt >= 3)
                                                && tracker.is_repeat_failure(
                                                    &tool_call.name,
                                                    &args,
                                                    3,
                                                )
                                            {
                                                let attempt = attempt.unwrap_or_default();
                                                let annotation = format!(
                                                    "Attempt {}. This tool has failed {} times with identical arguments. Try a different approach.",
                                                    attempt, attempt
                                                );
                                                if !error.contains(&annotation) {
                                                    if !error.is_empty() {
                                                        error.push(' ');
                                                    }
                                                    error.push_str(&annotation);
                                                }
                                            }

                                            if consecutive_failure_count >= 3 {
                                                blocked_tools.insert(tool_call.name.clone());
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

                                    if let Some(result) = result {
                                        tool_results.push(result);
                                    }
                                }

                                drop(stream);
                                drop(agent_guard);
                                agent_guard = agent.lock().await;
                                stream = agent_guard
                                    .continue_with_tool_results(tool_calls, tool_results);
                                continue;
                            }

                            warn!(
                                session_id = %stream_ctx.session_id,
                                max_tool_depth = max_tool_depth,
                                "max_tool_depth reached, forcing final response without tools"
                            );
                            if let Some(forced_terminal) =
                                Self::stream_forced_final_response_at_depth_limit(
                                    &mut agent_guard,
                                    &stream_ctx,
                                    accumulated_response,
                                )
                                .await
                            {
                                terminal_chunk = Some(forced_terminal);
                            } else {
                                Self::emit_max_tool_depth_hard_stop(&stream_ctx, max_tool_depth)
                                    .await;
                            }
                            break;
                        }

                        terminal_chunk = Some(chunk);
                        break;
                    }
                }
                Err(e) => {
                    // When continue_with_tool_results is not supported (e.g. ACP agents)
                    // and we already have accumulated response text, treat it as a graceful
                    // completion rather than an error.
                    if matches!(&e, crucible_core::traits::chat::ChatError::NotSupported(_))
                        && !accumulated_response.trim().is_empty()
                    {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            error = %e,
                            "Tool continuation not supported, completing with accumulated response"
                        );
                        // Create a synthetic terminal chunk so message_complete is emitted
                        terminal_chunk = Some(crucible_core::traits::chat::ChatChunk {
                            delta: String::new(),
                            done: true,
                            reasoning: None,
                            usage: None,
                            tool_calls: None,
                            tool_results: None,
                            precognition_notes: None,
                            precognition_notes_count: None,
                            subagent_events: None,
                        });
                        break;
                    }
                    error!(session_id = %stream_ctx.session_id, error = %e, "Agent stream error");
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {}", e)),
                    ) {
                        warn!(session_id = %stream_ctx.session_id, "No subscribers for error event");
                    }
                    break;
                }
            }
        }

        if accumulated_response.trim().is_empty() && !tool_calls_dispatched {
            error!(
                session_id = %stream_ctx.session_id,
                "LLM stream completed with no content and no tool calls"
            );
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::ended(
                    &stream_ctx.session_id,
                    format!(
                        "error: {}",
                        crate::provider::genai_handle::EMPTY_RESPONSE_ERROR
                    ),
                ),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for empty-response ended event"
                );
            }
            return;
        }

        if terminal_chunk.is_none() {
            warn!(
                session_id = %stream_ctx.session_id,
                reason = crate::provider::genai_handle::STREAM_UNEXPECTED_END_ERROR,
                "Stream ended without terminal done chunk"
            );
        }

        if let Some(chunk) = terminal_chunk {
            let injection = Self::run_reactor_handlers(
                &stream_ctx,
                &chunk,
                accumulated_response,
                is_continuation,
            )
            .await;

            if let Some((injected_content, _)) = injection {
                drop(agent_guard);

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
