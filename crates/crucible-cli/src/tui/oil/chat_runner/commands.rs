use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay, OilChatApp};
use crucible_core::error_utils::strip_tool_error_prefix;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::traits::chat::AgentHandle;
use crucible_lua::SessionCommand;

use super::OilChatRunner;

impl OilChatRunner {
    pub(super) async fn handle_session_command<A: AgentHandle>(
        cmd: SessionCommand,
        agent: &mut A,
        app: &mut OilChatApp,
    ) {
        match cmd {
            SessionCommand::GetTemperature(reply) => {
                let _ = reply.send(agent.get_temperature());
            }
            SessionCommand::SetTemperature(temp, reply) => {
                let result = agent.set_temperature(temp).await.map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::GetMaxTokens(reply) => {
                let _ = reply.send(agent.get_max_tokens());
            }
            SessionCommand::SetMaxTokens(tokens, reply) => {
                let result = agent
                    .set_max_tokens(tokens)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::GetMaxIterations(reply) => {
                let _ = reply.send(agent.get_max_iterations());
            }
            SessionCommand::SetMaxIterations(iterations, reply) => {
                let result = agent
                    .set_max_iterations(iterations)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::GetExecutionTimeout(reply) => {
                let _ = reply.send(agent.get_execution_timeout());
            }
            SessionCommand::SetExecutionTimeout(timeout, reply) => {
                let result = agent
                    .set_execution_timeout(timeout)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::GetThinkingBudget(reply) => {
                let _ = reply.send(agent.get_thinking_budget());
            }
            SessionCommand::SetThinkingBudget(budget, reply) => {
                let result = agent
                    .set_thinking_budget(budget)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::GetModel(reply) => {
                let _ = reply.send(agent.current_model().map(|s| s.to_string()));
            }
            SessionCommand::SwitchModel(model, reply) => {
                let result = AgentHandle::switch_model(agent, &model)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::ListModels(reply) => {
                let _ = reply.send(agent.fetch_available_models().await);
            }
            SessionCommand::GetMode(reply) => {
                let _ = reply.send(agent.get_mode_id().to_string());
            }
            SessionCommand::SetMode(mode, reply) => {
                let result = agent.set_mode_str(&mode).await.map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            // Notification commands - route to OilChatApp
            SessionCommand::Notify(notification) => app.add_notification(notification),
            SessionCommand::ToggleMessages => app.toggle_messages(),
            SessionCommand::ShowMessages => app.show_messages(),
            SessionCommand::HideMessages => app.hide_messages(),
            SessionCommand::ClearMessages => app.clear_messages(),
            SessionCommand::GetSystemPrompt(reply) => {
                let _ = reply.send(agent.get_system_prompt());
            }
            SessionCommand::SetSystemPrompt(prompt, reply) => {
                let result = agent
                    .set_system_prompt(&prompt)
                    .await
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            SessionCommand::MarkFirstMessageSent => {}
            SessionCommand::SetVariable { .. } | SessionCommand::GetVariable { .. } => {}
        }
    }

    /// Handle a SessionEvent, dispatching to appropriate ChatAppMsg.
    ///
    /// Returns Some(ChatAppMsg) if the event should be forwarded to the app,
    /// or None if the event was handled internally or should be skipped.
    pub fn handle_session_event(event: SessionEvent) -> Option<ChatAppMsg> {
        match event {
            SessionEvent::InteractionRequested {
                request_id,
                request,
            } => match &request {
                InteractionRequest::Ask(_) | InteractionRequest::Permission(_) => {
                    Some(ChatAppMsg::OpenInteraction {
                        request_id,
                        request,
                    })
                }
                InteractionRequest::AskBatch(_)
                | InteractionRequest::Edit(_)
                | InteractionRequest::Show(_)
                | InteractionRequest::Popup(_)
                | InteractionRequest::Panel(_) => Some(ChatAppMsg::OpenInteraction {
                    request_id,
                    request,
                }),
            },
            SessionEvent::DelegationSpawned {
                delegation_id,
                prompt,
                target_agent,
                ..
            } => Some(ChatAppMsg::DelegationSpawned {
                id: delegation_id,
                prompt,
                target_agent,
            }),
            SessionEvent::DelegationCompleted {
                delegation_id,
                result_summary,
                ..
            } => Some(ChatAppMsg::DelegationCompleted {
                id: delegation_id,
                summary: result_summary,
            }),
            SessionEvent::DelegationFailed {
                delegation_id,
                error,
                ..
            } => Some(ChatAppMsg::DelegationFailed {
                id: delegation_id,
                error,
            }),
            _ => None,
        }
    }
}

/// Convert a session event into `ChatAppMsg`(s) for the TUI.
///
/// Returns zero or more messages. The `tool_result` event produces two messages
/// (delta + complete), while most events produce one. `replay_complete` and
/// unknown event types return an empty Vec.
pub fn session_event_to_chat_msgs(event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
    match event_type {
        "user_message" => data
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| vec![ChatAppMsg::UserMessage(c.to_string())])
            .unwrap_or_default(),
        "text_delta" => data
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| vec![ChatAppMsg::TextDelta(c.to_string())])
            .unwrap_or_default(),
        "thinking" => data
            .get("content")
            .and_then(|v| v.as_str())
            .map(|c| vec![ChatAppMsg::ThinkingDelta(c.to_string())])
            .unwrap_or_default(),
        "tool_call" => {
            let name = data
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            let args = data.get("args").map(|v| v.to_string()).unwrap_or_default();
            let call_id = data
                .get("call_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            // Descriptions are not shown during live streaming (the LLM chunk
            // doesn't include them), so omit them on resume for consistency.
            let description = None;
            let source = data
                .get("source")
                .and_then(|v| v.as_str())
                .map(String::from);
            let lua_primary_arg = data
                .get("lua_primary_arg")
                .and_then(|v| v.as_str())
                .map(String::from);
            let diffs = match data.get("diffs") {
                Some(raw) => match serde_json::from_value(raw.clone()) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        tracing::warn!(
                            target: "tui",
                            error = %err,
                            tool = ?data.get("tool"),
                            call_id = ?data.get("call_id"),
                            raw = %raw,
                            "tool_call event carried a malformed `diffs` field; \
                             ignoring and continuing with empty Vec",
                        );
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            vec![ChatAppMsg::ToolCall {
                name,
                args,
                call_id,
                description,
                source,
                lua_primary_arg,
                diffs,
            }]
        }
        "tool_call_diff_update" => {
            let Some(call_id) = data
                .get("call_id")
                .and_then(|v| v.as_str())
                .map(String::from)
            else {
                return Vec::new();
            };
            let diffs = match data.get("diffs") {
                Some(raw) => match serde_json::from_value(raw.clone()) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        tracing::warn!(
                            target: "tui",
                            error = %err,
                            call_id = %call_id,
                            raw = %raw,
                            "tool_call_diff_update event carried a malformed `diffs` field; \
                             ignoring",
                        );
                        return Vec::new();
                    }
                },
                None => Vec::new(),
            };
            if diffs.is_empty() {
                return Vec::new();
            }
            vec![ChatAppMsg::ToolCallDiffUpdate { call_id, diffs }]
        }
        "tool_result" => {
            let name = data
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            let call_id = data
                .get("call_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let result_data = data.get("result");
            let error = result_data
                .and_then(|r| r.get("error"))
                .and_then(|e| e.as_str());

            if let Some(err) = error {
                vec![ChatAppMsg::ToolResultError {
                    name,
                    error: strip_tool_error_prefix(err),
                    call_id,
                }]
            } else {
                let result_str = result_data
                    .and_then(|r| r.get("result"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Strip nested tool-error prefixes from result text that
                // looks like an error (matches old handle_stream_chunk
                // behaviour).
                let result_str = if result_str.starts_with("Error: ") {
                    strip_tool_error_prefix(result_str)
                } else {
                    result_str.to_string()
                };
                vec![
                    ChatAppMsg::ToolResultDelta {
                        name: name.clone(),
                        delta: result_str,
                        call_id: call_id.clone(),
                    },
                    ChatAppMsg::ToolResultComplete { name, call_id },
                ]
            }
        }
        "message_complete" => {
            let mut msgs = Vec::new();
            // Reconstruct the full response text from the persisted snapshot.
            // text_delta events are not persisted (too granular), so this is
            // the only source of assistant text on resume.
            if let Some(text) = data.get("full_response").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    msgs.push(ChatAppMsg::TextDelta(text.to_string()));
                }
            }
            // If the daemon attached token counts to message_complete, surface
            // them as ContextUsage. The `total` side requires a context-limit
            // lookup, which the standalone converter cannot do — the caller
            // (SessionEventStream) fills it in.
            if let Some(total_tokens) = data.get("total_tokens").and_then(|v| v.as_u64()) {
                msgs.push(ChatAppMsg::ContextUsage {
                    used: total_tokens as usize,
                    total: 0,
                });
            }
            msgs.push(ChatAppMsg::StreamComplete);
            msgs
        }
        "precognition_complete" => {
            let notes_count = data
                .get("notes_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(0);
            let notes = data
                .get("notes")
                .and_then(|v| {
                    serde_json::from_value::<
                        Vec<crucible_core::traits::chat::PrecognitionNoteInfo>,
                    >(v.clone())
                    .ok()
                })
                .unwrap_or_default();
            if notes_count > 0 {
                vec![ChatAppMsg::PrecognitionResult { notes_count, notes }]
            } else {
                vec![]
            }
        }
        "delegation_spawned" => {
            let id = data
                .get("delegation_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let prompt = data
                .get("prompt")
                .and_then(|v| v.as_str())
                .map(String::from);
            let target_agent = data
                .get("target_agent")
                .and_then(|v| v.as_str())
                .map(String::from);

            match (id, prompt) {
                (Some(id), Some(prompt)) => vec![ChatAppMsg::DelegationSpawned {
                    id,
                    prompt,
                    target_agent,
                }],
                _ => vec![],
            }
        }
        "delegation_completed" => {
            let id = data
                .get("delegation_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let summary = data
                .get("result_summary")
                .and_then(|v| v.as_str())
                .map(String::from);

            match (id, summary) {
                (Some(id), Some(summary)) => {
                    vec![ChatAppMsg::DelegationCompleted { id, summary }]
                }
                _ => vec![],
            }
        }
        "delegation_failed" => {
            let id = data
                .get("delegation_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let error = data.get("error").and_then(|v| v.as_str()).map(String::from);

            match (id, error) {
                (Some(id), Some(error)) => vec![ChatAppMsg::DelegationFailed { id, error }],
                _ => vec![],
            }
        }
        "subagent_spawned" => {
            let id = data
                .get("job_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let prompt = data
                .get("prompt")
                .and_then(|v| v.as_str())
                .map(String::from);
            match (id, prompt) {
                (Some(id), Some(prompt)) => vec![ChatAppMsg::SubagentSpawned { id, prompt }],
                (Some(id), None) => vec![ChatAppMsg::SubagentSpawned {
                    id,
                    prompt: String::new(),
                }],
                _ => vec![],
            }
        }
        "subagent_completed" => {
            let id = data
                .get("job_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let summary = data
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from);
            match (id, summary) {
                (Some(id), Some(summary)) => vec![ChatAppMsg::SubagentCompleted { id, summary }],
                (Some(id), None) => vec![ChatAppMsg::SubagentCompleted {
                    id,
                    summary: String::new(),
                }],
                _ => vec![],
            }
        }
        "subagent_failed" => {
            let id = data
                .get("job_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let error = data.get("error").and_then(|v| v.as_str()).map(String::from);
            match (id, error) {
                (Some(id), Some(error)) => vec![ChatAppMsg::SubagentFailed { id, error }],
                (Some(id), None) => vec![ChatAppMsg::SubagentFailed {
                    id,
                    error: "Unknown error".to_string(),
                }],
                _ => vec![],
            }
        }
        "replay_complete" => vec![],
        "session_initialized" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::SessionInitializedPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::SessionInitialized(payload)],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode session_initialized payload");
                    vec![]
                }
            }
        }
        "providers_listed" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::ProvidersListedPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::ProvidersListed(payload.providers)],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode providers_listed payload");
                    vec![]
                }
            }
        }
        "context_limit_resolved" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::ContextLimitResolvedPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::ContextLimitResolved {
                    limit: payload.limit,
                    source: payload.source,
                }],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode context_limit_resolved payload");
                    vec![]
                }
            }
        }
        "workspace_indexed" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::WorkspaceIndexedPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::WorkspaceIndexed(payload.files)],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode workspace_indexed payload");
                    vec![]
                }
            }
        }
        "kiln_notes_indexed" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::KilnNotesIndexedPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::KilnNotesIndexed(payload.notes)],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode kiln_notes_indexed payload");
                    vec![]
                }
            }
        }
        "plugins_discovered" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::PluginsDiscoveredPayload,
            >(data.clone())
            {
                Ok(payload) => vec![ChatAppMsg::PluginsDiscovered(payload.plugins)],
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode plugins_discovered payload");
                    vec![]
                }
            }
        }
        "mcp_servers_ready" => {
            match serde_json::from_value::<
                crucible_core::protocol::session_events::McpServersReadyPayload,
            >(data.clone())
            {
                Ok(payload) => {
                    // Map McpServerInfo (tools: Vec<String>) → McpServerDisplay
                    // (tool_count: usize). The TUI renders tool_count only;
                    // collapsing at the boundary keeps the rest of the TUI
                    // unchanged. The real connected-state / tool count is
                    // refreshed later by the background MCP gateway task.
                    let servers: Vec<McpServerDisplay> = payload
                        .servers
                        .into_iter()
                        .map(|s| McpServerDisplay {
                            name: s.name,
                            prefix: s.prefix.trim_end_matches('_').to_string(),
                            tool_count: s.tools.len(),
                            connected: s.connected,
                        })
                        .collect();
                    vec![ChatAppMsg::McpServersReady(servers)]
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode mcp_servers_ready payload");
                    vec![]
                }
            }
        }
        _ => {
            tracing::trace!(event_type = %event_type, "Skipping unknown session event");
            vec![]
        }
    }
}
