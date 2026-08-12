use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay, OilChatApp};
use crucible_core::error_utils::strip_tool_error_prefix;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::protocol::session_events::{
    EventDecodeError, JobPayload, SessionEventPayload, SettingsPayload, SetupPayload,
    SystemPayload, ToolResultBody, TurnPayload,
};
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
            // Previously a no-op arm: `session:set_variable` silently
            // discarded and `get_variable` always returned nil, so a plugin
            // stashing state across handlers got documented silence.
            SessionCommand::SetVariable { key, value } => {
                app.set_session_variable(key, value);
            }
            SessionCommand::GetVariable { key, response } => {
                let _ = response.send(app.session_variable(&key));
            }
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
            // The three delegation arms that used to live here were dead: this
            // function's only caller (`runner.rs`) invokes it exclusively with
            // `SessionEvent::InteractionRequested`, and the live delegation
            // mapping is the wire one in `session_event_to_chat_msgs`. Two
            // implementations of the same mapping had already diverged.
            _ => None,
        }
    }
}

/// Convert a session event into `ChatAppMsg`(s) for the TUI.
///
/// Returns zero or more messages. The `tool_result` event produces two messages
/// (delta + complete), while most events produce one. `replay_complete` and
/// unknown event types return an empty Vec.
///
/// Keyed on the typed payload rather than on `data.get("…")`: a new event in a
/// group the TUI handles now fails to compile here instead of falling through to
/// the `trace!` arm.
pub fn session_event_to_chat_msgs(event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
    // `subagent_*` are NOT on the wire — they exist only as
    // `InternalSessionEvent` variants for Lua, so they have no
    // `SessionEventPayload` name. The arms stay because `ChatAppMsg` carries the
    // variants and a test pins them; whether the wire should carry them is a
    // feature question with a missing producer, not three orphan consumers.
    if let Some(msgs) = subagent_msgs(event_type, data) {
        return msgs;
    }

    // Also not in the typed vocabulary, and for a structural reason rather than
    // an oversight: `stream_gap` is minted per *connection* by the daemon's event
    // forwarder when this client's broadcast cursor falls off the ring
    // (`daemon/src/server/core.rs`), so no session produces it and
    // `SessionEventPayload` has no name for it. It must be handled before the
    // typed dispatch, because that dispatch's `UnknownEvent` arm is a silent
    // `trace!` — which is exactly how the gap stayed invisible.
    if event_type == "stream_gap" {
        return vec![stream_gap_msg(data)];
    }

    match SessionEventPayload::from_wire(event_type, data) {
        Ok(SessionEventPayload::Turn(turn)) => turn_msgs(turn),
        Ok(SessionEventPayload::Setup(setup)) => setup_msgs(setup),
        Ok(SessionEventPayload::Settings(settings)) => settings_msgs(settings),
        Ok(SessionEventPayload::Job(job)) => job_msgs(job),
        Ok(SessionEventPayload::System(system)) => system_msgs(system),
        Ok(SessionEventPayload::Review(_))
        | Ok(SessionEventPayload::Notification(_))
        | Ok(SessionEventPayload::Workflow(_)) => vec![],
        Err(EventDecodeError::UnknownEvent { event }) => {
            tracing::trace!(event_type = %event, "Skipping unknown session event");
            vec![]
        }
        Err(e @ EventDecodeError::MalformedPayload { .. }) => {
            tracing::warn!(error = %e, "Dropping malformed session event");
            vec![]
        }
    }
}

/// Say out loud that this transcript is missing events.
///
/// Routed through `ChatAppMsg::Error`, which surfaces as a warning notification.
/// A gap is not a turn failure, but it is the one thing the user cannot find out
/// any other way: the events are gone from this connection and no later event
/// mentions them, so a message that is easy to miss is the same as no message.
///
/// A missing `dropped` still warns. The count is the daemon's own field, so its
/// absence means a version skew — not a reason to swallow the fact of the loss.
fn stream_gap_msg(data: &serde_json::Value) -> ChatAppMsg {
    // Count first: the status bar shows one truncated line, so the number has to
    // survive the truncation to be worth carrying.
    let dropped = data.get("dropped").and_then(|v| v.as_u64());
    ChatAppMsg::Error(match dropped {
        Some(n) => format!(
            "{n} events dropped: the event stream fell behind. \
             This conversation is incomplete — reload the session."
        ),
        None => "Events dropped: the event stream fell behind. \
                 This conversation is incomplete — reload the session."
            .to_string(),
    })
}

