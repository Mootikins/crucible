use crate::session::OpenedSession;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::commands::SetEffect;
use crate::tui::oil::event::Event;
use crate::tui::oil::theme;
use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{
    live_session_event_consumer, session_event_consumer, ChatExit, DrainMessagesOutcome,
    DrainPhaseOutcome, EventLoopParams, EventLoopSelectOutcome, HandleSelectOutcomeParams,
    HandleSelectedEventParams, OilChatRunner, ProcessActionParams, SessionEventStream,
};

impl OilChatRunner {
    /// Run the chat TUI. `open_session` opens the daemon session; a replay
    /// never calls it.
    pub async fn run_with_factory<F, Fut>(&mut self, open_session: F) -> Result<ChatExit>
    where
        F: Fn(AgentSelection) -> Fut,
        Fut: std::future::Future<Output = Result<OpenedSession>>,
    {
        self.terminal.enter()?;

        let mut app = OilChatApp::default();
        app.set_mode(self.mode.clone());
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
        if let Some(dir) = self.shell_output_dir.take() {
            app.set_shell_output_dir(dir);
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
        app.set_show_thinking(self.show_thinking);
        app.set_show_diffs(self.show_diffs);

        // The banner goes in before the first frame, so the session opens
        // saying what knowledge it is attached to. A replay attaches nothing
        // and answers nothing, so it gets no banner.
        if self.replay_path.is_none() {
            app.announce_kilns(&std::mem::take(&mut self.connected_kilns));
        }

        let terminal_size = self.terminal.size();
        let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
        // Reserve the popup's rows for this frame too — `render` here does not
        // go through `render_frame`, and a first frame at another height would
        // move the prompt as soon as the second one lands.
        self.terminal
            .set_min_viewport_rows(app.min_viewport_rows(&ctx));
        if self.fullscreen.is_some() {
            self.render_app_frame(&mut app)?;
        } else {
            let tree = app.view(&ctx);
            self.terminal.render(&tree, "")?;
        }

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

            // The local driver sends the wire type. The consumer reads the
            // same type, so no adapter sits between them.
            let (event_tx_driver, event_rx_driver) =
                tokio::sync::mpsc::unbounded_channel::<crucible_daemon::SessionEvent>();

            let driver_session_id = replay_session_id.clone();
            let driver_speed = self.replay_speed;
            background_tasks.push(tokio::spawn(async move {
                crate::tui::oil::local_replay::drive_replay(
                    events,
                    driver_speed,
                    driver_session_id,
                    event_tx_driver,
                )
                .await;
            }));

            self.is_replay = true;
            self.replay_remaining_completes = 1;
            app.set_precognition(false);
            app.set_status("Replay");

            let msg_tx_clone = msg_tx.clone();
            background_tasks.push(tokio::spawn(session_event_consumer(
                replay_session_id.clone(),
                event_rx_driver,
                msg_tx_clone,
                None,
            )));

            // A replay has no session, so no code path can reach the daemon.
            let event_loop_result = self
                .event_loop(EventLoopParams {
                    app: &mut app,
                    session: None,
                    msg_tx,
                    msg_rx,
                    background_tasks: &mut background_tasks,
                })
                .await;
            Self::abort_background_tasks(&mut background_tasks);

            // Always restore terminal before propagating errors
            self.exit_terminal(&mut app);
            event_loop_result?;
            return Ok(ChatExit::Quit);
        }

        let selection = self.discover_agent().await;
        let OpenedSession {
            live: session,
            events,
            pending,
            workspace,
        } = open_session(selection).await?;
        app.set_session_workspace(workspace);
        // The model list is session-scoped: `session.list_models` answers an
        // ACP agent's own selector and the provider catalogue (narrowed by
        // the session's classification) for an internal one. The
        // all-providers catalogue would offer an ACP agent models it would
        // reject.
        let session_models_source = Some(session.id.clone());
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

        // The live consumer: the daemon's broadcast flows through
        // SessionEventStream → ChatAppMsg, matching replay and resume. It
        // also opens the prompts, first the ones that waited before this
        // client attached.
        background_tasks.push(tokio::spawn(live_session_event_consumer(
            session.id.clone(),
            events,
            pending,
            msg_tx.clone(),
            self.context_limit.clone(),
        )));

        // The first status list, read now that the session's events flow:
        // a status that changes after this read arrives as an event.
        Self::spawn_status_fetch(Some(session.id.clone()), &msg_tx, &mut background_tasks);
        Self::spawn_notification_fetch(Some(session.id.clone()), &msg_tx, &mut background_tasks);

        self.apply_initial_sets(&mut app, Some(&session), &msg_tx, &mut background_tasks)
            .await?;

        // MCP server status is NOT fetched here. The daemon publishes it on
        // `mcp_servers_ready` with live tool counts and connection state
        // (`AgentManager::mcp_tools_by_upstream`), which is where it has always
        // belonged: the daemon holds a connected gateway for its whole lifetime.
        // This used to fork a second one — one stdio child process per configured
        // upstream, plus an `initialize` round-trip with each, on every launch —
        // and drop it after counting tools.

        // Prefetch available models in background — daemon cache should be warm,
        // so this returns near-instantly. Ensures :model popup has data immediately.
        self.queue_model_prefetch(&msg_tx, &mut background_tasks, session_models_source);
        // The status line counts the proposals from the first frame, not
        // from the first `proposal_changed` event.
        Self::spawn_proposal_fetch(false, &msg_tx, &mut background_tasks);

        let event_loop_result = self
            .event_loop(EventLoopParams {
                app: &mut app,
                session: Some(&session),
                msg_tx,
                msg_rx,
                background_tasks: &mut background_tasks,
            })
            .await;
        Self::abort_background_tasks(&mut background_tasks);

        // Always restore terminal before propagating errors
        self.exit_terminal(&mut app);
        // The session ends when the TUI closes. `--resume` opens it again.
        session.end().await;
        event_loop_result?;

        // `/resume` chose another session: the caller opens it at once, so a
        // hint to resume this one later is noise.
        if let Some(next) = self.next_session.take() {
            return Ok(ChatExit::Resume(next));
        }

        // Print resume hint after terminal is restored to main screen
        use colored::Colorize;
        println!(
            "  Resume with: {}",
            format!("cru chat --resume {}", session.id).dimmed()
        );

        Ok(ChatExit::Quit)
    }

