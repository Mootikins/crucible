use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::{Action, App};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::AgentHandle;
use std::io;
use tokio::sync::mpsc;

use super::{DrainMessagesOutcome, OilChatRunner, ProcessActionParams};

impl OilChatRunner {
    /// Route a `ChatAppMsg` to the app reducer, and — in live mode — kick
    /// off any side-effects (e.g. sending a user message via RPC).
    ///
    /// Live sends are fire-and-forget: the daemon broadcasts the ensuing
    /// turn as `SessionEvent`s, which `session_event_consumer` feeds back
    /// into this channel. In replay mode the daemon already drives
    /// everything, so `UserMessage` is purely a display signal.
    pub(super) async fn process_message<A: AgentHandle>(
        msg: &ChatAppMsg,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        is_replay: bool,
    ) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(content) => {
                // Daemon-side precognition handles enrichment; the TUI just
                // forwards the raw user message. (Older code gated this on
                // `!app.precognition()` and waited for an EnrichedMessage
                // that no longer has a producer — every message was dropped.)
                if !is_replay && !app.is_streaming() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: content.clone(),
                        participant_id: "user".to_string(),
                    });
                    if let Err(e) = agent.send_message_fire_and_forget(content.clone()).await {
                        tracing::warn!(error = %e, "send_message_fire_and_forget failed");
                    }
                }
            }
            ChatAppMsg::FetchModels => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "drain_pending_messages: received FetchModels");
            }
            _ => {}
        }
        app.on_message(msg.clone())
    }

    pub(super) async fn drain_pending_messages<A: AgentHandle>(
        &mut self,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        msg_rx: &mut mpsc::UnboundedReceiver<ChatAppMsg>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> DrainMessagesOutcome {
        let mut processed_any = false;

        while let Ok(msg) = msg_rx.try_recv() {
            processed_any = true;

            // Handle replay-complete signal (from session_event_consumer)
            if self.is_replay && matches!(msg, ChatAppMsg::Status(ref s) if s == "Replay complete")
            {
                self.replay_remaining_completes = 0;
                if self.replay_auto_exit.is_some() {
                    *replay_auto_exit_deadline = Some(tokio::time::Instant::now());
                }
            }

            // Unified message processing for all paths.
            // In replay mode, process_message skips the RPC send so the
            // recorded events drive the UI without hitting the daemon.
            let mut action = Self::process_message(&msg, app, agent, bridge, self.is_replay).await;
            while let Action::Send(follow_up) = action {
                action =
                    Self::process_message(&follow_up, app, agent, bridge, self.is_replay).await;
            }
            if action.is_quit() {
                return DrainMessagesOutcome::Quit;
            }
        }

        if processed_any {
            DrainMessagesOutcome::Processed
        } else {
            DrainMessagesOutcome::Idle
        }
    }

    pub(super) fn should_wait_for_event(outcome: DrainMessagesOutcome) -> bool {
        matches!(outcome, DrainMessagesOutcome::Idle)
    }

    pub(super) async fn process_action<A: AgentHandle>(
        &mut self,
        params: ProcessActionParams<'_, A>,
    ) -> io::Result<bool> {
        match params.action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                match &msg {
                    ChatAppMsg::Undo(count) => {
                        if params.app.is_streaming() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Cannot undo while streaming".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        let count = *count;
                        let Some(undoable) = params.agent.as_undoable_mut() else {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Undo not supported by this agent".to_string(),
                                ),
                            );
                            return Ok(false);
                        };
                        match undoable.undo(count).await {
                            Ok(summaries) if !summaries.is_empty() => {
                                let total_removed: usize =
                                    summaries.iter().map(|s| s.messages_removed).sum();
                                let turns = summaries.len();
                                let _ = params.app.on_message(ChatAppMsg::UndoComplete {
                                    turns,
                                    messages_removed: total_removed,
                                });
                                tracing::info!(
                                    turns = turns,
                                    messages_removed = total_removed,
                                    "Agent undo completed"
                                );
                            }
                            Ok(_) => {
                                params.app.add_notification(
                                    crucible_core::types::Notification::toast(
                                        "Nothing to undo".to_string(),
                                    ),
                                );
                            }
                            Err(e) => {
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Undo failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                        return Ok(false);
                    }
                    ChatAppMsg::ClearHistory => {
                        if params.app.is_streaming() {
                            if let Err(e) = AgentHandle::cancel(params.agent).await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream");
                            }
                        }
                        match params.agent.clear_history().await {
                            Ok(()) => {
                                params.app.reset_session();
                                tracing::info!("New session started (history cleared)");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "clear_history failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Clear history failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::StreamCancelled => {
                        if params.app.is_streaming() {
                            if let Err(e) = AgentHandle::cancel(params.agent).await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream on daemon");
                            }
                            tracing::info!("Cancelled active turn via session.cancel RPC");
                        }
                    }
                    ChatAppMsg::SwitchModel(model_id) => {
                        tracing::info!(model = %model_id, "Model switch requested");
                        match AgentHandle::switch_model(params.agent, model_id).await {
                            Ok(()) => {
                                tracing::info!(model = %model_id, "Model switched successfully");
                            }
                            Err(e) => {
                                tracing::warn!(model = %model_id, error = %e, "Model switch failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Model switch failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::FetchModels if !self.is_replay => {
                        // Skipped in replay mode to preserve the TUI-only guarantee
                        // (no daemon RPC calls). Replay never populates the model
                        // picker — `:model` is moot when there's no live session.
                        //
                        // For ACP agents the fetched list is the daemon's configured
                        // internal providers, not the ACP agent's own model — trying
                        // to switch will surface a NotSupported error at that point.
                        // Spawn model fetch as background task to avoid blocking the event loop.
                        // Uses a fresh DaemonClient connection (same pattern as plugin reload).
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "background: FetchModels starting");
                            match crucible_daemon::DaemonClient::connect().await {
                                Ok(client) => {
                                    match client.list_all_models(None).await {
                                        Ok(models) if models.is_empty() => {
                                            let _ = tx.send(ChatAppMsg::ModelsFetchFailed(
                                                "No models available".to_string(),
                                            ));
                                        }
                                        Ok(models) => {
                                            tracing::info!(count = models.len(), "Models fetched successfully");
                                            let _ = tx.send(ChatAppMsg::ModelsLoaded(models));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(ChatAppMsg::ModelsFetchFailed(
                                                format!("Failed to list models: {}", e),
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(ChatAppMsg::ModelsFetchFailed(
                                        format!("Failed to connect to daemon: {}", e),
                                    ));
                                }
                            }
                        }));
                    }
                    ChatAppMsg::McpStatusLoaded(_) | ChatAppMsg::PluginStatusLoaded(_) => {
                        params.app.on_message(msg.clone());
                    }
                    ChatAppMsg::SetThinkingBudget(budget) => {
                        tracing::info!(budget = budget, "Setting thinking budget");
                        match params.agent.set_thinking_budget(*budget).await {
                            Ok(()) => {
                                tracing::info!(budget = budget, "Thinking budget set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(budget = budget, error = %e, "set_thinking_budget failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set thinking_budget failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetMaxIterations(max_iterations) => {
                        tracing::info!(max_iterations = ?max_iterations, "Setting max_iterations");
                        match params.agent.set_max_iterations(*max_iterations).await {
                            Ok(()) => {
                                tracing::info!(max_iterations = ?max_iterations, "Max iterations set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(max_iterations = ?max_iterations, error = %e, "set_max_iterations failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set max_iterations failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetExecutionTimeout(timeout_secs) => {
                        tracing::info!(timeout_secs = ?timeout_secs, "Setting execution_timeout");
                        match params.agent.set_execution_timeout(*timeout_secs).await {
                            Ok(()) => {
                                tracing::info!(timeout_secs = ?timeout_secs, "Execution timeout set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(timeout_secs = ?timeout_secs, error = %e, "set_execution_timeout failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set execution_timeout failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetContextBudget(budget) => {
                        tracing::info!(context_budget = ?budget, "Setting context_budget");
                        match params.agent.set_context_budget(*budget).await {
                            Ok(()) => {
                                tracing::info!(context_budget = ?budget, "Context budget set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(context_budget = ?budget, error = %e, "set_context_budget failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set context_budget failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetContextStrategy(strategy_str) => {
                        tracing::info!(context_strategy = %strategy_str, "Setting context_strategy");
                        match strategy_str.parse::<crucible_core::session::ContextStrategy>() {
                            Ok(strategy) => {
                                match params.agent.set_context_strategy(strategy).await {
                                    Ok(()) => {
                                        tracing::info!(context_strategy = %strategy_str, "Context strategy set successfully");
                                    }
                                    Err(e) => {
                                        tracing::warn!(context_strategy = %strategy_str, error = %e, "set_context_strategy failed");
                                        params.app.add_notification(
                                            crucible_core::types::Notification::warning(format!(
                                                "Set context_strategy failed: {}",
                                                e
                                            )),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Invalid context strategy");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Invalid context_strategy: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetContextWindow(window) => {
                        tracing::info!(context_window = ?window, "Setting context_window");
                        match params.agent.set_context_window(*window).await {
                            Ok(()) => {
                                tracing::info!(context_window = ?window, "Context window set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(context_window = ?window, error = %e, "set_context_window failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set context_window failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetOutputValidation(ref validation_str) => {
                        tracing::info!(output_validation = %validation_str, "Setting output_validation");
                        match validation_str.parse::<crucible_core::session::OutputValidation>() {
                            Ok(validation) => {
                                match params.agent.set_output_validation(validation).await {
                                    Ok(()) => {
                                        tracing::info!(output_validation = %validation_str, "Output validation set successfully");
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "set_output_validation failed");
                                        params.app.add_notification(
                                            crucible_core::types::Notification::warning(format!(
                                                "Set output_validation failed: {}",
                                                e
                                            )),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Invalid output validation");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Invalid output_validation: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetValidationRetries(retries) => {
                        tracing::info!(validation_retries = retries, "Setting validation_retries");
                        match params.agent.set_validation_retries(*retries).await {
                            Ok(()) => {
                                tracing::info!(
                                    validation_retries = retries,
                                    "Validation retries set successfully"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(validation_retries = retries, error = %e, "set_validation_retries failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set validation_retries failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::SetPrecognitionResults(count) => {
                        tracing::info!(
                            precognition_results = count,
                            "Setting precognition_results"
                        );
                        match params.agent.set_precognition_results(*count).await {
                            Ok(()) => {
                                tracing::info!(
                                    precognition_results = count,
                                    "Precognition results count set successfully"
                                );
                                params.app.set_precognition_results(*count);
                            }
                            Err(e) => {
                                tracing::warn!(precognition_results = count, error = %e, "Precognition results not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetAutocompactThreshold(threshold) => {
                        tracing::info!(autocompact_threshold = ?threshold, "Setting autocompact_threshold");
                        match params.agent.set_autocompact_threshold(*threshold).await {
                            Ok(()) => {
                                tracing::info!(
                                    autocompact_threshold = ?threshold,
                                    "Autocompact threshold set successfully"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(autocompact_threshold = ?threshold, error = %e, "set_autocompact_threshold failed");
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set autocompact_threshold failed: {}",
                                        e
                                    )),
                                );
                            }
                        }
                    }
                    ChatAppMsg::CloseInteraction {
                        request_id,
                        response,
                    } => {
                        tracing::info!(request_id = %request_id, "Sending interaction response");
                        match params
                            .agent
                            .interaction_respond(request_id.clone(), response.clone())
                            .await
                        {
                            Ok(()) => {
                                tracing::info!(request_id = %request_id, "Interaction response sent successfully");
                            }
                            Err(e) => {
                                tracing::warn!(request_id = %request_id, error = %e, "Failed to send interaction response");
                            }
                        }
                    }
                    ChatAppMsg::ModeChanged(ref mode_id) => {
                        tracing::info!(mode = %mode_id, "Mode change requested");
                        if let Err(e) = params.agent.set_mode_str(mode_id).await {
                            tracing::warn!(mode = %mode_id, error = %e, "Failed to set mode on agent");
                        }
                    }
                    ChatAppMsg::UserMessage(ref content) => {
                        // Note: do NOT gate on `app.is_streaming()` here.
                        // `handle_submit` calls `submit_user_message` (which
                        // marks the turn active so the spinner appears)
                        // BEFORE returning this action, so an is_streaming
                        // check here always trips and silently drops the
                        // send. Keypress entry is already gated against
                        // streaming upstream in input_handling.
                        if !self.is_replay {
                            params.bridge.ring.push(SessionEvent::MessageReceived {
                                content: content.clone(),
                                participant_id: "user".to_string(),
                            });
                            if let Err(e) = params
                                .agent
                                .send_message_fire_and_forget(content.clone())
                                .await
                            {
                                tracing::warn!(error = %e, "send_message_fire_and_forget failed");
                            }
                        }
                    }
                    // Gated on `!self.is_replay`: plugin reload opens a fresh
                    // `DaemonClient::connect()` and must not fire during replay.
                    ChatAppMsg::ReloadPlugin(ref name) if !self.is_replay => {
                        tracing::info!(plugin = %name, "Plugin reload requested");
                        let name = name.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            match crucible_daemon::DaemonClient::connect().await {
                                Ok(client) => {
                                    if name.is_empty() {
                                        match client.plugin_list().await {
                                            Ok(plugins) if plugins.is_empty() => {
                                                let _ = tx.send(ChatAppMsg::Status(
                                                    "No plugins loaded".to_string(),
                                                ));
                                            }
                                            Ok(plugins) => {
                                                let mut ok = 0usize;
                                                let mut errs = Vec::new();
                                                for p in &plugins {
                                                    match client.plugin_reload(p).await {
                                                        Ok(_) => ok += 1,
                                                        Err(e) => {
                                                            errs.push(format!("{}: {}", p, e))
                                                        }
                                                    }
                                                }
                                                if errs.is_empty() {
                                                    let _ = tx.send(ChatAppMsg::Status(format!(
                                                        "✓ Reloaded {} plugin(s)",
                                                        ok
                                                    )));
                                                } else {
                                                    let _ = tx.send(ChatAppMsg::Error(format!(
                                                        "Reloaded {}/{}: {}",
                                                        ok,
                                                        plugins.len(),
                                                        errs.join("; ")
                                                    )));
                                                }
                                            }
                                            Err(e) => {
                                                let _ = tx.send(ChatAppMsg::Error(format!(
                                                    "Failed to list plugins: {}",
                                                    e
                                                )));
                                            }
                                        }
                                    } else {
                                        match client.plugin_reload(&name).await {
                                            Ok(result) => {
                                                let tools = result
                                                    .get("tools")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0);
                                                let services = result
                                                    .get("services")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0);
                                                let _ = tx.send(ChatAppMsg::Status(format!(
                                                    "✓ Reloaded '{}' ({} tools, {} services)",
                                                    name, tools, services
                                                )));
                                            }
                                            Err(e) => {
                                                let _ = tx.send(ChatAppMsg::Error(format!(
                                                    "✗ Plugin reload failed: {}",
                                                    e
                                                )));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(ChatAppMsg::Error(format!(
                                        "Cannot connect to daemon: {}",
                                        e
                                    )));
                                }
                            }
                        }));
                    }
                    // Gated on `!self.is_replay`: slash commands forward to the
                    // agent (and thus the daemon). Defense-in-depth — user
                    // keystrokes during replay must not hit the daemon.
                    ChatAppMsg::ExecuteSlashCommand(ref cmd) if !self.is_replay => {
                        tracing::info!(command = %cmd, "Forwarding slash command as user message");
                        if let Err(e) = params.agent.send_message_fire_and_forget(cmd.clone()).await
                        {
                            tracing::warn!(error = %e, "send_message_fire_and_forget failed for slash command");
                        }
                    }
                    // Gated on `!self.is_replay`: export reads the recording
                    // from the session directory via `crucible_daemon::load_events`.
                    // During replay there is no live session to export.
                    ChatAppMsg::ExportSession(ref export_path) if !self.is_replay => {
                        let session_dir = match params.app.session_dir() {
                            Some(dir) => dir.to_path_buf(),
                            None => {
                                params.app.on_message(ChatAppMsg::Error(
                                    "Export failed: no active session".to_string(),
                                ));
                                return Ok(false);
                            }
                        };

                        match crucible_daemon::load_events(&session_dir).await {
                            Ok(events) if events.is_empty() => {
                                params.app.on_message(ChatAppMsg::Error(
                                    "Nothing to export — session has no recorded events"
                                        .to_string(),
                                ));
                            }
                            Ok(events) => {
                                let options = crucible_daemon::RenderOptions::default();
                                let md = crucible_daemon::render_to_markdown(&events, &options);
                                match tokio::fs::write(&export_path, &md).await {
                                    Ok(_) => {
                                        params.app.add_system_message(format!(
                                            "Session exported to {}",
                                            export_path.display()
                                        ));
                                    }
                                    Err(e) => {
                                        params.app.on_message(ChatAppMsg::Error(format!(
                                            "Export failed: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Err(e) => {
                                params.app.on_message(ChatAppMsg::Error(format!(
                                    "Failed to load session events: {}",
                                    e
                                )));
                            }
                        }
                    }
                    // Swallow daemon-bound messages during replay. The match
                    // guards on the live arms above (`if !self.is_replay`)
                    // mean these land here in replay mode. Any new daemon
                    // side-effect for these variants must stay behind that
                    // guard so the TUI-only guarantee holds.
                    ChatAppMsg::ReloadPlugin(_)
                    | ChatAppMsg::ExecuteSlashCommand(_)
                    | ChatAppMsg::ExportSession(_)
                    | ChatAppMsg::FetchModels => {
                        if self.is_replay {
                            tracing::debug!(?msg, "daemon-bound message ignored in replay mode");
                        }
                    }
                    _ => {}
                }
                let action = params.app.on_message(msg);
                Box::pin(self.process_action(ProcessActionParams {
                    action,
                    app: params.app,
                    agent: params.agent,
                    bridge: params.bridge,
                    msg_tx: params.msg_tx,
                    background_tasks: params.background_tasks,
                }))
                .await
            }
            Action::Batch(actions) => {
                for action in actions {
                    if Box::pin(self.process_action(ProcessActionParams {
                        action,
                        app: params.app,
                        agent: params.agent,
                        bridge: params.bridge,
                        msg_tx: params.msg_tx,
                        background_tasks: params.background_tasks,
                    }))
                    .await?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}