/// `Some` only for the three names that never cross the wire.
fn subagent_msgs(event_type: &str, data: &serde_json::Value) -> Option<Vec<ChatAppMsg>> {
    let id = data.get("job_id").and_then(|v| v.as_str())?.to_string();
    let text = |key: &str| {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Some(match event_type {
        "subagent_spawned" => vec![ChatAppMsg::SubagentSpawned {
            id,
            prompt: text("prompt"),
        }],
        "subagent_completed" => vec![ChatAppMsg::SubagentCompleted {
            id,
            summary: text("summary"),
        }],
        "subagent_failed" => {
            let error = data
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            vec![ChatAppMsg::SubagentFailed { id, error }]
        }
        _ => return None,
    })
}

/// Empty strings and absent keys are the same thing here: every one of these
/// fields is `#[serde(default)]`, so a missing key arrives as `""`, and the
/// untyped code this replaces dropped the message rather than pushing an empty
/// one.
fn non_empty(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

fn turn_msgs(turn: TurnPayload) -> Vec<ChatAppMsg> {
    match turn {
        TurnPayload::UserMessage { content, .. } => non_empty(content)
            .map(|c| vec![ChatAppMsg::UserMessage(c)])
            .unwrap_or_default(),
        TurnPayload::TextDelta { content } => non_empty(content)
            .map(|c| vec![ChatAppMsg::TextDelta(c)])
            .unwrap_or_default(),
        TurnPayload::Thinking { content } => non_empty(content)
            .map(|c| vec![ChatAppMsg::ThinkingDelta(c)])
            .unwrap_or_default(),
        TurnPayload::ToolCall {
            call_id,
            tool,
            args,
            source,
            lua_primary_arg,
            auto_approved,
            diffs,
            ..
        } => {
            vec![ChatAppMsg::ToolCall {
                name: non_empty(tool).unwrap_or_else(|| "tool".to_string()),
                args: if args.is_null() {
                    String::new()
                } else {
                    args.to_string()
                },
                call_id: non_empty(call_id),
                // Descriptions are not shown during live streaming (the LLM
                // chunk doesn't include them), so omit them on resume for
                // consistency.
                description: None,
                source,
                lua_primary_arg,
                diffs,
                auto_approved,
            }]
        }
        TurnPayload::ToolCallDiffUpdate { call_id, diffs } => {
            let Some(call_id) = non_empty(call_id) else {
                return Vec::new();
            };
            if diffs.is_empty() {
                return Vec::new();
            }
            vec![ChatAppMsg::ToolCallDiffUpdate { call_id, diffs }]
        }
        TurnPayload::ToolCallArgsUpdate { call_id, args } => {
            let Some(call_id) = non_empty(call_id) else {
                return Vec::new();
            };
            // Same noise filter as the diff path: an empty or null payload
            // carries nothing worth disturbing the existing card for.
            if args.is_null() || args == serde_json::json!({}) {
                return Vec::new();
            }
            let args = serde_json::to_string(&args).unwrap_or_default();
            if args.is_empty() {
                return Vec::new();
            }
            vec![ChatAppMsg::ToolCallArgsUpdate { call_id, args }]
        }
        TurnPayload::ToolResult {
            call_id,
            tool,
            result,
            ..
        } => {
            let name = non_empty(tool).unwrap_or_else(|| "tool".to_string());
            let call_id = non_empty(call_id);
            let body = ToolResultBody::of(&result);
            if let Some(err) = body.as_ref().and_then(|b| b.error()) {
                return vec![ChatAppMsg::ToolResultError {
                    name,
                    error: strip_tool_error_prefix(err),
                    call_id,
                }];
            }
            let result_str = match &body {
                Some(ToolResultBody::Ok { result, .. }) => result.as_str().unwrap_or(""),
                _ => "",
            };
            // Strip nested tool-error prefixes from result text that looks like
            // an error (matches old handle_stream_chunk behaviour).
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
        TurnPayload::MessageComplete {
            full_response,
            total_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            ..
        } => {
            let mut msgs = Vec::new();
            // Reconstruct the full response text from the persisted snapshot.
            // text_delta events are not persisted (too granular), so this is the
            // only source of assistant text on resume.
            if let Some(text) = non_empty(full_response) {
                msgs.push(ChatAppMsg::TextDelta(text));
            }
            // If the daemon attached token counts, surface them as ContextUsage.
            // The `total` side requires a context-limit lookup, which the
            // standalone converter cannot do — the caller (SessionEventStream)
            // fills it in.
            if let Some(total) = total_tokens {
                msgs.push(ChatAppMsg::ContextUsage {
                    used: total as usize,
                    total: 0,
                });
            }
            // Cache hit rate from the per-event token fields. Both are optional;
            // emit only when at least one is present so the StatusBar's "no
            // data" sentinel still works for older sessions.
            if cache_read_tokens.is_some() || cache_creation_tokens.is_some() {
                let read = u64::from(cache_read_tokens.unwrap_or(0));
                let creation = u64::from(cache_creation_tokens.unwrap_or(0));
                let denom = read + creation;
                let rate = (denom != 0).then(|| read as f64 / denom as f64);
                msgs.push(ChatAppMsg::CacheHitRate(rate));
            }
            msgs.push(ChatAppMsg::StreamComplete);
            msgs
        }
        TurnPayload::PrecognitionComplete {
            notes_count, notes, ..
        } => {
            if notes_count > 0 {
                vec![ChatAppMsg::PrecognitionResult { notes_count, notes }]
            } else {
                vec![]
            }
        }
        // Rendered by other paths or not rendered at all: `segment_complete` is
        // additive over `message_complete`'s text, `ended` is handled by the
        // stateful wrapper, interactions ride their own channel, and the rest is
        // context plumbing and telemetry.
        TurnPayload::SegmentComplete { .. }
        | TurnPayload::Ended { .. }
        | TurnPayload::InteractionRequested { .. }
        | TurnPayload::InteractionCompleted { .. }
        | TurnPayload::InjectionPending { .. }
        | TurnPayload::ContextInjected { .. }
        | TurnPayload::PostLlmCall { .. } => vec![],
    }
}

/// The seven setup payloads used to be decoded one at a time, each with its own
/// warn-and-drop block. One decode, one exhaustive match.
fn setup_msgs(setup: SetupPayload) -> Vec<ChatAppMsg> {
    match setup {
        SetupPayload::SessionInitialized(p) => vec![ChatAppMsg::SessionInitialized(p)],
        SetupPayload::ProvidersListed(p) => vec![ChatAppMsg::ProvidersListed(p.providers)],
        SetupPayload::ContextLimitResolved(p) => vec![ChatAppMsg::ContextLimitResolved {
            limit: p.limit,
            source: p.source,
        }],
        SetupPayload::WorkspaceIndexed(p) => vec![ChatAppMsg::WorkspaceIndexed(p.files)],
        SetupPayload::KilnNotesIndexed(p) => vec![ChatAppMsg::KilnNotesIndexed(p.notes)],
        SetupPayload::PluginsDiscovered(p) => vec![ChatAppMsg::PluginsDiscovered(p.plugins)],
        SetupPayload::McpServersReady(p) => {
            // Map McpServerInfo (tools: Vec<String>) → McpServerDisplay
            // (tool_count: usize). The TUI renders tool_count only; collapsing at
            // the boundary keeps the rest of the TUI unchanged. The real
            // connected-state / tool count is refreshed later by the background
            // MCP gateway task.
            let servers: Vec<McpServerDisplay> = p
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
    }
}

fn settings_msgs(settings: SettingsPayload) -> Vec<ChatAppMsg> {
    match settings {
        // A mode change made anywhere else — the web UI, another client, a Lua
        // handler — reaches the statusline only through this arm. Without it the
        // daemon emitted `mode_changed` to nobody on the TUI side, and the badge
        // kept showing the mode this client last set itself.
        SettingsPayload::ModeChanged { mode } => non_empty(mode)
            .map(|m| vec![ChatAppMsg::ModeSynced(m)])
            .unwrap_or_default(),
        // The rest are acknowledgements of a change this client either made or
        // can re-read from the session record.
        _ => vec![],
    }
}

fn job_msgs(job: JobPayload) -> Vec<ChatAppMsg> {
    match job {
        JobPayload::DelegationSpawned {
            delegation_id,
            prompt,
            target_agent,
            ..
        } => match (non_empty(delegation_id), non_empty(prompt)) {
            (Some(id), Some(prompt)) => vec![ChatAppMsg::DelegationSpawned {
                id,
                prompt,
                target_agent,
            }],
            _ => vec![],
        },
        JobPayload::DelegationCompleted {
            delegation_id,
            result_summary,
            ..
        } => match (non_empty(delegation_id), non_empty(result_summary)) {
            (Some(id), Some(summary)) => vec![ChatAppMsg::DelegationCompleted { id, summary }],
            _ => vec![],
        },
        JobPayload::DelegationFailed {
            delegation_id,
            error,
            ..
        } => match (non_empty(delegation_id), non_empty(error)) {
            (Some(id), Some(error)) => vec![ChatAppMsg::DelegationFailed { id, error }],
            _ => vec![],
        },
        // Bash and background jobs have no TUI surface yet.
        _ => vec![],
    }
}

fn system_msgs(system: SystemPayload) -> Vec<ChatAppMsg> {
    match system {
        // Hot reload and runtime theme switching arrive here. Applying the
        // payload and repainting are separate steps: over a socket, with the TUI
        // idle-blocked on input, a changed store repaints nothing by itself.
        SystemPayload::UiStyleChanged(config) => {
            crate::tui::oil::theme::apply_ui_config(&config);
            vec![ChatAppMsg::StyleChanged]
        }
        // `replay_complete` is consumed by the stateful wrapper, not here.
        _ => vec![],
    }
}