    /// Apply `cru chat --set` startup overrides.
    ///
    /// Daemon-bound overrides must go through `process_action` — that's
    /// where the RPC arms live. Sending them down the UI message channel
    /// only reaches the reducer, silently dropping the daemon call (the
    /// same seam documented on `queue_model_prefetch`), which left every
    /// `--set <daemon-key>=…` inert and `--set model=…` a display-only lie.
    pub(super) async fn apply_initial_sets(
        &mut self,
        app: &mut OilChatApp,
        session: Option<&crate::session::LiveSession>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<tokio::task::JoinHandle<()>>,
    ) -> io::Result<()> {
        for effect in std::mem::take(&mut self.initial_sets) {
            match effect {
                SetEffect::TuiLocal { key, value } => {
                    app.apply_cli_override(&key, value);
                }
                SetEffect::DaemonRpc(action) => {
                    let Some(msg) = action.into_chat_msg() else {
                        continue;
                    };
                    self.process_action(ProcessActionParams {
                        action: Action::Send(msg),
                        app,
                        session,
                        msg_tx,
                        background_tasks,
                    })
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn event_loop(&mut self, mut params: EventLoopParams<'_>) -> Result<()> {
        let mut event_stream = EventStream::new();
        let mut tick_interval = tokio::time::interval(self.tick_rate);
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
                .drain_phase_outcome(&mut params, &mut replay_auto_exit_deadline)
                .await?
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

                // Last, so input, ticks and prompts go first: this branch
                // runs only when nothing else is ready. The full-screen view
                // lays out one batch of the nodes that still have only an
                // estimate, then the loop draws a frame and waits again.
                _ = std::future::ready(()),
                    if self.fullscreen.as_ref().is_some_and(|view| view.has_idle_work()) => {
                    if let Some(view) = self.fullscreen.as_mut() {
                        view.lay_out_idle(params.app, crate::tui::oil::fullscreen::IDLE_BUDGET);
                    }
                    EventLoopSelectOutcome::Continue
                }
            };

            if self
                .handle_select_outcome(HandleSelectOutcomeParams {
                    select_outcome,
                    app: params.app,
                    session: params.session,
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

    async fn drain_phase_outcome(
        &mut self,
        params: &mut EventLoopParams<'_>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> Result<DrainPhaseOutcome> {
        let drain_outcome = self
            .drain_pending_messages(params, replay_auto_exit_deadline)
            .await?;

        if drain_outcome == DrainMessagesOutcome::Quit {
            return Ok(DrainPhaseOutcome::Quit);
        }
        if !Self::should_wait_for_event(drain_outcome) {
            return Ok(DrainPhaseOutcome::Continue);
        }

        Ok(DrainPhaseOutcome::Wait)
    }

    async fn handle_selected_event(
        &mut self,
        params: HandleSelectedEventParams<'_>,
    ) -> Result<bool> {
        let Some(ev) = params.event else {
            return Ok(false);
        };

        // The full-screen view takes its scroll keys and the mouse first.
        if let Some(view) = self.fullscreen.as_mut() {
            use crate::tui::oil::fullscreen::ViewAction;
            match view.handle_event(&ev, params.app) {
                ViewAction::Ignored => {}
                ViewAction::Handled => return Ok(false),
                ViewAction::Copy(text) => {
                    self.copy_text(params.app, &text);
                    return Ok(false);
                }
                ViewAction::Dump(rows) => {
                    self.terminal.print_to_main_screen(&rows)?;
                    params
                        .app
                        .add_notification(crucible_core::types::Notification::toast(format!(
                            "Printed {} rows to the terminal scrollback",
                            rows.len()
                        )));
                    return Ok(false);
                }
                ViewAction::ToggleMouse => {
                    let on = !self.terminal.mouse_captured();
                    self.terminal.set_mouse_capture(on)?;
                    params.app.add_notification(crucible_core::types::Notification::toast(
                        if on {
                            "Mouse on: the TUI scrolls and selects (F2 for the terminal's selection)"
                        } else {
                            "Mouse off: the terminal selects (F2 to give the mouse back)"
                        },
                    ));
                    return Ok(false);
                }
            }
        }

        let action = params.app.update(ev.clone());
        tracing::trace!(?ev, ?action, "processed event");

        if self
            .process_action(ProcessActionParams {
                action,
                app: params.app,
                session: params.session,
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

    async fn handle_select_outcome(
        &mut self,
        params: HandleSelectOutcomeParams<'_>,
    ) -> Result<bool> {
        let event = match params.select_outcome {
            EventLoopSelectOutcome::Event(event) => event,
            EventLoopSelectOutcome::Continue => None,
            EventLoopSelectOutcome::Quit => return Ok(true),
        };

        self.handle_selected_event(HandleSelectedEventParams {
            event,
            app: params.app,
            session: params.session,
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

    /// Restore the terminal. The full-screen mode then prints the transcript
    /// that it has not printed yet to the main screen, so the session stays
    /// in the terminal's scrollback after the exit.
    fn exit_terminal(&mut self, app: &mut OilChatApp) {
        let rows = self
            .fullscreen
            .as_mut()
            .map(|view| view.take_dump(app, true))
            .unwrap_or_default();
        let _ = self.terminal.exit();
        if !rows.is_empty() {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            for row in rows {
                let _ = writeln!(out, "{row}\x1b[0m");
            }
            let _ = out.flush();
        }
    }

    /// Copy `text` through the full-screen copy chain and say how in a toast.
    fn copy_text(&mut self, app: &mut OilChatApp, text: &str) {
        use crate::tui::oil::fullscreen::clipboard::CopyEnv;
        let terminal = &mut self.terminal;
        let report = self.copier.copy(text, CopyEnv::detect(), |sequence| {
            terminal.write_raw(sequence).map_err(|e| e.to_string())
        });
        tracing::debug!(?report, "full-screen copy");
        app.add_notification(crucible_core::types::Notification::toast(
            report.summary(text.chars().count()),
        ));
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
            CtEvent::Paste(text) => Ok(Event::Paste(text)),
            CtEvent::Mouse(mouse) => Ok(Event::Mouse(mouse)),
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
