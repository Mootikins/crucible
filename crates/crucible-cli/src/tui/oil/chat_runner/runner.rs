use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay, OilChatApp};
use crate::tui::oil::commands::{SetEffect, SetRpcAction};
use crate::tui::oil::event::Event;
use crate::tui::oil::theme;
use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream};
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::AgentHandle;
use crucible_lua::SessionCommand;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{
    session_event_consumer, DrainMessagesOutcome, DrainPhaseOutcome, EventLoopParams,
    EventLoopSelectOutcome, HandleSelectOutcomeParams, HandleSelectedEventParams, OilChatRunner,
    ProcessActionParams, SessionEventStream,
};

impl OilChatRunner {
    pub async fn run_with_factory<F, Fut, A>(
        &mut self,
        bridge: &AgentEventBridge,
        create_agent: F,
    ) -> Result<()>
    where
        F: Fn(AgentSelection) -> Fut,
        Fut: std::future::Future<
            Output = Result<(
                A,
                Option<mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>>,
            )>,
        >,
        A: AgentHandle,
    {
        self.terminal.enter()?;

        let mut app = OilChatApp::default();
        app.set_mode(self.mode);
        if !self.model.is_empty() {
            app.set_model(std::mem::take(&mut self.model));
        }
        app.set_status("Connecting...");

        if !self.workspace_files.is_empty() {
            app.set_workspace_files(std::mem::take(&mut self.workspace_files));
        }
        if !self.kiln_notes.is_empty() {
            app.set_kiln_notes(std::mem::take(&mut self.kiln_notes));
        }
        if let Some(session_dir) = self.session_dir.take() {
            app.set_session_dir(session_dir);
        }
        if !self.mcp_servers.is_empty() {
            app.set_mcp_servers(std::mem::take(&mut self.mcp_servers));
        }
        if !self.plugin_status.is_empty() {
            let entries = std::mem::take(&mut self.plugin_status);
            for entry in &entries {
                if let Some(ref err) = entry.error {
                    app.add_notification(crucible_core::types::Notification::warning(format!(
                        "Plugin '{}' failed to load: {}",
                        entry.name, err
                    )));
                }
            }
            app.set_plugin_status(entries);
        }
        if !self.available_models.is_empty() {
            app.set_available_models(std::mem::take(&mut self.available_models));
        }
        app.set_show_thinking(self.show_thinking);
        app.set_show_diffs(self.show_diffs);
        if !self.slash_commands.is_empty() {
            app.set_slash_commands(std::mem::take(&mut self.slash_commands));
        }

        let terminal_size = self.terminal.size();
        let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
        let tree = app.view(&ctx);
        self.terminal.render(&tree, "")?;

        let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();
        let mut background_tasks: Vec<JoinHandle<()>> = Vec::new();

        // Hydrate viewport with conversation history from a resumed session by
        // pumping stored events through the shared SessionEventStream — the
        // same path live and replay use. `message_complete` in the stream
        // produces a `StreamComplete` that finalizes the final turn.
        if let Some(events) = self.resume_history.take() {
            if !events.is_empty() {
                tracing::info!(count = events.len(), "Loading resume history into viewport");
                let msg_tx_resume = msg_tx.clone();
                background_tasks.push(tokio::spawn(async move {
                    let mut stream = SessionEventStream::new();
                    for event in events {
                        let event_type = event.get("event").and_then(|e| e.as_str()).unwrap_or("");
                        let data = event.get("data").cloned().unwrap_or_default();
                        for m in stream.translate(event_type, &data) {
                            if msg_tx_resume.send(m).is_err() {
                                return;
                            }
                        }
                    }
                }));
            }
        }

        if let Some(replay_path) = self.replay_path.clone() {
            // Read the recording directly off disk. No daemon contact.
            let (header, events) = crate::tui::oil::local_replay::read_recording(&replay_path)?;

            // Header's terminal_size records the geometry the recording was
            // rendered against. The real terminal's size is whatever the user
            // has right now; we log the recorded size for diagnostics rather
            // than resizing the live terminal.
            if let Some((cols, rows)) = header.terminal_size {
                tracing::info!(
                    recorded_cols = cols,
                    recorded_rows = rows,
                    "Replay recording terminal_size (informational)"
                );
            }
            tracing::info!(
                original_session = %header.session_id,
                started_at = %header.started_at,
                event_count = events.len(),
                speed = self.replay_speed,
                "Replaying recording from disk"
            );

            let replay_session_id = format!(
                "local-replay-{}",
                chrono::Utc::now().format("%Y%m%d-%H%M%S-%f")
            );

            // Local driver pumps RecordedEvents out as SessionEventMessages.
            // An adapter converts them into daemon SessionEvents so we can
            // feed the unified session_event_consumer (Task 2.5).
            let (msg_tx_driver, mut msg_rx_driver) = tokio::sync::mpsc::unbounded_channel::<
                crucible_core::protocol::SessionEventMessage,
            >();
            let (event_tx_adapter, event_rx_adapter) =
                tokio::sync::mpsc::unbounded_channel::<crucible_daemon::SessionEvent>();

            let driver_session_id = replay_session_id.clone();
            let driver_speed = self.replay_speed;
            background_tasks.push(tokio::spawn(async move {
                crate::tui::oil::local_replay::drive_replay(
                    events,
                    driver_speed,
                    driver_session_id,
                    msg_tx_driver,
                )
                .await;
            }));

            // Adapter: SessionEventMessage → SessionEvent. The consumer only
            // reads session_id/event_type/data, so a straight projection is
            // enough; timestamp/seq are informational.
            background_tasks.push(tokio::spawn(async move {
                while let Some(msg) = msg_rx_driver.recv().await {
                    let event = crucible_daemon::SessionEvent {
                        session_id: msg.session_id,
                        event_type: msg.event,
                        data: msg.data,
                    };
                    if event_tx_adapter.send(event).is_err() {
                        return;
                    }
                }
            }));

            self.is_replay = true;
            self.replay_remaining_completes = 1;
            app.set_precognition(false);
            app.set_status("Replay");

            let msg_tx_clone = msg_tx.clone();
            background_tasks.push(tokio::spawn(session_event_consumer(
                replay_session_id.clone(),
                event_rx_adapter,
                msg_tx_clone,
                None,
            )));

            // NoopAgentHandle: pure-display replay. No daemon RPC under any
            // code path. Drops cleanly (no session.end call).
            let mut agent = crate::tui::oil::noop_agent::NoopAgentHandle::new(replay_session_id);
            let interaction_rx = agent.take_interaction_receiver();

            let event_loop_result = self
                .event_loop(EventLoopParams {
                    app: &mut app,
                    agent: &mut agent,
                    bridge,
                    msg_tx,
                    msg_rx,
                    interaction_rx,
                    background_tasks: &mut background_tasks,
                })
                .await;
            Self::abort_background_tasks(&mut background_tasks);

            // Always restore terminal before propagating errors
            let _ = self.terminal.exit();
            event_loop_result?;
            return Ok(());
        }

        let selection = self.discover_agent().await;
        let (mut agent, live_event_rx) = create_agent(selection).await?;
        self.is_replay = false;
        self.replay_remaining_completes = 0;

        // Fresh sessions show "Loading..." and flip to "Ready" once the
        // daemon's setup task emits `mcp_servers_ready` (the last common
        // setup event). Resumed sessions don't receive setup events, so
        // stay at "Ready" immediately.
        if self.resume_session_id.is_some() {
            app.set_status("Ready");
        } else {
            app.set_status("Loading...");
        }

        // Spawn the live SessionEvent consumer if the factory handed us a
        // raw event receiver. This is the unified event path: the daemon's
        // broadcast flows through SessionEventStream → ChatAppMsg, matching
        // replay and resume.
        if let Some(event_rx) = live_event_rx {
            let session_id = agent.session_id().unwrap_or("").to_string();
            let msg_tx_live = msg_tx.clone();
            let context_limit = self.context_limit.clone();
            background_tasks.push(tokio::spawn(session_event_consumer(
                session_id,
                event_rx,
                msg_tx_live,
                Some(context_limit),
            )));
        }

        if !self.initial_sets.is_empty() {
            for effect in std::mem::take(&mut self.initial_sets) {
                match effect {
                    SetEffect::TuiLocal { key, value } => {
                        app.apply_cli_override(&key, value);
                    }
                    SetEffect::DaemonRpc(action) => {
                        let msg = match action {
                            SetRpcAction::SwitchModel(m) => ChatAppMsg::SwitchModel(m),
                            SetRpcAction::SetThinkingBudget(Some(b)) => {
                                ChatAppMsg::SetThinkingBudget(b)
                            }
                            SetRpcAction::SetThinkingBudget(None) => continue,
                            SetRpcAction::SetTemperature(t) => ChatAppMsg::SetTemperature(t),
                            SetRpcAction::SetMaxTokens(n) => ChatAppMsg::SetMaxTokens(n),
                            SetRpcAction::SetMaxIterations(n) => ChatAppMsg::SetMaxIterations(n),
                            SetRpcAction::SetExecutionTimeout(n) => {
                                ChatAppMsg::SetExecutionTimeout(n)
                            }
                            SetRpcAction::SetContextBudget(n) => ChatAppMsg::SetContextBudget(n),
                            SetRpcAction::SetContextStrategy(s) => {
                                ChatAppMsg::SetContextStrategy(s)
                            }
                            SetRpcAction::SetContextWindow(n) => ChatAppMsg::SetContextWindow(n),
                            SetRpcAction::SetOutputValidation(v) => {
                                ChatAppMsg::SetOutputValidation(v)
                            }
                            SetRpcAction::SetValidationRetries(n) => {
                                ChatAppMsg::SetValidationRetries(n)
                            }
                            SetRpcAction::SetPrecognitionResults(n) => {
                                ChatAppMsg::SetPrecognitionResults(n)
                            }
                            SetRpcAction::SetAutocompactThreshold(t) => {
                                ChatAppMsg::SetAutocompactThreshold(t)
                            }
                        };
                        let _ = msg_tx.send(msg);
                    }
                }
            }
        }

        // Connect to MCP servers in background to update tool_count /
        // connected state. The initial list (name, prefix, connected)
        // arrives from the daemon's `mcp_servers_ready` setup event;
        // this background task refines it with live upstream-connect
        // info. Triggered whenever an MCP config is present — we no
        // longer gate on `self.mcp_servers`, which is empty until the
        // setup event lands.
        //
        // Structurally live-path only: this runs inside `run_with_factory`.
        // The replay entry point (Task 2.3c) never reaches this code, so
        // the MCP gateway is not instantiated during replay.
        if let Some(ref mcp_config) = self.mcp_config {
            let mcp_config = mcp_config.clone();
            let mcp_tx = msg_tx.clone();
            background_tasks.push(tokio::spawn(async move {
                use crucible_daemon::tools::mcp_gateway::McpGatewayManager;
                match McpGatewayManager::from_config(&mcp_config).await {
                    Ok(gateway) => {
                        let servers: Vec<McpServerDisplay> = gateway
                            .upstream_names()
                            .map(|name| {
                                let tools_for_upstream: Vec<_> = gateway
                                    .all_tools()
                                    .into_iter()
                                    .filter(|t| t.upstream == name)
                                    .collect();
                                McpServerDisplay {
                                    name: name.to_string(),
                                    prefix: name.to_string(),
                                    tool_count: tools_for_upstream.len(),
                                    connected: !tools_for_upstream.is_empty(),
                                }
                            })
                            .collect();
                        let _ = mcp_tx.send(ChatAppMsg::McpStatusLoaded(servers));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to connect MCP servers: {}", e);
                    }
                }
                // Drop the gateway — Phase A is display-only
            }));
        }

        // Prefetch available models in background — daemon cache should be warm,
        // so this returns near-instantly. Ensures :model popup has data immediately.
        self.queue_model_prefetch(&msg_tx);

        let interaction_rx = agent.take_interaction_receiver();
        tracing::debug!(
            has_rx = interaction_rx.is_some(),
            "take_interaction_receiver"
        );

        let event_loop_result = self
            .event_loop(EventLoopParams {
                app: &mut app,
                agent: &mut agent,
                bridge,
                msg_tx,
                msg_rx,
                interaction_rx,
                background_tasks: &mut background_tasks,
            })
            .await;
        Self::abort_background_tasks(&mut background_tasks);

        // Capture session ID before dropping the agent
        let session_id = agent.session_id().map(|s| s.to_string());

        // Always restore terminal before propagating errors
        let _ = self.terminal.exit();
        event_loop_result?;

        // Print resume hint after terminal is restored to main screen
        if let Some(id) = session_id {
            use colored::Colorize;
            println!(
                "  Resume with: {}",
                format!("cru chat --resume {}", id).dimmed()
            );
        }

        Ok(())
    }

