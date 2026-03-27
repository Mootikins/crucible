//! Message dispatch handlers for OilChatApp.
//!
//! Groups the four `handle_*_msg` methods that dispatch incoming `ChatAppMsg`
//! variants to the appropriate state update logic.

use std::sync::Arc;

use crate::tui::oil::app::Action;
use crate::tui::oil::viewport_cache::{CachedSubagent, CachedToolCall, ToolSourceDisplay};

use super::messages::ChatAppMsg;
use super::model_state::ModelListState;
use super::state::AutocompleteKind;
use super::OilChatApp;

fn parse_tool_source(s: &str) -> Option<ToolSourceDisplay> {
    match s {
        "Core" => Some(ToolSourceDisplay::Core),
        "Crucible" => Some(ToolSourceDisplay::Crucible),
        s if s.starts_with("Mcp:") => Some(ToolSourceDisplay::Mcp {
            server: Arc::from(&s[4..]),
        }),
        s if s.starts_with("Plugin:") => Some(ToolSourceDisplay::Plugin {
            name: Arc::from(&s[7..]),
        }),
        _ => None,
    }
}

impl OilChatApp {
    pub(super) fn handle_stream_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::TextDelta(delta) => {
                if self.drop_stream_deltas {
                    return Action::Continue;
                }
                self.container_list.append_text(&delta);
                Action::Continue
            }
            ChatAppMsg::ThinkingDelta(delta) => {
                self.container_list.append_thinking(&delta);
                Action::Continue
            }
            ChatAppMsg::ToolCall {
                name,
                args,
                call_id,
                description,
                source,
                lua_primary_arg,
            } => {
                if !self.container_list.is_streaming() {
                    self.container_list.mark_turn_active();
                }
                self.message_queue.message_counter += 1;
                let tool_id = format!("tool-{}", self.message_queue.message_counter);
                tracing::debug!(
                    tool_name = %name,
                    ?call_id,
                    args_len = args.len(),
                    counter = self.message_queue.message_counter,
                    "Adding ToolCall"
                );
                let mut tool = CachedToolCall::new(tool_id, &name, &args);
                tool.call_id = call_id;
                tool.description = description.map(Arc::from);
                tool.source = source.and_then(|s| parse_tool_source(&s));
                tool.lua_primary_arg = lua_primary_arg.map(Arc::from);
                if name == "delegate_session" && !self.pending_delegate_supersessions.is_empty() {
                    tool.superseded = true;
                    if let Some(pending_id) =
                        self.pending_delegate_supersessions.iter().next().cloned()
                    {
                        self.pending_delegate_supersessions.remove(&pending_id);
                    }
                }
                self.container_list.add_tool_call(tool);
                Action::Continue
            }
            ChatAppMsg::ToolResultDelta {
                name,
                delta,
                call_id,
            } => {
                tracing::debug!(
                    tool_name = %name,
                    ?call_id,
                    delta_len = delta.len(),
                    "Received ToolResultDelta"
                );
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.append_output(&delta);
                    });
                Action::Continue
            }
            ChatAppMsg::ToolResultComplete { name, call_id } => {
                tracing::debug!(tool_name = %name, ?call_id, "Received ToolResultComplete");
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.mark_complete();
                    });
                Action::Continue
            }
            ChatAppMsg::ToolResultError {
                name,
                error,
                call_id,
            } => {
                tracing::debug!(tool_name = %name, ?call_id, error = %error, "Received ToolResultError");
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.set_error(crucible_core::error_utils::strip_tool_error_prefix(&error));
                    });
                Action::Continue
            }
            ChatAppMsg::StreamComplete => {
                self.drop_stream_deltas = false;
                self.finalize_streaming();
                self.process_deferred_queue()
            }
            ChatAppMsg::StreamCancelled => {
                self.drop_stream_deltas = true;
                self.finalize_streaming();
                self.add_notification(crucible_core::types::Notification::toast("Cancelled"));
                self.process_deferred_queue()
            }
            _ => Action::Continue,
        }
    }

    pub(super) fn handle_config_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::SwitchModel(model) => {
                self.model = model;
                self.status = format!("Model: {}", self.model);
                Action::Continue
            }
            ChatAppMsg::FetchModels => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "handle_config_msg: FetchModels -> state=Loading");
                self.model_list_state = ModelListState::Loading;
                self.status = "Fetching models...".to_string();
                Action::Continue
            }
            ChatAppMsg::ModelsLoaded(models) => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", count = models.len(), "handle_config_msg: ModelsLoaded -> state=Loaded");
                self.available_models = models;
                self.model_list_state = ModelListState::Loaded;
                self.model_fetch_message_shown = false;
                if self.popup.kind == AutocompleteKind::Model && self.popup.show {
                    self.popup.selected = 0;
                }
                Action::Continue
            }
            ChatAppMsg::ModelsFetchFailed(reason) => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", error = %reason, "handle_config_msg: ModelsFetchFailed -> state=Failed");
                self.model_list_state = ModelListState::Failed(reason.clone());
                self.model_fetch_message_shown = false;
                self.notification_area
                    .add(crucible_core::types::Notification::warning(format!(
                        "Failed to fetch models: {}",
                        reason
                    )));
                Action::Continue
            }
            ChatAppMsg::McpStatusLoaded(servers) => {
                let connected = servers.iter().filter(|s| s.connected).count();
                let tools: usize = servers.iter().map(|s| s.tool_count).sum();
                self.set_mcp_servers(servers.clone());
                if connected > 0 {
                    self.add_notification(crucible_core::types::Notification::toast(format!(
                        "MCP: {} server(s) connected, {} tools",
                        connected, tools
                    )));
                }
                Action::Continue
            }
            ChatAppMsg::PluginStatusLoaded(entries) => {
                // Error notifications are surfaced once during runner init
                // (see OilChatRunner::setup_app). Only store status here.
                self.plugin_status = entries;
                Action::Continue
            }
            ChatAppMsg::SetThinkingBudget(_) => Action::Continue,
            ChatAppMsg::SetTemperature(_) => Action::Continue,
            ChatAppMsg::SetMaxTokens(_) => Action::Continue,
            ChatAppMsg::SetMaxIterations(_) => Action::Continue,
            ChatAppMsg::SetExecutionTimeout(_) => Action::Continue,
            ChatAppMsg::SetContextBudget(_) => Action::Continue,
            ChatAppMsg::SetContextStrategy(_) => Action::Continue,
            ChatAppMsg::SetContextWindow(_) => Action::Continue,
            ChatAppMsg::SetOutputValidation(_) => Action::Continue,
            ChatAppMsg::SetValidationRetries(_) => Action::Continue,
            _ => Action::Continue,
        }
    }

    pub(super) fn handle_delegation_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::SubagentSpawned { id, prompt } => {
                if !self.container_list.is_streaming() {
                    self.container_list.mark_turn_active();
                }
                self.container_list
                    .add_agent_task(CachedSubagent::new(id, &prompt, "subagent"), "subagent");
                Action::Continue
            }
            ChatAppMsg::SubagentCompleted { id, summary } => {
                self.container_list.update_agent_task(&id, |s| {
                    s.mark_completed(&summary);
                });
                Action::Continue
            }
            ChatAppMsg::SubagentFailed { id, error } => {
                self.container_list.update_agent_task(&id, |s| {
                    s.mark_failed(&error);
                });
                Action::Continue
            }
            ChatAppMsg::DelegationSpawned {
                id,
                prompt,
                target_agent,
            } => {
                if !self.container_list.is_streaming() {
                    self.container_list.mark_turn_active();
                }
                let mut delegation = CachedSubagent::new(id.clone(), &prompt, "delegation");
                delegation.target_agent = target_agent.clone();
                self.container_list.add_agent_task(delegation, "delegation");
                if !self
                    .container_list
                    .supersede_most_recent_tool("delegate_session")
                {
                    self.pending_delegate_supersessions.insert(id);
                }
                Action::Continue
            }
            ChatAppMsg::DelegationCompleted { id, summary } => {
                self.container_list.update_agent_task(&id, |d| {
                    d.mark_completed(&summary);
                });
                Action::Continue
            }
            ChatAppMsg::DelegationFailed { id, error } => {
                self.container_list.update_agent_task(&id, |d| {
                    d.mark_failed(&error);
                });
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    pub(super) fn handle_ui_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::QueueMessage(content) => {
                if self.is_streaming() {
                    self.message_queue.deferred_messages.push_back(content);
                    let count = self.message_queue.deferred_messages.len();
                    self.add_notification(crucible_core::types::Notification::toast(format!(
                        "{} message{} queued",
                        count,
                        if count == 1 { "" } else { "s" }
                    )));
                    Action::Continue
                } else {
                    self.submit_user_message(content.clone());
                    Action::Send(ChatAppMsg::UserMessage(content))
                }
            }
            ChatAppMsg::Error(msg) => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(msg));
                self.container_list.cancel_streaming();
                Action::Continue
            }
            ChatAppMsg::Status(status) => {
                self.status = status;
                Action::Continue
            }
            ChatAppMsg::ModeChanged(mode) => {
                self.mode = super::state::ChatMode::parse(&mode);
                Action::Continue
            }
            ChatAppMsg::ContextUsage { used, total } => {
                self.context_used = used;
                self.context_total = total;
                Action::Continue
            }
            ChatAppMsg::ClearHistory => Action::Continue,
            ChatAppMsg::Undo(_) => {
                // Side effects handled in chat_runner; this is the state-update pass
                Action::Continue
            }
            ChatAppMsg::UndoComplete {
                turns,
                messages_removed,
            } => {
                self.notification_area
                    .add(crucible_core::types::Notification::toast(format!(
                        "Undid {} turn{} ({} messages removed)",
                        turns,
                        if turns == 1 { "" } else { "s" },
                        messages_removed,
                    )));
                // Re-render the chat view by marking a full redraw
                self.needs_full_redraw = true;
                Action::Continue
            }
            ChatAppMsg::ToggleMessages => {
                self.notification_area.toggle();
                Action::Continue
            }
            ChatAppMsg::OpenInteraction {
                request_id,
                request,
            } => self.open_interaction(request_id, request),
            ChatAppMsg::CloseInteraction {
                request_id: _,
                response: _,
            } => {
                // Response handling will be implemented in a later task
                self.close_interaction();
                Action::Continue
            }
            ChatAppMsg::LoadHistoryEvents(events) => {
                self.load_history_events(events);
                Action::Continue
            }
            ChatAppMsg::PrecognitionResult { notes_count, notes } => {
                if notes_count > 0 {
                    if notes.is_empty() {
                        self.add_system_message(format!("Found {} relevant notes", notes_count));
                    } else {
                        let mut msg = format!("Found {} relevant notes:", notes_count);
                        for note in &notes {
                            if let Some(label) = &note.kiln_label {
                                msg.push_str(&format!("\n  \u{00B7} {} [{}]", note.title, label));
                            } else {
                                msg.push_str(&format!("\n  \u{00B7} {}", note.title));
                            }
                        }
                        self.add_system_message(msg);
                    }
                }
                Action::Continue
            }
            ChatAppMsg::EnrichedMessage { .. } => {
                // Handled by the runner — starts agent stream with enriched content
                Action::Continue
            }
            ChatAppMsg::ExecuteSlashCommand(_)
            | ChatAppMsg::ExportSession(_)
            | ChatAppMsg::ReloadPlugin(_) => Action::Continue,
            _ => Action::Continue,
        }
    }
}
