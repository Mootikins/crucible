//! Message dispatch handlers for OilChatApp.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use crate::tui::oil::app::Action;
use crate::tui::oil::viewport_cache::{CachedSubagent, CachedToolCall, ToolSourceDisplay};

use super::messages::ChatAppMsg;
use super::model_state::ModelListState;
use super::OilChatApp;

/// Parse a tool source provenance string into a display type.
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
    /// Handle streaming events (TextDelta, ThinkingDelta, ToolCall, etc.)
    pub(super) fn handle_stream_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::TextDelta(delta) => {
                if !self.drop_stream_deltas {
                    if !self.container_list.is_streaming() {
                        self.container_list.mark_turn_active();
                    }
                    self.container_list.append_text(&delta);
                }
            }
            ChatAppMsg::ThinkingDelta(delta) => {
                if !self.drop_stream_deltas {
                    if !self.container_list.is_streaming() {
                        self.container_list.mark_turn_active();
                    }
                    self.container_list.append_thinking(&delta);
                }
            }
            ChatAppMsg::ToolCall {
                name,
                args,
                call_id,
                description,
                source,
                lua_primary_arg,
                diffs,
            } => {
                let tool = CachedToolCall {
                    id: call_id.as_deref().map_or_else(
                        || format!("tool-{}", name),
                        |cid| format!("tool-{}-{}", name, cid),
                    ),
                    name: Arc::from(name.as_str()),
                    args: Arc::from(args.as_str()),
                    call_id,
                    output_tail: VecDeque::new(),
                    output_path: None,
                    output_total_bytes: 0,
                    error: None,
                    started_at: Instant::now(),
                    complete: false,
                    superseded: false,
                    description: description.map(|d| Arc::from(d.as_str())),
                    source: source.as_deref().and_then(parse_tool_source),
                    lua_primary_arg: lua_primary_arg.map(|a| Arc::from(a.as_str())),
                    diffs,
                };
                self.container_list.add_tool_call(tool);
            }
            ChatAppMsg::ToolCallDiffUpdate { call_id, diffs } => {
                self.container_list
                    .update_tool_by_call_id(&call_id, |t| t.set_diffs(diffs));
            }
            ChatAppMsg::ToolResultDelta {
                name,
                delta,
                call_id,
            } => {
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| t.append_output(&delta));
            }
            ChatAppMsg::ToolResultComplete { name, call_id } => {
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| t.mark_complete());
            }
            ChatAppMsg::ToolResultError {
                name,
                error,
                call_id,
            } => {
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| t.set_error(error.clone()));
            }
            ChatAppMsg::StreamComplete => {
                self.container_list.complete_response();
                self.finalize_streaming();
                self.drop_stream_deltas = false;
            }
            ChatAppMsg::StreamCancelled => {
                self.container_list.cancel_streaming();
                self.finalize_streaming();
                self.drop_stream_deltas = false;
            }
            _ => {
                tracing::trace!("[stub] stream msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }

    /// Handle config messages (SwitchModel, Set*, ModelsLoaded, etc.)
    pub(super) fn handle_config_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::SwitchModel(model) => {
                self.model = model;
            }
            ChatAppMsg::FetchModels => {
                tracing::debug!(
                    target: "crucible_cli::tui::oil::model_flow",
                    msg = "FetchModels",
                    state = "Loading",
                    "model state transition"
                );
                self.model_list_state = ModelListState::Loading;
            }
            ChatAppMsg::ModelsLoaded(ref models) => {
                tracing::debug!(
                    target: "crucible_cli::tui::oil::model_flow",
                    msg = "ModelsLoaded",
                    state = "Loaded",
                    count = models.len(),
                    "model state transition"
                );
                self.available_models = models.clone();
                self.model_list_state = ModelListState::Loaded;
            }
            ChatAppMsg::ModelsFetchFailed(ref err) => {
                tracing::debug!(
                    target: "crucible_cli::tui::oil::model_flow",
                    msg = "ModelsFetchFailed",
                    state = "Failed",
                    error = %err,
                    "model state transition"
                );
                self.model_list_state = ModelListState::Failed(err.clone());
            }
            ChatAppMsg::McpStatusLoaded(servers) => {
                self.mcp_servers = servers;
            }
            ChatAppMsg::PluginStatusLoaded(entries) => {
                self.plugin_status = entries;
            }
            // Command-only: side effects handled by chat_runner::process_action
            ChatAppMsg::SetThinkingBudget(_)
            | ChatAppMsg::SetMaxIterations(_)
            | ChatAppMsg::SetExecutionTimeout(_)
            | ChatAppMsg::SetContextBudget(_)
            | ChatAppMsg::SetContextStrategy(_)
            | ChatAppMsg::SetContextWindow(_)
            | ChatAppMsg::SetOutputValidation(_)
            | ChatAppMsg::SetValidationRetries(_)
            | ChatAppMsg::SetPrecognitionResults(_)
            | ChatAppMsg::SetAutocompactThreshold(_) => {}
            _ => {
                tracing::warn!("unhandled config msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }

    /// Handle delegation events (Subagent*, Delegation*)
    pub(super) fn handle_delegation_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::SubagentSpawned { id, prompt } => {
                let agent = CachedSubagent::new(id, prompt, "subagent");
                self.container_list.add_agent_task(agent);
            }
            ChatAppMsg::SubagentCompleted { id, summary } => {
                self.container_list
                    .update_agent_task(&id, |s| s.mark_completed(&summary));
            }
            ChatAppMsg::SubagentFailed { id, error } => {
                self.container_list
                    .update_agent_task(&id, |s| s.mark_failed(&error));
            }
            ChatAppMsg::DelegationSpawned {
                id,
                prompt,
                target_agent,
            } => {
                // If this delegation supersedes a pending tool, mark it
                if self.pending_delegate_supersessions.contains(&id) {
                    self.pending_delegate_supersessions.remove(&id);
                }
                let mut agent = CachedSubagent::new(&id, prompt, "delegation");
                agent.target_agent = target_agent;
                self.container_list.add_agent_task(agent);
            }
            ChatAppMsg::DelegationCompleted { id, summary } => {
                self.container_list
                    .update_agent_task(&id, |s| s.mark_completed(&summary));
            }
            ChatAppMsg::DelegationFailed { id, error } => {
                self.container_list
                    .update_agent_task(&id, |s| s.mark_failed(&error));
            }
            _ => {
                tracing::trace!("[stub] delegation msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }

    /// Handle UI messages (ClearHistory, ToggleMessages, Status, etc.)
    pub(super) fn handle_ui_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::Error(err) => {
                self.add_notification(crucible_core::types::Notification::warning(err));
            }
            ChatAppMsg::ClearHistory => {
                self.reset_session();
            }
            ChatAppMsg::ToggleMessages => {
                self.toggle_messages();
            }
            ChatAppMsg::Status(status) => {
                self.status = status;
            }
            ChatAppMsg::LuaEvaled { output, is_error } => {
                if is_error {
                    self.notification_area
                        .add(crucible_core::types::Notification::warning(format!(
                            "lua: {}",
                            output
                        )));
                } else {
                    self.add_system_message(output);
                }
            }
            ChatAppMsg::ModeChanged(mode) => {
                self.mode = super::state::ChatMode::parse(&mode);
            }
            ChatAppMsg::ContextUsage { used, total } => {
                self.context_used = used;
                self.context_total = total;
            }
            ChatAppMsg::CacheHitRate(rate) => {
                self.cache_hit_rate = rate;
            }
            ChatAppMsg::PrecognitionResult { notes_count, notes } => {
                self.precognition.last_notes_count = Some(notes_count);
                self.precognition.last_notes = notes;
            }
            ChatAppMsg::UndoComplete {
                turns,
                messages_removed,
            } => {
                self.add_notification(crucible_core::types::Notification::toast(format!(
                    "Undid {turns} turn(s), removed {messages_removed} message(s)"
                )));
            }
            ChatAppMsg::OpenInteraction {
                request_id,
                request,
            } => {
                return self.open_interaction(request_id, request);
            }
            ChatAppMsg::CloseInteraction { .. } => {
                self.close_interaction();
                // The actual response is sent by process_action in chat_runner
            }

            // --- Setup Events (daemon → TUI, fire once per session) ---
            ChatAppMsg::SessionInitialized(payload) => {
                // Model from the daemon's SessionAgent. Empty means the
                // daemon didn't have a configured model yet (ACP sessions
                // with agent-owned model, etc.); preserve the existing
                // display value in that case.
                if !payload.model.is_empty() {
                    self.set_model(payload.model);
                }
                if !payload.mode.is_empty() {
                    self.mode = super::state::ChatMode::parse(&payload.mode);
                }
                // agent_name doesn't have a dedicated field on OilChatApp;
                // display_model already captured it at runner construction.
                // If the daemon reports a different value, log it for now.
                if let Some(name) = payload.agent_name.as_ref() {
                    tracing::debug!(agent_name = %name, "session_initialized agent_name");
                }
            }
            ChatAppMsg::ProvidersListed(providers) => {
                // Surface the first available provider as the "current"
                // display string. Matches the old preflight behavior.
                if let Some(p) = providers.iter().find(|p| p.available) {
                    self.current_provider = p.name.clone();
                }
            }
            ChatAppMsg::ContextLimitResolved { limit, source } => {
                tracing::debug!(limit, ?source, "context_limit_resolved");
                self.context_total = limit;
            }
            ChatAppMsg::WorkspaceIndexed(files) => {
                self.set_workspace_files(files);
            }
            ChatAppMsg::KilnNotesIndexed(notes) => {
                self.set_kiln_notes(notes);
            }
            ChatAppMsg::PluginsDiscovered(entries) => {
                for entry in &entries {
                    if let Some(ref err) = entry.error {
                        self.add_notification(crucible_core::types::Notification::warning(
                            format!("Plugin '{}' failed to load: {}", entry.name, err),
                        ));
                    }
                }
                self.set_plugin_status(entries);
            }
            ChatAppMsg::McpServersReady(servers) => {
                self.set_mcp_servers(servers);
                // mcp_servers_ready is the last common setup event the daemon
                // emits. Flip the status bar from "Loading..." to "Ready" so
                // the user can start typing.
                self.set_status("Ready");
            }

            // Command-only: side effects handled by chat_runner::process_action
            ChatAppMsg::ReloadPlugin(_)
            | ChatAppMsg::EvalLua(_)
            | ChatAppMsg::ConfigSet { .. }
            | ChatAppMsg::ExecuteSlashCommand(_)
            | ChatAppMsg::ExportSession(_)
            | ChatAppMsg::Undo(_) => {}
            _ => {
                tracing::warn!("unhandled ui msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }
}