    async fn event_loop<A: AgentHandle>(
        &mut self,
        mut params: EventLoopParams<'_, A>,
    ) -> Result<()> {
        let mut event_stream = EventStream::new();
        let mut tick_interval = tokio::time::interval(self.tick_rate);
        let mut session_cmd_rx = self.session_cmd_rx.take();
        let mut replay_auto_exit_deadline = if self.is_replay
            && self.replay_remaining_completes == 0
            && self.replay_auto_exit.is_some()
        {
            Some(tokio::time::Instant::now())
        } else {
            None
        };

        loop {
            self.render_app_frame(params.app)?;

            match self
                .drain_phase_outcome(
                    params.app,
                    params.agent,
                    params.bridge,
                    &mut params.msg_rx,
                    &mut replay_auto_exit_deadline,
                )
                .await
            {
                DrainPhaseOutcome::Quit => return Ok(()),
                DrainPhaseOutcome::Continue => continue,
                DrainPhaseOutcome::Wait => {}
            }

            let select_outcome = tokio::select! {
                biased;

                event_opt = futures::StreamExt::next(&mut event_stream) => {
                    self.handle_terminal_event(event_opt)?
                }

                _ = tick_interval.tick() => {
                    tracing::trace!("tick");
                    EventLoopSelectOutcome::Event(Some(Event::Tick))
                }

                Some(cmd) = Self::next_session_command(&mut session_cmd_rx) => {
                    Self::handle_session_command(cmd, params.agent, params.app).await;
                    EventLoopSelectOutcome::Continue
                }

                Some(interaction_event) = Self::next_interaction_event(&mut params.interaction_rx) => {
                    let action = Self::handle_interaction_event(params.app, interaction_event);
                    // Process autoconfirm actions through the async path so
                    // interaction_respond is called on the agent handle.
                    if matches!(action, Action::Send(_)) {
                        let _ = self.process_action(ProcessActionParams {
                            action,
                            app: params.app,
                            agent: params.agent,
                            bridge: params.bridge,
                            msg_tx: &params.msg_tx,
                            background_tasks: params.background_tasks,
                        }).await;
                    }
                    EventLoopSelectOutcome::Continue
                }

                _ = Self::wait_for_replay_auto_exit(replay_auto_exit_deadline, self.replay_auto_exit),
                    if Self::should_wait_for_replay_auto_exit(
                        self.is_replay,
                        self.replay_remaining_completes,
                        replay_auto_exit_deadline,
                        self.replay_auto_exit,
                    ) => {
                    tracing::info!("Replay auto-exit triggered");
                    EventLoopSelectOutcome::Quit
                }
            };

            if self
                .handle_select_outcome(HandleSelectOutcomeParams {
                    select_outcome,
                    app: params.app,
                    agent: params.agent,
                    bridge: params.bridge,
                    msg_tx: &params.msg_tx,
                    background_tasks: params.background_tasks,
                })
                .await?
            {
                break;
            }
        }

        Ok(())
    }

