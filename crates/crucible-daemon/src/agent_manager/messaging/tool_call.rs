use super::super::*;
use crucible_core::events::InternalSessionEvent;
use crucible_core::types::ToolSource;
use crucible_lua::{
    execute_tool_before_execute_hooks, execute_tool_display_complete_hooks,
    execute_tool_display_start_hooks, ToolBeforeExecuteEvent, ToolDisplayCompleteEvent,
    ToolDisplayStartEvent,
};

impl AgentManager {
    pub(super) async fn handle_tool_call_in_stream(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
    ) -> Option<crucible_core::traits::chat::ChatToolResult> {
        let call_id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let args = tool_call
            .arguments
            .clone()
            .unwrap_or(serde_json::Value::Null);

        {
            let mut state = stream_ctx.session_state.lock().await;
            let pre_tool_event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
                name: tool_call.name.clone(),
                args: args.clone(),
            });
            match state.reactor.emit(pre_tool_event).await {
                Ok(EmitResult::Cancelled { by_handler, .. }) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        handler = %by_handler,
                        "PreToolCall cancelled by handler"
                    );
                    let error_msg = format!("Tool call denied by handler: {}", by_handler);
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            &call_id,
                            &tool_call.name,
                            serde_json::json!({ "error": error_msg }),
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            "No subscribers for handler denied tool_result event"
                        );
                    }
                    return Some(crucible_core::traits::chat::ChatToolResult {
                        name: tool_call.name.clone(),
                        result: String::new(),
                        error: Some(error_msg),
                        call_id: Some(call_id.clone()),
                    });
                }
                Ok(EmitResult::Failed { handler, error, .. }) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        handler = %handler,
                        error = %error,
                        "PreToolCall handler failed, continuing (fail-open)"
                    );
                }
                Ok(EmitResult::Completed { .. }) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %error,
                        "PreToolCall emit failed, continuing (fail-open)"
                    );
                }
            }

            for handler in state
                .registry
                .runtime_handlers_for("pre_tool_call", Some(&tool_call.name))
            {
                let event = SessionEvent::Custom {
                    name: "pre_tool_call".to_string(),
                    payload: serde_json::json!({
                        "tool": &tool_call.name,
                        "args": &args,
                    }),
                };
                match state
                    .registry
                    .execute_runtime_handler(&state.lua, &handler.name, &event)
                    .await
                {
                    Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            handler = %handler.name,
                            reason = %reason,
                            "pre_tool_call handler cancelled"
                        );
                        let error_msg = format!("Tool blocked by crucible.on handler: {}", reason);
                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_result(
                                &stream_ctx.session_id,
                                &call_id,
                                &tool_call.name,
                                serde_json::json!({ "error": error_msg }),
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %tool_call.name,
                                "No subscribers for handler denied tool_result event"
                            );
                        }
                        return Some(crucible_core::traits::chat::ChatToolResult {
                            name: tool_call.name.clone(),
                            result: String::new(),
                            error: Some(error_msg),
                            call_id: Some(call_id.clone()),
                        });
                    }
                    Ok(crucible_lua::ScriptHandlerResult::Handled { result }) => {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            handler = %handler.name,
                            "pre_tool_call handler provided result"
                        );
                        // Emit tool_call event so TUI shows tool running
                        emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_call(
                                &stream_ctx.session_id,
                                &call_id,
                                &tool_call.name,
                                args.clone(),
                            ),
                        );
                        let result_string = match result {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        };
                        // Emit tool_result event so TUI shows completion
                        emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_result(
                                &stream_ctx.session_id,
                                &call_id,
                                &tool_call.name,
                                serde_json::json!({ "result": &result_string }),
                            ),
                        );
                        return Some(crucible_core::traits::chat::ChatToolResult {
                            name: tool_call.name.clone(),
                            result: result_string,
                            error: None,
                            call_id: Some(call_id.clone()),
                        });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            handler = %handler.name,
                            error = %error,
                            "pre_tool_call handler error (fail-open)"
                        );
                    }
                }
            }
        }

        if !is_safe(&tool_call.name)
            && !Self::handle_permission_request(stream_ctx, tool_call, &call_id, &args).await
        {
            return Some(crucible_core::traits::chat::ChatToolResult {
                name: tool_call.name.clone(),
                result: String::new(),
                error: Some(format!(
                    "Tool call denied by permission gate: {}",
                    tool_call.name
                )),
                call_id: Some(call_id.clone()),
            });
        }

        let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());
        let (mut description, mut source) = stream_ctx
            .tool_dispatcher
            .get_tool_ref(&tool_call.name)
            .and_then(|tool_ref| match &tool_ref.source {
                ToolSource::Core | ToolSource::Crucible => Some((
                    tool_ref.definition.description.map(|d| d.to_string()),
                    Some(Self::format_tool_source(&tool_ref.source)),
                )),
                ToolSource::Mcp { .. } | ToolSource::Plugin { .. } => None,
            })
            .unwrap_or((None, None));

        let mut lua_primary_arg: Option<String> = None;

        {
            let state = stream_ctx.session_state.lock().await;
            let hook_event = ToolDisplayStartEvent {
                name: tool_call.name.clone(),
                args: args_str.clone(),
            };
            match execute_tool_display_start_hooks(&state.lua, &state.registry, &hook_event).await {
                Ok(Some(hints)) => {
                    if let Some(label) = hints.label {
                        description = Some(label);
                    }
                    if let Some(detail) = hints.detail {
                        source = Some(detail);
                    }
                    if let Some(pa) = hints.primary_arg {
                        lua_primary_arg = Some(pa);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %error,
                        "Lua tool:display_start hook error, falling back to default metadata"
                    );
                }
            }
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::tool_call_with_metadata(
                &stream_ctx.session_id,
                &call_id,
                &tool_call.name,
                args.clone(),
                description,
                source,
                lua_primary_arg,
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "No subscribers for tool_call event"
            );
        }

        // Fire tool:before_execute hook for env var injection
        let hook_env_vars = {
            let state = stream_ctx.session_state.lock().await;
            let hook_event = ToolBeforeExecuteEvent {
                name: tool_call.name.clone(),
                args: args.clone(),
            };
            match execute_tool_before_execute_hooks(&state.lua, &state.registry, &hook_event).await
            {
                Ok(Some(result)) => result.env,
                Ok(None) => std::collections::HashMap::new(),
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %error,
                        "Lua tool:before_execute hook error, proceeding without env vars"
                    );
                    std::collections::HashMap::new()
                }
            }
        };

        // ACP agents execute their own tools internally; Crucible only sees the
        // tool_call / tool_result as notifications. If the tool isn't in our
        // dispatcher, skip dispatch — the tool_call event was already emitted
        // above (for TUI display), and the ACP ToolEnd chunk will emit the
        // matching tool_result separately. Dispatching would produce a bogus
        // "Unknown tool" error that the TUI would render as a failed call.
        if !stream_ctx.tool_dispatcher.has_tool(&tool_call.name) {
            debug!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "Tool not in local dispatcher; leaving result to external agent"
            );
            return None;
        }

        let tool_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            stream_ctx
                .tool_dispatcher
                .dispatch_tool(&tool_call.name, args.clone(), hook_env_vars),
        )
        .await;
        let (mut result_str, error_str) = match tool_result {
            Ok(Ok(val)) => (val.to_string(), None),
            Ok(Err(e)) => (String::new(), Some(e)),
            Err(_elapsed) => (
                String::new(),
                Some(
                    anyhow::anyhow!("Tool '{}' timed out after 30 seconds", tool_call.name)
                        .to_string(),
                ),
            ),
        };

        // Spill large tool outputs to disk and replace with a token-efficient reference.
        // Skip tools whose output is trivially reproducible from existing data on disk.
        const SPILL_THRESHOLD: usize = 10 * 1024; // 10KB
        let should_spill = error_str.is_none()
            && result_str.len() >= SPILL_THRESHOLD
            && !is_reproducible_tool(&tool_call.name);
        let spill_path = if should_spill {
            let counter = {
                let state = stream_ctx.session_state.lock().await;
                state
                    .spill_counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            };
            match Self::spill_tool_output(
                &stream_ctx.session_dir,
                &tool_call.name,
                &result_str,
                counter,
            )
            .await
            {
                Ok((path, filename)) => {
                    // Count lines in the actual content, not the JSON-serialized string
                    let line_count = serde_json::from_str::<serde_json::Value>(&result_str)
                        .ok()
                        .and_then(|v| {
                            v.as_str().map(|s| s.lines().count()).or_else(|| {
                                v.get("result")
                                    .and_then(|r| r.as_str())
                                    .map(|s| s.lines().count())
                            })
                        })
                        .unwrap_or_else(|| result_str.lines().count());
                    let byte_kb = result_str.len() / 1024;
                    result_str = format!(
                        "[{line_count} lines, {byte_kb}KB — full output in $CRU_SESSION_DIR/tools/{filename}]"
                    );
                    Some(path)
                }
                Err(e) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %e,
                        "Failed to spill tool output, sending full result"
                    );
                    None
                }
            }
        } else {
            None
        };

        let mut event_result = if let Some(error) = &error_str {
            serde_json::json!({ "error": error })
        } else {
            serde_json::json!({ "result": result_str })
        };

        if let Some(ref path) = spill_path {
            event_result["spill_path"] = serde_json::json!(path);
        }

        {
            let state = stream_ctx.session_state.lock().await;
            let hook_event = ToolDisplayCompleteEvent {
                name: tool_call.name.clone(),
                args: args_str,
                result: error_str.clone().unwrap_or_else(|| result_str.clone()),
            };
            match execute_tool_display_complete_hooks(&state.lua, &state.registry, &hook_event)
                .await
            {
                Ok(Some(hints)) => {
                    if let Some(summary) = hints.summary {
                        event_result["summary"] = serde_json::json!(summary);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %error,
                        "Lua tool:display_complete hook error, falling back to default metadata"
                    );
                }
            }
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::tool_result(
                &stream_ctx.session_id,
                &call_id,
                &tool_call.name,
                event_result,
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "No subscribers for tool_result event"
            );
        }

        Some(crucible_core::traits::chat::ChatToolResult {
            name: tool_call.name.clone(),
            result: result_str,
            error: error_str,
            call_id: Some(call_id),
        })
    }

    /// Spill large tool output to disk. Returns (absolute_path, filename).
    async fn spill_tool_output(
        session_dir: &std::path::Path,
        tool_name: &str,
        output: &str,
        counter: u32,
    ) -> anyhow::Result<(PathBuf, String)> {
        let tools_dir = session_dir.join("tools");
        tokio::fs::create_dir_all(&tools_dir).await?;

        let name_slug: String = tool_name
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.txt", name_slug, counter);
        let path = tools_dir.join(&filename);

        tokio::fs::write(&path, output).await?;
        Ok((path, filename))
    }
}

/// Tools whose output is trivially reproducible from existing data on disk.
/// These should not be spilled — the content already exists and can be re-read.
fn is_reproducible_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "mcp_read"
            | "edit_file"
            | "mcp_edit"
            | "write_file"
            | "mcp_write"
            | "glob"
            | "mcp_glob"
            | "grep"
            | "mcp_grep"
            | "list_notes"
            | "read_note"
            | "read_metadata"
            | "get_kiln_info"
    )
}
