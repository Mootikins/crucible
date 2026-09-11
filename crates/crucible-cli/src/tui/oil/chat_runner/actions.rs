use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::commands::DropKind;
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::{AgentHandle, SessionKnobs};
use std::io;
use tokio::sync::mpsc;

use super::{DrainMessagesOutcome, OilChatRunner, ProcessActionParams};

/// Write one app-config key through the daemon, and report what the store
/// holds afterwards.
///
/// The read-back is the point. `config.set` can drop a key — the store
/// withholds the keys that name where the daemon acts — so echoing the value
/// that was sent would let the TUI claim a setting the daemon never took.
/// The daemon's value is the only value, here and in the transcript.
async fn write_app_config_key(
    key: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = crucible_daemon::DaemonClient::connect()
        .await
        .map_err(|e| format!("daemon connect failed: {e}"))?;

    let written = client
        .call(
            "config.set",
            serde_json::json!({ "values": { key: value } }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let refused = written
        .get("rejected")
        .and_then(|r| r.as_array())
        .is_some_and(|keys| keys.iter().any(|k| k.as_str() == Some(key)));
    if refused {
        return Err(format!(
            "'{key}' names where the daemon acts; change it in the config file"
        ));
    }

    let read_back = client
        .call("config.get", serde_json::json!({ "key": key }))
        .await
        .map_err(|e| e.to_string())?;
    match read_back.get("value") {
        Some(serde_json::Value::Null) | None => {
            Err(format!("the daemon config store did not keep '{key}'"))
        }
        Some(value) => Ok(value.clone()),
    }
}

/// Read one app-config key back out of the daemon store, with its provenance
/// when the caller asked for the history spelling.
///
/// The daemon is the only reader here on purpose: `:set key?` used to answer
/// from a TUI overlay that never held app config, so every key `init.lua`
/// wrote read back as "not set".
async fn read_app_config_key(
    key: &str,
    history: bool,
) -> Result<(serde_json::Value, Option<serde_json::Value>), String> {
    let client = crucible_daemon::DaemonClient::connect()
        .await
        .map_err(|e| format!("daemon connect failed: {e}"))?;

    // Same lookup as the write's read-back, so `:set k=v` and `:set k?`
    // cannot disagree about which key they named.
    let read = client
        .call("config.get", serde_json::json!({ "key": key }))
        .await
        .map_err(|e| e.to_string())?;
    let value = read
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    if !history {
        return Ok((value, None));
    }
    let origin = client
        .call("config.origin", serde_json::json!({ "key": key }))
        .await
        .map_err(|e| e.to_string())?;
    Ok((value, Some(origin)))
}

/// Drop config layers for one app-config key through the daemon, and report
/// what the store then holds.
///
/// [`DropKind`] picks the verb: `config.reset` drops the ephemeral layer a
/// `:set` writes, `config.pop` drops the highest layer holding the leaf, and
/// `config.unset` removes the key. The daemon's row is the whole answer — the
/// value, its origin, and the layers that went — because the TUI keeps no
/// copy of app config to update.
async fn drop_app_config_key(
    key: &str,
    kind: DropKind,
) -> Result<(Vec<String>, serde_json::Value, serde_json::Value), String> {
    let client = crucible_daemon::DaemonClient::connect()
        .await
        .map_err(|e| format!("daemon connect failed: {e}"))?;

    let method = kind.method();
    let row = client
        .call(method, serde_json::json!({ "key": key }))
        .await
        .map_err(|e| e.to_string())?;

    // The store withholds the keys that name where the daemon acts from every
    // write door, and a drop is a write door. Reporting it as "nothing to
    // drop" would read as a key with no layers rather than a key this socket
    // may not touch.
    if row.get("outcome").and_then(|o| o.as_str()) == Some("withheld") {
        return Err(format!(
            "'{key}' names where the daemon acts; change it in the config file"
        ));
    }
    let dropped = row
        .get("dropped")
        .and_then(|d| d.as_array())
        .map(|sources| {
            sources
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let value = row.get("value").cloned().unwrap_or(serde_json::Value::Null);
    Ok((dropped, value, row))
}

impl OilChatRunner {
    /// Spawn the daemon RPC behind `FetchModels` as a background task.
    ///
    /// The reducer only flips `model_list_state` to Loading; this is the
    /// half that actually resolves it (ModelsLoaded / ModelsFetchFailed).
    /// Shared by the `:model` action path and the startup prefetch — a
    /// Loading transition without a matching spawn wedges the picker
    /// forever, since nothing re-fetches while Loading.
    ///
    /// Uses a fresh DaemonClient connection (same pattern as plugin reload)
    /// to avoid blocking the event loop.
    pub(super) fn spawn_model_fetch(
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<tokio::task::JoinHandle<()>>,
    ) {
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "background: FetchModels starting");
            match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => match client.list_all_models(None).await {
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
                        let _ = tx.send(ChatAppMsg::ModelsFetchFailed(format!(
                            "Failed to list models: {}",
                            e
                        )));
                    }
                },
                Err(e) => {
                    let _ = tx.send(ChatAppMsg::ModelsFetchFailed(format!(
                        "Failed to connect to daemon: {}",
                        e
                    )));
                }
            }
        }));
    }

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
        _agent: &mut A,
        _bridge: &AgentEventBridge,
        _is_replay: bool,
    ) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(_) => {
                // Pure display signal. Live typed messages send via
                // `process_action`; messages that reach the drain are either
                // replay/resume history (already executed on the daemon) or
                // the daemon's own broadcast feedback. Re-sending here made
                // *resume* re-run every historical prompt (is_replay is false
                // on a live resume), so this arm must never call the agent.
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
            //
            // CAUTION: follow-up Sends re-enter process_message, NOT
            // process_action — side effects that only exist in
            // process_action arms (daemon RPCs, spawns) are dropped here.
            // A reducer that returns Action::Send for a side-effectful
            // variant must not rely on this loop to execute it (this is
            // how the FetchModels prefetch and --set startup overrides
            // silently broke; both now run their effects directly).
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
                        match params.agent.undo(count).await {
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
                        // Handled in-arm (reset already applied); don't fall
                        // through to the on_message dispatch, which resets again.
                        return Ok(false);
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
                        match SessionKnobs::switch_model(params.agent, model_id).await {
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
                        Self::spawn_model_fetch(params.msg_tx, params.background_tasks);
                    }
                    ChatAppMsg::FetchModes if !self.is_replay => {
                        // Goes through the agent handle rather than a fresh
                        // DaemonClient (the model prefetch's pattern) because
                        // the mode list is per-session: it is resolved from
                        // the session's Lua registry, and two sessions in
                        // different projects can offer different modes.
                        let modes = params.agent.fetch_available_modes().await;
                        if !modes.is_empty() {
                            params.app.on_message(ChatAppMsg::ModesLoaded(modes));
                        }
                    }
                    // The daemon just named a mode our list does not have — it
                    // was declared after our one startup fetch. Refresh rather
                    // than leave it uncyclable. Applying the message itself is
                    // the trailing `on_message` dispatch's job.
                    ChatAppMsg::ModeSynced(ref mode_id) if !params.app.knows_mode(mode_id) => {
                        let _ = params.msg_tx.send(ChatAppMsg::FetchModes);
                    }
                    ChatAppMsg::PluginStatusLoaded(_) => {
                        params.app.on_message(msg.clone());
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
                    ChatAppMsg::SetPrecognition(enabled) => {
                        tracing::info!(precognition = enabled, "Setting precognition");
                        match params.agent.set_precognition(*enabled).await {
                            Ok(()) => {
                                tracing::info!(
                                    precognition = enabled,
                                    "Precognition set successfully"
                                );
                                params.app.set_precognition(*enabled);
                            }
                            Err(e) => {
                                // Revert the optimistic local flag: the `:set`
                                // readout must not claim a state the daemon
                                // refused.
                                tracing::warn!(precognition = enabled, error = %e, "set_precognition failed");
                                params.app.set_precognition(!*enabled);
                                params.app.add_notification(
                                    crucible_core::types::Notification::warning(format!(
                                        "Set precognition failed: {}",
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
                        // The badge was set optimistically before we got here.
                        // A warn-only failure left it claiming a mode the
                        // daemon refused — the exact "the UI says one thing,
                        // the agent does another" state this whole area is
                        // about. Revert to what the daemon reports and say so.
                        if let Err(e) = params.agent.set_mode_str(mode_id).await {
                            tracing::warn!(mode = %mode_id, error = %e, "Failed to set mode on agent");
                            // Queued, not applied here: `process_action` calls
                            // `on_message(msg)` after this match, which would
                            // re-apply the optimistic `ModeChanged` over the
                            // top of a direct revert.
                            let actual = params.agent.get_mode_id().to_string();
                            let _ = params.msg_tx.send(ChatAppMsg::ModeSynced(actual));
                            let _ = params.msg_tx.send(ChatAppMsg::Error(format!("mode: {e}")));
                            // We offered a mode the daemon rejects, so the list
                            // we offered it from is stale. `FetchModes` fires
                            // once at startup, which is why a mode declared
                            // later never appeared — and one removed later
                            // stayed on offer.
                            let _ = params.msg_tx.send(ChatAppMsg::FetchModes);
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
                    // Gated on `!self.is_replay`: an app-config `:set` is a
                    // write to the daemon store, and the store's own answer
                    // is what comes back. Nothing here is best-effort: a
                    // failure produces an error and no value at all, because
                    // the TUI holds no second copy to fall back on.
                    ChatAppMsg::ConfigSet { ref key, ref value } if !self.is_replay => {
                        let key = key.clone();
                        let value = value.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match write_app_config_key(&key, value).await {
                                Ok(value) => ChatAppMsg::ConfigSetResolved { key, value },
                                Err(e) => {
                                    tracing::warn!(key = %key, error = %e, "config.set failed");
                                    ChatAppMsg::Error(format!("set {}: {}", key, e))
                                }
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // Gated on `!self.is_replay`: a `:set key?` on an
                    // app-config key is a daemon read, and the daemon's
                    // answer is the only answer — the TUI holds no copy of
                    // app config to fall back on.
                    ChatAppMsg::ConfigQuery { ref key, history } if !self.is_replay => {
                        let key = key.clone();
                        let history = *history;
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match read_app_config_key(&key, history).await {
                                Ok((value, origin)) => {
                                    ChatAppMsg::ConfigQueryResolved { key, value, origin }
                                }
                                Err(e) => {
                                    tracing::warn!(key = %key, error = %e, "config.get failed");
                                    ChatAppMsg::Error(format!("set {}?: {}", key, e))
                                }
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // Gated on `!self.is_replay`: a `:set key&` or `:set
                    // key^` on an app-config key CHANGES the daemon store, so
                    // replaying a transcript must not re-drop the layers.
                    ChatAppMsg::ConfigDrop { ref key, kind } if !self.is_replay => {
                        let key = key.clone();
                        let kind = *kind;
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match drop_app_config_key(&key, kind).await {
                                Ok((dropped, value, origin)) => ChatAppMsg::ConfigDropResolved {
                                    key,
                                    dropped,
                                    value,
                                    origin,
                                },
                                Err(e) => {
                                    tracing::warn!(key = %key, ?kind, error = %e, "a config drop failed");
                                    ChatAppMsg::Error(format!(
                                        "set {key}{}: {e}",
                                        kind.spelling()
                                    ))
                                }
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // Gated on `!self.is_replay`: opens a fresh
                    // `DaemonClient::connect()` and must not fire during replay.
                    ChatAppMsg::EvalLua(ref code) if !self.is_replay => {
                        let code = code.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let evaled = match crucible_daemon::DaemonClient::connect().await {
                                Ok(client) => client
                                    .call("lua.eval", serde_json::json!({ "code": code }))
                                    .await
                                    .map(|resp| {
                                        resp.get("result")
                                            .and_then(|r| r.as_str())
                                            .unwrap_or("nil")
                                            .to_string()
                                    })
                                    .map_err(|e| {
                                        // Surface just the daemon's message, not
                                        // the raw `RPC error: {json}` wrapper.
                                        let raw = e.to_string();
                                        raw.strip_prefix("RPC error: ")
                                            .and_then(|j| {
                                                serde_json::from_str::<serde_json::Value>(j).ok()
                                            })
                                            .and_then(|v| {
                                                v.get("message")
                                                    .and_then(|m| m.as_str())
                                                    .map(String::from)
                                            })
                                            .unwrap_or(raw)
                                    }),
                                Err(e) => Err(format!("daemon connect failed: {}", e)),
                            };
                            let msg = match evaled {
                                Ok(output) => ChatAppMsg::LuaEvaled {
                                    output,
                                    is_error: false,
                                },
                                Err(output) => ChatAppMsg::LuaEvaled {
                                    output,
                                    is_error: true,
                                },
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // Same replay gate as the reload below: this opens its own
                    // `DaemonClient::connect()`, and a replay must reach no daemon.
                    ChatAppMsg::OpenSurface(ref name) if !self.is_replay => {
                        let name = name.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            match crucible_daemon::DaemonClient::connect().await {
                                Ok(client) => {
                                    match fetch_surface(&client, name.as_deref(), true).await {
                                        Ok(Some(msg)) => {
                                            let _ = tx.send(msg);
                                        }
                                        Ok(None) => {
                                            let _ = tx.send(ChatAppMsg::Status(
                                                "No plugin surfaces declared".to_string(),
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(ChatAppMsg::Error(format!(
                                                "Surface fetch failed: {e}"
                                            )));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(ChatAppMsg::Error(format!(
                                        "Daemon unreachable: {e}"
                                    )));
                                }
                            }
                        }));
                    }
                    // A `surface_changed` refetch. Same replay gate, and
                    // `open_if_closed = false`: this must refresh what is open and
                    // never open anything.
                    ChatAppMsg::RefreshSurface(ref name) if !self.is_replay => {
                        let name = name.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            // A daemon this client cannot reach says nothing about
                            // the surface, so the refetch reports no outcome.
                            let Ok(client) = crucible_daemon::DaemonClient::connect().await else {
                                return;
                            };
                            let fetched = fetch_surface(&client, Some(&name), false).await;
                            if let Some(msg) = refresh_outcome(&name, fetched) {
                                let _ = tx.send(msg);
                            }
                        }));
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
                    // Gated on `!self.is_replay`: runs the command in the
                    // daemon's plugin registry. The result renders as a
                    // system message — an invocation, not a chat turn.
                    ChatAppMsg::RunPluginCommand { ref name, ref args } if !self.is_replay => {
                        tracing::info!(command = %name, "Running plugin command");
                        let name = name.clone();
                        let args = if args.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!({ "input": args })
                        };
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            match crucible_daemon::DaemonClient::connect().await {
                                Ok(client) => match client.plugin_run_command(&name, args).await {
                                    Ok(result) => {
                                        let rendered = match result.get("result") {
                                            Some(serde_json::Value::String(s)) => s.clone(),
                                            Some(other) => serde_json::to_string_pretty(other)
                                                .unwrap_or_else(|_| other.to_string()),
                                            None => "(no result)".to_string(),
                                        };
                                        let _ = tx.send(ChatAppMsg::Status(format!(
                                            "/{name}: {rendered}"
                                        )));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(ChatAppMsg::Error(format!(
                                            "/{name} failed: {e}"
                                        )));
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.send(ChatAppMsg::Error(format!(
                                        "Cannot connect to daemon: {e}"
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
                    | ChatAppMsg::OpenSurface(_)
                    | ChatAppMsg::RefreshSurface(_)
                    | ChatAppMsg::EvalLua(_)
                    | ChatAppMsg::ConfigSet { .. }
                    | ChatAppMsg::ConfigQuery { .. }
                    | ChatAppMsg::ConfigDrop { .. }
                    | ChatAppMsg::ExecuteSlashCommand(_)
                    | ChatAppMsg::RunPluginCommand { .. }
                    | ChatAppMsg::ExportSession(_)
                    | ChatAppMsg::FetchModels
                        if self.is_replay =>
                    {
                        tracing::debug!(?msg, "daemon-bound message ignored in replay mode");
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

/// What a background refetch of `name` tells the app.
///
/// The two empty answers mean different things, and this function is where the
/// difference lives:
///
/// - `Ok(None)`: the daemon answered, and the answer is that the surface is
///   absent. A plugin uninstall does this. The app must stop drawing the panel,
///   so this reports a withdrawal.
/// - `Err(_)`: the refetch itself failed. A daemon under load, or one that is
///   briefly unreachable, produces this. The surface may still exist, so this
///   reports nothing and the open panel stays as it is.
///
/// The failure path also stays silent because a background refresh is work the
/// user did not ask for. A warning about it is noise the user cannot act on.
pub(super) fn refresh_outcome(
    name: &str,
    fetched: anyhow::Result<Option<ChatAppMsg>>,
) -> Option<ChatAppMsg> {
    match fetched {
        Ok(Some(msg)) => Some(msg),
        Ok(None) => Some(ChatAppMsg::SurfaceWithdrawn(name.to_string())),
        Err(_) => None,
    }
}

/// Fetch one surface for the modal, or the first declared when none is named.
///
/// Returns `Ok(None)` when no surface exists at all, which is a fact to report
/// rather than an error: a user asking to see surfaces before any plugin
/// declares one has done nothing wrong.
async fn fetch_surface(
    client: &crucible_daemon::DaemonClient,
    name: Option<&str>,
    open_if_closed: bool,
) -> anyhow::Result<Option<ChatAppMsg>> {
    let value = match name {
        Some(name) => client
            .call("surface.get", serde_json::json!({ "name": name }))
            .await?["surface"]
            .clone(),
        // No name: take the first of the list, so `:surfaces` shows something
        // instead of asking the user to know a name they have not been told.
        None => client
            .call("surface.list", serde_json::json!({}))
            .await?
            .get("surfaces")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    };

    if value.is_null() {
        return Ok(None);
    }

    let rows = value
        .get("rows")
        .and_then(|r| r.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| crate::tui::oil::components::SurfaceModalRow {
                    id: row
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    text: row
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    detail: row
                        .get("detail")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    mark: row.get("mark").and_then(|v| v.as_str()).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(ChatAppMsg::SurfaceLoaded {
        // The daemon's own name for the surface, not the argument. `:surfaces`
        // with no argument names nothing, and the modal still needs the name to
        // compare a later withdrawal against.
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        title: value
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Surface")
            .to_string(),
        rows,
        version: value.get("version").and_then(|v| v.as_u64()).unwrap_or(0),
        open_if_closed,
    }))
}