    async fn drain_phase_outcome<A: AgentHandle>(
        &mut self,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        msg_rx: &mut mpsc::UnboundedReceiver<ChatAppMsg>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> DrainPhaseOutcome {
        let drain_outcome = self
            .drain_pending_messages(app, agent, bridge, msg_rx, replay_auto_exit_deadline)
            .await;

        if drain_outcome == DrainMessagesOutcome::Quit {
            return DrainPhaseOutcome::Quit;
        }
        if !Self::should_wait_for_event(drain_outcome) {
            return DrainPhaseOutcome::Continue;
        }

        DrainPhaseOutcome::Wait
    }

    async fn handle_selected_event<A: AgentHandle>(
        &mut self,
        params: HandleSelectedEventParams<'_, A>,
    ) -> Result<bool> {
        let Some(ev) = params.event else {
            return Ok(false);
        };

        let action = params.app.update(ev.clone());
        tracing::trace!(?ev, ?action, "processed event");

        if self
            .process_action(ProcessActionParams {
                action,
                app: params.app,
                agent: params.agent,
                bridge: params.bridge,
                msg_tx: params.msg_tx,
                background_tasks: params.background_tasks,
            })
            .await?
        {
            tracing::trace!("quit action received, breaking loop");
            return Ok(true);
        }

        Ok(false)
    }

    async fn handle_select_outcome<A: AgentHandle>(
        &mut self,
        params: HandleSelectOutcomeParams<'_, A>,
    ) -> Result<bool> {
        let event = match params.select_outcome {
            EventLoopSelectOutcome::Event(event) => event,
            EventLoopSelectOutcome::Continue => None,
            EventLoopSelectOutcome::Quit => return Ok(true),
        };

        self.handle_selected_event(HandleSelectedEventParams {
            event,
            app: params.app,
            agent: params.agent,
            bridge: params.bridge,
            msg_tx: params.msg_tx,
            background_tasks: params.background_tasks,
        })
        .await
    }

    fn handle_terminal_event(
        &mut self,
        event_opt: Option<std::result::Result<CtEvent, io::Error>>,
    ) -> Result<EventLoopSelectOutcome> {
        match event_opt {
            Some(Ok(ct_event)) => {
                tracing::trace!(?ct_event, "received crossterm event");
                Ok(EventLoopSelectOutcome::Event(Some(
                    self.convert_event(ct_event)?,
                )))
            }
            Some(Err(e)) => Err(e.into()),
            None => {
                tracing::warn!("EventStream returned None - stream ended");
                Ok(EventLoopSelectOutcome::Quit)
            }
        }
    }

    fn handle_interaction_event(
        app: &mut OilChatApp,
        interaction_event: crucible_core::interaction::InteractionEvent,
    ) -> Action<ChatAppMsg> {
        tracing::info!(
            request_id = %interaction_event.request_id,
            kind = %interaction_event.request.kind(),
            "Received interaction event"
        );
        let session_event = SessionEvent::InteractionRequested {
            request_id: interaction_event.request_id,
            request: interaction_event.request,
        };
        if let Some(msg) = Self::handle_session_event(session_event) {
            app.on_message(msg)
        } else {
            Action::Continue
        }
    }

    async fn next_session_command(
        session_cmd_rx: &mut Option<mpsc::UnboundedReceiver<SessionCommand>>,
    ) -> Option<SessionCommand> {
        match session_cmd_rx {
            Some(rx) => rx.recv().await,
            None => std::future::pending().await,
        }
    }

    async fn next_interaction_event(
        interaction_rx: &mut Option<
            mpsc::UnboundedReceiver<crucible_core::interaction::InteractionEvent>,
        >,
    ) -> Option<crucible_core::interaction::InteractionEvent> {
        match interaction_rx {
            Some(rx) => rx.recv().await,
            None => std::future::pending().await,
        }
    }

    fn should_wait_for_replay_auto_exit(
        is_replay: bool,
        replay_remaining_completes: usize,
        replay_auto_exit_deadline: Option<tokio::time::Instant>,
        replay_auto_exit: Option<u64>,
    ) -> bool {
        is_replay
            && replay_remaining_completes == 0
            && replay_auto_exit_deadline.is_some()
            && replay_auto_exit.is_some()
    }

    async fn wait_for_replay_auto_exit(
        replay_auto_exit_deadline: Option<tokio::time::Instant>,
        replay_auto_exit: Option<u64>,
    ) {
        match replay_auto_exit_deadline {
            Some(deadline_start) => {
                let delay_ms = replay_auto_exit.unwrap_or(0);
                tokio::time::sleep_until(deadline_start + Duration::from_millis(delay_ms)).await;
            }
            None => std::future::pending::<()>().await,
        }
    }

    fn convert_event(&mut self, ct_event: CtEvent) -> io::Result<Event> {
        match ct_event {
            CtEvent::Key(key) => Ok(Event::Key(key)),
            CtEvent::Resize(w, h) => {
                self.terminal.handle_resize()?;
                Ok(Event::Resize {
                    width: w,
                    height: h,
                })
            }
            _ => Ok(Event::Tick),
        }
    }

    async fn discover_agent(&self) -> AgentSelection {
        match &self.agent_name {
            Some(name) => AgentSelection::Acp(name.clone()),
            None => AgentSelection::Internal,
        }
    }
}
