use crate::session::LiveSession;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::commands::DropKind;
use crate::tui::oil::commands::PLUGIN_APPROVAL;
use crucible_core::protocol::RpcMethod;
use std::io;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{DrainMessagesOutcome, EventLoopParams, OilChatRunner, ProcessActionParams};

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
            RpcMethod::ConfigSet,
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
        .call(RpcMethod::ConfigGet, serde_json::json!({ "key": key }))
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
        .call(RpcMethod::ConfigGet, serde_json::json!({ "key": key }))
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
        .call(RpcMethod::ConfigOrigin, serde_json::json!({ "key": key }))
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
        background_tasks: &mut Vec<JoinHandle<()>>,
        session_models_source: Option<String>,
    ) {
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "background: FetchModels starting");
            match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => {
                    // An ACP agent's models are its own advertisement,
                    // answered per session (`session.list_models`); the
                    // all-providers catalogue would offer models the agent
                    // would reject. Internal agents have no session list —
                    // the catalogue is their answer.
                    let fetched = match session_models_source.as_deref() {
                        Some(session_id) => client.session_list_models(session_id).await,
                        None => client.list_all_models(None).await,
                    };
                    match fetched {
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
                    }
                }
                Err(e) => {
                    let _ = tx.send(ChatAppMsg::ModelsFetchFailed(format!(
                        "Failed to connect to daemon: {}",
                        e
                    )));
                }
            }
        }));
    }

    /// The message of a status read: the list, or a warning when the read
    /// fails or an item does not decode. A list that is shorter without a
    /// word could hide an `ask` or a `stop` (rule 7).
    pub(super) fn status_items_msg(
        read: anyhow::Result<Vec<crucible_core::types::StatusDisplayItem>>,
    ) -> ChatAppMsg {
        match read {
            Ok(items) => ChatAppMsg::StatusItemsLoaded(items),
            Err(e) => ChatAppMsg::Error(format!("The status list is not available: {e:#}")),
        }
    }

    /// Read the first status list of the session.
    ///
    /// The caller starts this after the session's events flow, so a change
    /// between the read and the subscription reaches the TUI as an event.
    /// A session without an id has no daemon session and no status list.
    pub(super) fn spawn_status_fetch(
        session_id: Option<String>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<JoinHandle<()>>,
    ) {
        let Some(session_id) = session_id else {
            return;
        };
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            let read = match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => client.session_status_items(&session_id).await,
                Err(e) => Err(e),
            };
            let _ = tx.send(Self::status_items_msg(read));
        }));
    }

    /// The messages of the first read of the session's notifications: one
    /// per notification, oldest first, or a warning when the read fails.
    pub(super) async fn notification_msgs(
        client: &crucible_daemon::DaemonClient,
        session_id: &str,
    ) -> Vec<ChatAppMsg> {
        match client.session_list_notifications(session_id).await {
            // The hub lists the newest first.
            Ok(list) => list
                .into_iter()
                .rev()
                .map(ChatAppMsg::Notification)
                .collect(),
            Err(e) => vec![ChatAppMsg::Error(format!(
                "The notifications of the session are not available: {e:#}"
            ))],
        }
    }

    /// Read the session's notifications once, after its events flow, so a
    /// notification that arrives after the read comes as an event.
    pub(super) fn spawn_notification_fetch(
        session_id: Option<String>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<JoinHandle<()>>,
    ) {
        let Some(session_id) = session_id else {
            return;
        };
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            let msgs = match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => Self::notification_msgs(&client, &session_id).await,
                Err(e) => vec![ChatAppMsg::Error(format!(
                    "The notifications of the session are not available: {e:#}"
                ))],
            };
            for msg in msgs {
                let _ = tx.send(msg);
            }
        }));
    }

    /// Close each notification `ids` for the session in the daemon. Returns
    /// one warning for each close that failed.
    pub(super) async fn close_notification_msgs(
        client: &crucible_daemon::DaemonClient,
        session_id: &str,
        ids: &[String],
    ) -> Vec<ChatAppMsg> {
        let mut msgs = Vec::new();
        for id in ids {
            if let Err(e) = client.session_dismiss_notification(session_id, id).await {
                msgs.push(ChatAppMsg::Error(format!(
                    "The daemon could not close a notification: {e:#}"
                )));
            }
        }
        msgs
    }

    /// Close the notifications `ids` of the session in the daemon, so they
    /// do not come back when a client attaches again.
    fn spawn_notification_close(
        session_id: String,
        ids: Vec<String>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<JoinHandle<()>>,
    ) {
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            let msgs = match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => Self::close_notification_msgs(&client, &session_id, &ids).await,
                Err(e) => vec![ChatAppMsg::Error(format!(
                    "The daemon could not close a notification: {e:#}"
                ))],
            };
            for msg in msgs {
                let _ = tx.send(msg);
            }
        }));
    }

    /// Read the proposals in the Inbox for the count and the `:proposals`
    /// view.
    ///
    /// A fetch that the user asked for (`open`) reports its failure. A
    /// refresh after `proposal_changed` stays silent, because the user did
    /// not ask for it and cannot act on the warning.
    pub(super) fn spawn_proposal_fetch(
        open: bool,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<JoinHandle<()>>,
    ) {
        let tx = msg_tx.clone();
        background_tasks.push(tokio::spawn(async move {
            let fetched = match crucible_daemon::DaemonClient::connect().await {
                Ok(client) => client.proposal_list(false).await,
                Err(e) => Err(e),
            };
            let msg = match fetched {
                Ok(proposals) => Some(ChatAppMsg::ProposalsLoaded { proposals, open }),
                Err(e) if open => Some(ChatAppMsg::Error(format!("Proposals failed: {e:#}"))),
                Err(e) => {
                    tracing::debug!(error = %e, "proposal count refresh failed");
                    None
                }
            };
            if let Some(msg) = msg {
                let _ = tx.send(msg);
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
    pub(super) fn process_message(msg: &ChatAppMsg, app: &mut OilChatApp) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(_) => {
                // Pure display signal. Live typed messages send via
                // `process_action`; messages that reach the drain are either
                // replay/resume history (already executed on the daemon) or
                // the daemon's own broadcast feedback. Re-sending here made
                // *resume* re-run every historical prompt (is_replay is false
                // on a live resume), so this arm must never call the daemon.
            }
            ChatAppMsg::FetchModels => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "drain_pending_messages: received FetchModels");
            }
            _ => {}
        }
        app.on_message(msg.clone())
    }

    pub(super) async fn drain_pending_messages(
        &mut self,
        params: &mut EventLoopParams<'_>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> io::Result<DrainMessagesOutcome> {
        let app: &mut OilChatApp = params.app;
        let session = params.session;
        let msg_tx = &params.msg_tx;
        let msg_rx = &mut params.msg_rx;
        let background_tasks: &mut Vec<JoinHandle<()>> = params.background_tasks;
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

            // The drained message itself goes to the reducer only. It is an
            // event or a replay record, so it must not start its own effect
            // again (a resume would send every old prompt once more).
            //
            // The follow-up of the reducer is new intent, as a key press is.
            // Thus it goes to `process_action`, which starts its daemon read
            // and then gives it to the reducer. The replay gates of
            // `process_action` keep a replay away from the daemon.
            let action = Self::process_message(&msg, app);
            let quit = self
                .process_action(ProcessActionParams {
                    action,
                    app: &mut *app,
                    session,
                    msg_tx,
                    background_tasks: &mut *background_tasks,
                })
                .await?;
            if quit {
                return Ok(DrainMessagesOutcome::Quit);
            }
        }

        Ok(if processed_any {
            DrainMessagesOutcome::Processed
        } else {
            DrainMessagesOutcome::Idle
        })
    }

    pub(super) fn should_wait_for_event(outcome: DrainMessagesOutcome) -> bool {
        matches!(outcome, DrainMessagesOutcome::Idle)
    }

    pub(super) async fn process_action(
        &mut self,
        params: ProcessActionParams<'_>,
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
                        let Some(session) = params.session else {
                            params
                                .app
                                .add_notification(crucible_core::types::Notification::toast(
                                    "Nothing to undo".to_string(),
                                ));
                            return Ok(false);
                        };
                        match session.client.session_undo(&session.id, count).await {
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
                                        crucible_daemon::rpc_error_message(&e)
                                    )),
                                );
                            }
                        }
                        return Ok(false);
                    }
                    ChatAppMsg::StreamCancelled => {
                        if let (true, Some(session)) = (params.app.is_streaming(), params.session) {
                            if let Err(e) = session.client.session_cancel(&session.id).await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream on daemon");
                            }
                            tracing::info!("Cancelled active turn via session.cancel RPC");
                        }
                    }
                    ChatAppMsg::SwitchModel(model_id) => {
                        tracing::info!(model = %model_id, "Model switch requested");
                        let switched = match params.session {
                            Some(session) => {
                                session
                                    .client
                                    .session_switch_model(&session.id, model_id)
                                    .await
                            }
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        match switched {
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
                        // Session-scoped: an ACP agent's list is its own
                        // advertisement (`session.list_models` brings the agent up
                        // if it is not yet connected); an internal session answers
                        // the catalogue narrowed by its classification.
                        Self::spawn_model_fetch(
                            params.msg_tx,
                            params.background_tasks,
                            params.session.map(|s| s.id.clone()),
                        );
                    }
                    // The mode list is per session: the daemon resolves it
                    // from the session's Lua registry, and two sessions in
                    // different projects can offer different modes.
                    ChatAppMsg::FetchModes if !self.is_replay => {
                        if let Some(session) = params.session {
                            match session.client.session_list_modes(&session.id).await {
                                Ok(state) if !state.modes.is_empty() => {
                                    params.app.on_message(ChatAppMsg::ModesLoaded(state.modes));
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to fetch modes from daemon");
                                }
                            }
                        }
                    }
                    // The daemon just named a mode our list does not have — it
                    // was declared after our one startup fetch. Refresh rather
                    // than leave it uncyclable. Applying the message itself is
                    // the trailing `on_message` dispatch's job.
                    ChatAppMsg::ModeSynced(ref mode_id) if !params.app.knows_mode(mode_id) => {
                        let _ = params.msg_tx.send(ChatAppMsg::FetchModes);
                        // Each mode is also a command of the catalog.
                        let _ = params.msg_tx.send(ChatAppMsg::FetchCommands);
                    }
                    ChatAppMsg::PluginStatusLoaded(_) => {
                        params.app.on_message(msg.clone());
                    }
                    ChatAppMsg::SetContextStrategy(strategy_str) => {
                        tracing::info!(context_strategy = %strategy_str, "Setting context_strategy");
                        match strategy_str.parse::<crucible_core::session::ContextStrategy>() {
                            Ok(strategy) => {
                                let set = match params.session {
                                    Some(session) => session
                                        .client
                                        .session_set_context_strategy(
                                            &session.id,
                                            &strategy.to_string(),
                                        )
                                        .await
                                        .map(|_| ()),
                                    None => Err(anyhow::anyhow!("no live session")),
                                };
                                match set {
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
                        let set = match params.session {
                            Some(session) => session
                                .client
                                .session_set_precognition(&session.id, *enabled)
                                .await
                                .map(|_| ()),
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        match set {
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
                    ChatAppMsg::SetPluginTurnLimit(limit) => {
                        let set = match params.session {
                            Some(session) => session
                                .client
                                .session_set_plugin_turn_limit(&session.id, *limit)
                                .await
                                .map(|_| ()),
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        if let Err(error) = set {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(format!(
                                    "Set plugin turn limit failed: {error}"
                                )),
                            );
                        }
                    }
                    // The line shows the value that the daemon holds after the
                    // change, read back from the daemon. The client keeps no
                    // copy that could disagree with another client.
                    ChatAppMsg::PluginApproval { plugin, set } => {
                        let shown = match params.session {
                            Some(session) => plugin_approval_after(session, plugin, *set).await,
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        match shown {
                            Ok(approval) => params.app.add_system_message(format!(
                                "  {PLUGIN_APPROVAL}{plugin}={}",
                                approval.as_str()
                            )),
                            Err(error) => params.app.add_notification(
                                crucible_core::types::Notification::warning(format!(
                                    "Set plugin approval failed: {error}"
                                )),
                            ),
                        }
                    }
                    ChatAppMsg::CloseInteraction {
                        request_id,
                        response,
                    } => {
                        tracing::info!(request_id = %request_id, "Sending interaction response");
                        let responded = match params.session {
                            Some(session) => {
                                session
                                    .client
                                    .session_interaction_respond(
                                        &session.id,
                                        request_id,
                                        response.clone(),
                                    )
                                    .await
                            }
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        match responded {
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
                        let set = match params.session {
                            Some(session) => session
                                .client
                                .session_set_mode(&session.id, mode_id)
                                .await
                                .map(|_| ()),
                            None => Err(anyhow::anyhow!("no live session")),
                        };
                        if let Err(e) = set {
                            tracing::warn!(mode = %mode_id, error = %e, "Failed to set mode on the session");
                            // Queued, not applied here: `process_action` calls
                            // `on_message(msg)` after this match, which would
                            // re-apply the optimistic `ModeChanged` over the
                            // top of a direct revert. The mode to go back to
                            // is the one that the daemon reports.
                            if let Some(session) = params.session {
                                if let Ok(state) =
                                    session.client.session_list_modes(&session.id).await
                                {
                                    let _ = params
                                        .msg_tx
                                        .send(ChatAppMsg::ModeSynced(state.current_mode_id));
                                }
                            }
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
                        if let (false, Some(session)) = (self.is_replay, params.session) {
                            // The input line sends a `/` line as a slash
                            // command, so a user message names no command.
                            if let Err(e) = send_user_message(session, content).await {
                                tracing::warn!(error = %e, "session.send_message failed");
                                // The daemon refuses a message that names an
                                // unknown or resolved comment with
                                // `@comment:<id>`. A log line only told the
                                // user nothing, and the transcript kept a
                                // turn that never ran.
                                let _ = params.msg_tx.send(ChatAppMsg::Error(format!("send: {e}")));
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
                                    .call(RpcMethod::LuaEval, serde_json::json!({ "code": code }))
                                    .await
                                    .map(|resp| {
                                        resp.get("result")
                                            .and_then(|r| r.as_str())
                                            .unwrap_or("nil")
                                            .to_string()
                                    })
                                    .map_err(|e| crucible_daemon::rpc_error_message(&e)),
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
                    // `/resume`. Same replay gate: a replay must reach no daemon.
                    ChatAppMsg::FetchSessions if !self.is_replay => {
                        let current = params.session.map(|s| s.id.clone());
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match fetch_resumable_sessions(current.as_deref()).await {
                                Ok(sessions) => ChatAppMsg::SessionsLoaded(sessions),
                                Err(e) => ChatAppMsg::SessionsFetchFailed(format!("{e:#}")),
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // `/resume <id>`: stop this run, and let the caller open
                    // the session through the `--resume` path. The daemon must
                    // know the id first, because the switch leaves this session.
                    ChatAppMsg::ResumeSession(ref id) if !self.is_replay => {
                        if params.session.is_some_and(|s| &s.id == id) {
                            params
                                .app
                                .add_notification(crucible_core::types::Notification::toast(
                                    format!("This console already shows session {id}"),
                                ));
                            return Ok(false);
                        }
                        if params.app.is_streaming() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Cannot resume another session while a turn runs".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        match check_session_exists(id).await {
                            Ok(()) => {
                                self.next_session = Some(id.clone());
                                return Ok(true);
                            }
                            Err(e) => {
                                params.app.on_message(ChatAppMsg::Error(format!(
                                    "Cannot resume {id}: {e:#}"
                                )));
                                return Ok(false);
                            }
                        }
                    }
                    // `:diff`. Same replay gate: a replay must reach no daemon.
                    // The root is the git top level of the workspace, and the
                    // daemon admits it or refuses it.
                    ChatAppMsg::OpenDiff(ref base) if !self.is_replay => {
                        let base = base.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match fetch_branch_diff(base.as_deref()).await {
                                Ok(diffset) => ChatAppMsg::DiffLoaded(Box::new(diffset)),
                                Err(e) => ChatAppMsg::Error(format!("Diff failed: {e:#}")),
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    ChatAppMsg::FetchDiffFile(ref request) if !self.is_replay => {
                        let request = request.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match fetch_diff_file(&request).await {
                                Ok(text) => ChatAppMsg::DiffFileLoaded {
                                    id: request.id,
                                    index: request.index,
                                    text,
                                },
                                Err(e) => ChatAppMsg::Error(format!(
                                    "Diff of {} failed: {e:#}",
                                    request.path
                                )),
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // The `:plugin-mode` menu reads the daemon's list: each
                    // plugin that starts turns, with the value that the
                    // session holds. The menu opens when it arrives.
                    ChatAppMsg::FetchPluginApprovals if !self.is_replay => {
                        let session_id = params.session.map(|s| s.id.clone());
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let fetched = match session_id {
                                Some(id) => match crucible_daemon::DaemonClient::connect().await {
                                    Ok(client) => client.session_list_plugin_approvals(&id).await,
                                    Err(e) => Err(e),
                                },
                                None => Err(anyhow::anyhow!("no session")),
                            };
                            let msg = match fetched {
                                Ok(approvals) => ChatAppMsg::PluginApprovalsLoaded(
                                    approvals.into_iter().collect(),
                                ),
                                Err(e) => {
                                    ChatAppMsg::Error(format!("Plugin approvals failed: {e:#}"))
                                }
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // `:proposals`. Same replay gate: a replay must reach no daemon.
                    ChatAppMsg::FetchProposals { open } if !self.is_replay => {
                        Self::spawn_proposal_fetch(*open, params.msg_tx, params.background_tasks);
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
                                                let _ = tx.send(ChatAppMsg::Status(format!(
                                                    "✓ Reloaded '{}' ({} tools, {} services)",
                                                    name, result.tools, result.services
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
                                    // A reload can add or drop plugin commands.
                                    let _ = tx.send(ChatAppMsg::FetchCommands);
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
                    // The daemon's context_cleared draws the divider.
                    ChatAppMsg::ClearContext if !self.is_replay => {
                        let session_id = params.session.map(|s| s.id.clone());
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let result = match session_id {
                                Some(id) => match crucible_daemon::DaemonClient::connect().await {
                                    Ok(client) => client.session_clear(&id).await,
                                    Err(e) => Err(e),
                                },
                                None => Err(anyhow::anyhow!("no session")),
                            };
                            if let Err(e) = result {
                                let _ = tx.send(ChatAppMsg::Error(format!("/clear failed: {e}")));
                            }
                        }));
                    }
                    // Gated on `!self.is_replay`: the daemon holds the
                    // session logs. The scope is the session's whole kiln set.
                    ChatAppMsg::SearchSessions(ref query) if !self.is_replay => {
                        if let Some(session) = params.session {
                            let client = std::sync::Arc::clone(&session.client);
                            let session_id = session.id.clone();
                            let query = query.clone();
                            let tx = params.msg_tx.clone();
                            params.background_tasks.push(tokio::spawn(async move {
                                let msg = match search_sessions(&client, &session_id, &query).await
                                {
                                    Ok(text) => ChatAppMsg::SystemNotice(text),
                                    Err(e) => ChatAppMsg::Error(format!(
                                        "/search failed: {}",
                                        crucible_daemon::rpc_error_message(&e)
                                    )),
                                };
                                let _ = tx.send(msg);
                            }));
                        }
                    }
                    // Gated on `!self.is_replay`: slash commands forward to the
                    // agent (and thus the daemon). Defense-in-depth — user
                    // keystrokes during replay must not hit the daemon.
                    ChatAppMsg::ExecuteSlashCommand(ref cmd) if !self.is_replay => {
                        tracing::info!(command = %cmd, "Sending slash command to the daemon");
                        if let Some(session) = params.session {
                            match send_user_message(session, cmd).await {
                                Ok(Some(result)) => {
                                    params.app.on_message(ChatAppMsg::SystemNotice(result));
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    params.app.on_message(ChatAppMsg::Error(e));
                                }
                            }
                        }
                    }
                    // The catalog is per session: modes, plugins, skills and
                    // the agent's own commands all differ between sessions.
                    ChatAppMsg::FetchCommands if !self.is_replay => {
                        if let Some(session) = params.session {
                            match session.client.session_commands(&session.id).await {
                                Ok(commands) => {
                                    params.app.on_message(ChatAppMsg::CommandsLoaded(commands));
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to fetch the command catalog");
                                }
                            }
                        }
                    }
                    // Gated on `!self.is_replay`: the daemon holds the
                    // recording of the session, so the daemon writes the
                    // export. During replay there is no live session to export.
                    ChatAppMsg::ExportSession(ref export_path) if !self.is_replay => {
                        let Some(session_id) = params.session.map(|s| s.id.clone()) else {
                            params.app.on_message(ChatAppMsg::Error(
                                "Export failed: no active session".to_string(),
                            ));
                            return Ok(false);
                        };
                        let export_path = export_path.clone();
                        let tx = params.msg_tx.clone();
                        params.background_tasks.push(tokio::spawn(async move {
                            let msg = match export_session(&session_id, &export_path).await {
                                Ok(written) => {
                                    ChatAppMsg::Status(format!("Session exported to {written}"))
                                }
                                Err(e) => ChatAppMsg::Error(format!("Export failed: {e:#}")),
                            };
                            let _ = tx.send(msg);
                        }));
                    }
                    // The user closed daemon notifications. A session without
                    // an id has no daemon notification to close.
                    ChatAppMsg::CloseDaemonNotifications(ref ids) if !self.is_replay => {
                        if let Some(session_id) = params.session.map(|s| s.id.clone()) {
                            Self::spawn_notification_close(
                                session_id,
                                ids.clone(),
                                params.msg_tx,
                                params.background_tasks,
                            );
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
                    | ChatAppMsg::OpenDiff(_)
                    | ChatAppMsg::FetchDiffFile(_)
                    | ChatAppMsg::FetchProposals { .. }
                    | ChatAppMsg::FetchPluginApprovals
                    | ChatAppMsg::FetchSessions
                    | ChatAppMsg::ResumeSession(_)
                    | ChatAppMsg::EvalLua(_)
                    | ChatAppMsg::ConfigSet { .. }
                    | ChatAppMsg::ConfigQuery { .. }
                    | ChatAppMsg::ConfigDrop { .. }
                    | ChatAppMsg::ExecuteSlashCommand(_)
                    | ChatAppMsg::SearchSessions(_)
                    | ChatAppMsg::FetchCommands
                    | ChatAppMsg::ClearContext
                    | ChatAppMsg::ExportSession(_)
                    | ChatAppMsg::CloseDaemonNotifications(_)
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
                    session: params.session,
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
                        session: params.session,
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

/// Send a user message to the session. A turn that follows comes back as
/// session events. A command that the daemon ran without a turn gives its
/// result as `Some` text for the transcript.
///
/// The error is the daemon's own message, not the `RPC error: {json}`
/// envelope: a refused turn names its reason there, and the TUI shows it.
async fn send_user_message(session: &LiveSession, content: &str) -> Result<Option<String>, String> {
    use crucible_core::types::SendOutcome;
    let outcome = session
        .client
        .session_send_message(&session.id, content, true)
        .await
        .map_err(|e| crucible_daemon::rpc_error_message(&e))?;
    Ok(match outcome {
        SendOutcome::Turn { .. } => None,
        SendOutcome::Command { command, result } => {
            Some(format!("/{command}: {}", SendOutcome::result_text(&result)))
        }
    })
}

/// `/search`: the matches in the sessions that share a kiln with this one,
/// as text.
async fn search_sessions(
    client: &crucible_daemon::DaemonClient,
    session_id: &str,
    query: &str,
) -> anyhow::Result<String> {
    let session = client.session_get(session_id).await?;
    let found = client
        .session_search(query, &session.kilns, Some(10))
        .await?;
    Ok(found.to_text(query))
}

/// Apply `set` to the plugin approval of `plugin`, when it is present, and
/// return the value that the daemon holds afterwards.
async fn plugin_approval_after(
    session: &LiveSession,
    plugin: &str,
    set: Option<crucible_core::session::PluginApproval>,
) -> anyhow::Result<crucible_core::session::PluginApproval> {
    if let Some(approval) = set {
        session
            .client
            .session_set_plugin_approval(&session.id, plugin, approval)
            .await?;
    }
    let approvals = session
        .client
        .session_list_plugin_approvals(&session.id)
        .await?;
    Ok(approvals.get(plugin).copied().unwrap_or_default())
}

/// What a background refetch of `name` tells the app.
///
/// The two empty answers mean different things, and this function is where the
/// difference lives:
///
/// - `Ok(None)`: the daemon answered, and the answer is that the surface is
///   absent. The app must stop drawing the panel, so this reports a withdrawal.
/// - `Err(_)`: the refetch itself failed. A daemon under load, or one that is
///   briefly unreachable, produces this. The surface may still exist, so this
///   reports nothing and the open panel stays as it is.
///
/// **`Ok(None)` is no longer how a withdrawal normally arrives.** A
/// `surface_changed` event now carries `withdrawn`, and `system_msgs` turns
/// that straight into [`ChatAppMsg::SurfaceWithdrawn`] without a refetch, so an
/// uninstall never reaches this function. What is left here is the second
/// defence: a surface removed in the window between a change event and its
/// refetch, or a withdrawal event this client never received. Both leave a
/// panel painted for a plugin that is gone, and both answer `Ok(None)`.
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

/// Ask the daemon to write the markdown of `session_id` to `path`, and
/// return the path it wrote.
async fn export_session(session_id: &str, path: &std::path::Path) -> anyhow::Result<String> {
    let client = crucible_daemon::DaemonClient::connect().await?;
    client
        .session_export_to_file(session_id, Some(path), None)
        .await
}

/// The most sessions the `/resume` picker offers.
const RESUME_PICKER_LIMIT: usize = 20;

/// List the sessions that `/resume` offers: the chat sessions that share the
/// kilns and the workspace of the open session.
///
/// The daemon holds both the scope and the list. Without an open session
/// the scope is empty, and the daemon lists what it can see.
async fn fetch_resumable_sessions(
    current: Option<&str>,
) -> anyhow::Result<Vec<crate::tui::oil::chat_app::model_state::SessionChoice>> {
    let client = crucible_daemon::DaemonClient::connect().await?;
    let mut request = crucible_core::protocol::requests::SessionListRequest {
        session_type: Some("chat".to_string()),
        ..Default::default()
    };
    if let Some(current) = current {
        let session = client.session_get(current).await?;
        request.kilns = session.kilns.iter().map(|k| k.to_string()).collect();
        request.workspace = session
            .workspace
            .as_ref()
            .map(|w| w.to_string_lossy().to_string());
    }
    let listed: serde_json::Value = client.typed_call(RpcMethod::SessionList, request).await?;
    Ok(resumable_sessions(
        current.unwrap_or_default(),
        &listed,
        RESUME_PICKER_LIMIT,
    ))
}

/// The picker rows of a `session.list` reply: every session but `current`,
/// the most recent activity first, at most `limit` of them.
pub(super) fn resumable_sessions(
    current: &str,
    listed: &serde_json::Value,
    limit: usize,
) -> Vec<crate::tui::oil::chat_app::model_state::SessionChoice> {
    use chrono::{DateTime, Utc};

    let time = |row: &serde_json::Value, key: &str| {
        row.get(key)
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
    };
    let mut rows: Vec<(Option<DateTime<Utc>>, _)> = listed
        .get("sessions")
        .and_then(|s| s.as_array())
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let id = row.get("session_id")?.as_str()?;
            if id == current {
                return None;
            }
            let active = time(row, "last_activity").or_else(|| time(row, "started_at"));
            let choice = crate::tui::oil::chat_app::model_state::SessionChoice {
                id: id.to_string(),
                title: row
                    .get("title")
                    .and_then(|t| t.as_str())
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_string),
                when: active
                    .map(|t| {
                        t.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "unknown time".to_string()),
            };
            Some((active, choice))
        })
        .collect();
    // Newest first; a session with no time goes last.
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    rows.into_iter().take(limit).map(|(_, c)| c).collect()
}

/// Whether the daemon knows the session `id`, live or stored.
async fn check_session_exists(id: &str) -> anyhow::Result<()> {
    let client = crucible_daemon::DaemonClient::connect().await?;
    client.session_get(id).await?;
    Ok(())
}

/// Compute the branch diff of the workspace against `base`.
///
/// The session's workspace is the working directory of this process. An
/// absent base asks the daemon for the default branch.
async fn fetch_branch_diff(base: Option<&str>) -> anyhow::Result<crucible_core::diff::Diffset> {
    let start = std::env::current_dir()?;
    let source = crate::commands::diff::branch_source(&start, base, None);
    let client = crucible_daemon::DaemonClient::connect().await?;
    client.diff_get(&source).await
}

/// Read the two texts of one file of the open diffset.
async fn fetch_diff_file(
    request: &crate::tui::oil::components::DiffFileRequest,
) -> anyhow::Result<crucible_core::diff::DiffFileText> {
    let client = crucible_daemon::DaemonClient::connect().await?;
    client
        .diff_file(&request.source, &request.path, request.from.as_deref())
        .await
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
            .call(RpcMethod::SurfaceGet, serde_json::json!({ "name": name }))
            .await?["surface"]
            .clone(),
        // No name: take the first of the list, so `:surfaces` shows something
        // instead of asking the user to know a name they have not been told.
        None => client
            .call(RpcMethod::SurfaceList, serde_json::json!({}))
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
