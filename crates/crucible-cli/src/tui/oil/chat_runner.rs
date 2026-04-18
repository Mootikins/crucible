use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{
    ChatAppMsg, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry,
};
use crate::tui::oil::event::Event;
use crate::tui::oil::theme;
use anyhow::Result;
#[allow(unused_imports)] // WIP: KeyCode, KeyModifiers not yet used
use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyModifiers};
use crucible_core::error_utils::strip_tool_error_prefix;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::traits::chat::AgentHandle;
use crucible_lua::SessionCommand;
use crucible_oil::focus::FocusContext;
use crucible_oil::terminal::Terminal;
use crucible_oil::FrameRenderer;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::tui::oil::commands::{SetEffect, SetRpcAction};

/// Render one frame through the shared FrameRenderer trait.
///
/// This is the single rendering function used by all paths:
/// - Live TUI (via Terminal)
/// - Fixture tests (via TestRuntime)
/// - Replay (via Terminal, same as live)
///
/// Handles: full redraw detection, scroll offset sync, view building,
/// rendering, and graduation feedback.
pub fn render_frame(app: &mut OilChatApp, renderer: &mut impl FrameRenderer, focus: &FocusContext) {
    if app.take_needs_full_redraw() {
        renderer.force_full_redraw();
    }

    // Expire toast notifications (previously done on Event::Tick)
    app.expire_toasts();

    // Build ViewContext first — needed for both graduation and viewport rendering
    let terminal_size = renderer.size();
    let ctx = ViewContext::with_terminal_size(focus, theme::active(), terminal_size);

    // Drain completed containers → stdout (terminal scrollback)
    let graduation = app.drain_graduated(&ctx);
    let tree = app.view(&ctx);
    renderer.render_frame(&tree, graduation.as_ref());
}

/// Parameters for event_loop function.
struct EventLoopParams<'a, A: AgentHandle> {
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: mpsc::UnboundedSender<ChatAppMsg>,
    pub msg_rx: mpsc::UnboundedReceiver<ChatAppMsg>,
    pub interaction_rx:
        Option<mpsc::UnboundedReceiver<crucible_core::interaction::InteractionEvent>>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for handle_selected_event function.
struct HandleSelectedEventParams<'a, A: AgentHandle> {
    pub event: Option<Event>,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for handle_select_outcome function.
struct HandleSelectOutcomeParams<'a, A: AgentHandle> {
    pub select_outcome: EventLoopSelectOutcome,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for process_action function.
struct ProcessActionParams<'a, A: AgentHandle> {
    pub action: Action<ChatAppMsg>,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

pub struct OilChatRunner {
    terminal: Terminal,
    tick_rate: Duration,
    mode: ChatMode,
    model: String,
    context_limit: Arc<AtomicUsize>,
    focus: FocusContext,
    workspace_files: Vec<String>,
    kiln_notes: Vec<String>,
    session_dir: Option<PathBuf>,
    resume_session_id: Option<String>,
    resume_history: Option<Vec<serde_json::Value>>,
    mcp_servers: Vec<McpServerDisplay>,
    plugin_status: Vec<PluginStatusEntry>,
    mcp_config: Option<crucible_config::mcp::McpConfig>,
    available_models: Vec<String>,
    show_thinking: bool,
    session_cmd_rx: Option<mpsc::UnboundedReceiver<SessionCommand>>,
    slash_commands: Vec<(String, String)>,
    agent_name: Option<String>,
    initial_sets: Vec<SetEffect>,
    recording_mode: Option<String>,
    recording_path: Option<PathBuf>,
    replay_path: Option<PathBuf>,
    replay_speed: f64,
    replay_auto_exit: Option<u64>,
    replay_remaining_completes: usize,
    is_replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrainMessagesOutcome {
    Idle,
    Quit,
    Processed,
}

enum EventLoopSelectOutcome {
    Event(Option<Event>),
    Continue,
    Quit,
}

enum DrainPhaseOutcome {
    Wait,
    Continue,
    Quit,
}

impl OilChatRunner {
    pub fn new() -> io::Result<Self> {
        Ok(Self::with_terminal(Terminal::new()?))
    }

    pub(crate) fn with_terminal(terminal: Terminal) -> Self {
        Self {
            terminal,
            tick_rate: Duration::from_millis(50),
            mode: ChatMode::Normal,
            model: String::new(),
            context_limit: Arc::new(AtomicUsize::new(0)),
            focus: FocusContext::new(),
            workspace_files: Vec::new(),
            kiln_notes: Vec::new(),
            session_dir: None,
            resume_session_id: None,
            resume_history: None,
            mcp_servers: Vec::new(),
            plugin_status: Vec::new(),
            mcp_config: None,
            available_models: Vec::new(),
            show_thinking: false,
            session_cmd_rx: None,
            slash_commands: Vec::new(),
            agent_name: None,
            initial_sets: Vec::new(),
            recording_mode: None,
            recording_path: None,
            replay_path: None,
            replay_speed: 1.0,
            replay_auto_exit: None,
            replay_remaining_completes: 0,
            is_replay: false,
        }
    }

    pub fn with_session_command_receiver(
        mut self,
        rx: mpsc::UnboundedReceiver<SessionCommand>,
    ) -> Self {
        self.session_cmd_rx = Some(rx);
        self
    }

    pub fn with_context_limit(mut self, limit: usize) -> Self {
        self.context_limit = Arc::new(AtomicUsize::new(limit));
        self
    }

    /// Returns a handle to set context_limit from a background task.
    pub fn context_limit_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.context_limit)
    }

    pub fn with_mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_session_dir(mut self, path: PathBuf) -> Self {
        self.session_dir = Some(path);
        self
    }

    pub fn with_resume_session(mut self, session_id: impl Into<String>) -> Self {
        self.resume_session_id = Some(session_id.into());
        self
    }

    pub fn with_resume_history(mut self, history: Vec<serde_json::Value>) -> Self {
        self.resume_history = Some(history);
        self
    }

    pub fn with_available_models(mut self, models: Vec<String>) -> Self {
        self.available_models = models;
        self
    }

    pub fn with_show_thinking(mut self, show: bool) -> Self {
        self.show_thinking = show;
        self
    }

    pub fn with_slash_commands(mut self, commands: Vec<(String, String)>) -> Self {
        self.slash_commands = commands;
        self
    }

    pub fn with_mcp_config(mut self, config: crucible_config::mcp::McpConfig) -> Self {
        self.mcp_config = Some(config);
        self
    }

    pub fn with_agent_name(mut self, name: Option<String>) -> Self {
        self.agent_name = name;
        self
    }

    pub fn with_initial_sets(mut self, sets: Vec<SetEffect>) -> Self {
        self.initial_sets = sets;
        self
    }

    pub fn with_recording_mode(mut self, mode: Option<String>) -> Self {
        self.recording_mode = mode;
        self
    }

    pub fn with_recording_path(mut self, path: Option<PathBuf>) -> Self {
        self.recording_path = path;
        self
    }

    pub fn with_replay_path(mut self, path: Option<PathBuf>) -> Self {
        self.replay_path = path;
        self
    }

    pub fn with_replay_speed(mut self, speed: f64) -> Self {
        self.replay_speed = speed;
        self
    }

    pub fn with_replay_auto_exit(mut self, delay: Option<u64>) -> Self {
        self.replay_auto_exit = delay;
        self
    }

    fn is_acp_session(&self) -> bool {
        self.agent_name.is_some()
    }

    /// Queue an initial `FetchModels` message so the `:model` popup has data
    /// without a user-triggered round-trip.
    ///
    /// Structurally live-path only: called exclusively from
    /// `run_with_factory`. The replay entry point (added in Task 2.3c) does
    /// not invoke this. If `FetchModels` ever reaches the event loop under
    /// replay anyway, the guard on the `ChatAppMsg::FetchModels` arm
    /// swallows it — see the match-arm comment there.
    fn queue_model_prefetch(&self, msg_tx: &mpsc::UnboundedSender<ChatAppMsg>) {
        if self.is_acp_session() {
            return;
        }

        if msg_tx.send(ChatAppMsg::FetchModels).is_err() {
            tracing::warn!("UI channel closed, initial FetchModels dropped");
        }
    }

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
        // replay and resume. Live turns do not consume ChatChunk streams.
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

    fn render_app_frame(&mut self, app: &mut OilChatApp) -> Result<()> {
        if app.has_shell_modal() {
            // Shell modal uses fullscreen rendering (Terminal-specific)
            if app.take_needs_full_redraw() {
                self.terminal.force_full_redraw()?;
            }
            let terminal_size = self.terminal.size();
            let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
            let tree = app.view(&ctx);
            self.terminal.render_fullscreen(&tree)?;
        } else {
            // Normal rendering through the shared FrameRenderer trait
            render_frame(app, &mut self.terminal, &self.focus);
        }
        Ok(())
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

    /// Route a `ChatAppMsg` to the app reducer, and — in live mode — kick
    /// off any side-effects (e.g. sending a user message via RPC).
    ///
    /// Live sends are fire-and-forget: the daemon broadcasts the ensuing
    /// turn as `SessionEvent`s, which `session_event_consumer` feeds back
    /// into this channel. In replay mode the daemon already drives
    /// everything, so `UserMessage` is purely a display signal.
    async fn process_message<A: AgentHandle>(
        msg: &ChatAppMsg,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        is_replay: bool,
    ) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(content) => {
                if !is_replay && !app.is_streaming() && !app.precognition() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: content.clone(),
                        participant_id: "user".to_string(),
                    });
                    if let Err(e) = agent.send_message_fire_and_forget(content.clone()).await {
                        tracing::warn!(error = %e, "send_message_fire_and_forget failed");
                    }
                }
            }
            ChatAppMsg::EnrichedMessage {
                original, enriched, ..
            } => {
                if !is_replay && !app.is_streaming() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: original.clone(),
                        participant_id: "user".to_string(),
                    });
                    if let Err(e) = agent.send_message_fire_and_forget(enriched.clone()).await {
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

    #[cfg(test)]
    pub(crate) async fn process_message_for_test<A: AgentHandle>(
        msg: &ChatAppMsg,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        is_replay: bool,
    ) -> Action<ChatAppMsg> {
        Self::process_message(msg, app, agent, bridge, is_replay).await
    }

    async fn drain_pending_messages<A: AgentHandle>(
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

    fn should_wait_for_event(outcome: DrainMessagesOutcome) -> bool {
        matches!(outcome, DrainMessagesOutcome::Idle)
    }

    async fn process_action<A: AgentHandle>(
        &mut self,
        params: ProcessActionParams<'_, A>,
    ) -> io::Result<bool> {
        match params.action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                match &msg {
                    ChatAppMsg::Undo(count) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Undo not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
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
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "History clearing not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        if params.app.is_streaming() {
                            if let Err(e) = params.agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream");
                            }
                        }
                        params.agent.clear_history().await;
                        params.app.reset_session();
                        tracing::info!("New session started (history cleared)");
                    }
                    ChatAppMsg::StreamCancelled => {
                        if params.app.is_streaming() {
                            if let Err(e) = params.agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream on daemon");
                            }
                            tracing::info!("Cancelled active turn via session.cancel RPC");
                        }
                    }
                    ChatAppMsg::SwitchModel(model_id) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Model switching not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(model = %model_id, "Model switch requested");
                        match params.agent.switch_model(model_id).await {
                            Ok(()) => {
                                tracing::info!(model = %model_id, "Model switched successfully");
                            }
                            Err(e) => {
                                tracing::warn!(model = %model_id, error = %e, "Model switch not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::FetchModels if !self.is_replay => {
                        // Skipped in replay mode to preserve the TUI-only guarantee
                        // (no daemon RPC calls). Replay never populates the model
                        // picker — `:model` is moot when there's no live session.
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Model listing not available for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
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
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Thinking budget not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(budget = budget, "Setting thinking budget");
                        match params.agent.set_thinking_budget(*budget).await {
                            Ok(()) => {
                                tracing::info!(budget = budget, "Thinking budget set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(budget = budget, error = %e, "Thinking budget not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetTemperature(temp) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Temperature setting not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(temperature = temp, "Setting temperature");
                        match params.agent.set_temperature(*temp).await {
                            Ok(()) => {
                                tracing::info!(temperature = temp, "Temperature set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(temperature = temp, error = %e, "Temperature not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetMaxTokens(max_tokens) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Max tokens setting not supported for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(max_tokens = ?max_tokens, "Setting max_tokens");
                        match params.agent.set_max_tokens(*max_tokens).await {
                            Ok(()) => {
                                tracing::info!(max_tokens = ?max_tokens, "Max tokens set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(max_tokens = ?max_tokens, error = %e, "Max tokens not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetMaxIterations(max_iterations) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Max iterations setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(max_iterations = ?max_iterations, "Setting max_iterations");
                        match params.agent.set_max_iterations(*max_iterations).await {
                            Ok(()) => {
                                tracing::info!(max_iterations = ?max_iterations, "Max iterations set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(max_iterations = ?max_iterations, error = %e, "Max iterations not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetExecutionTimeout(timeout_secs) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Execution timeout setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(timeout_secs = ?timeout_secs, "Setting execution_timeout");
                        match params.agent.set_execution_timeout(*timeout_secs).await {
                            Ok(()) => {
                                tracing::info!(timeout_secs = ?timeout_secs, "Execution timeout set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(timeout_secs = ?timeout_secs, error = %e, "Execution timeout not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetContextBudget(budget) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Context budget setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(context_budget = ?budget, "Setting context_budget");
                        match params.agent.set_context_budget(*budget).await {
                            Ok(()) => {
                                tracing::info!(context_budget = ?budget, "Context budget set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(context_budget = ?budget, error = %e, "Context budget not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetContextStrategy(strategy_str) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Context strategy setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(context_strategy = %strategy_str, "Setting context_strategy");
                        match strategy_str.parse::<crucible_core::session::ContextStrategy>() {
                            Ok(strategy) => {
                                match params.agent.set_context_strategy(strategy).await {
                                    Ok(()) => {
                                        tracing::info!(context_strategy = %strategy_str, "Context strategy set successfully");
                                    }
                                    Err(e) => {
                                        tracing::warn!(context_strategy = %strategy_str, error = %e, "Context strategy not supported by this agent");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Invalid context strategy");
                            }
                        }
                    }
                    ChatAppMsg::SetContextWindow(window) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Context window setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(context_window = ?window, "Setting context_window");
                        match params.agent.set_context_window(*window).await {
                            Ok(()) => {
                                tracing::info!(context_window = ?window, "Context window set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(context_window = ?window, error = %e, "Context window not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetOutputValidation(ref validation_str) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Output validation setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(output_validation = %validation_str, "Setting output_validation");
                        match validation_str.parse::<crucible_core::session::OutputValidation>() {
                            Ok(validation) => {
                                match params.agent.set_output_validation(validation).await {
                                    Ok(()) => {
                                        tracing::info!(output_validation = %validation_str, "Output validation set successfully");
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Output validation not supported by this agent");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Invalid output validation");
                            }
                        }
                    }
                    ChatAppMsg::SetValidationRetries(retries) => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Validation retries setting not supported for ACP agents"
                                        .to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::info!(validation_retries = retries, "Setting validation_retries");
                        match params.agent.set_validation_retries(*retries).await {
                            Ok(()) => {
                                tracing::info!(
                                    validation_retries = retries,
                                    "Validation retries set successfully"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(validation_retries = retries, error = %e, "Validation retries not supported by this agent");
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
                        if !self.is_replay && !params.app.is_streaming() {
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

    pub(crate) fn abort_background_tasks(background_tasks: &mut Vec<JoinHandle<()>>) {
        for task in background_tasks.drain(..) {
            task.abort();
        }
    }

    async fn handle_session_command<A: AgentHandle>(
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
                let result = agent.switch_model(&model).await.map_err(|e| e.to_string());
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
            vec![ChatAppMsg::ToolCall {
                name,
                args,
                call_id,
                description,
                source,
                lua_primary_arg,
            }]
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

/// Stateful SessionEvent → ChatAppMsg converter.
///
/// Tracks `saw_text_delta` per turn so `message_complete.full_response`
/// only produces a TextDelta when no granular text_deltas preceded it
/// (the "coarse resume" case — daemon drops text_delta during storage
/// compaction, keeping only the final message_complete snapshot).
///
/// Optionally holds a `context_limit` handle so that `message_complete`
/// token counts can be converted into a `ContextUsage` with the correct
/// `total` field. Without a handle, the total defaults to 0.
pub struct SessionEventStream {
    saw_text_delta: bool,
    context_limit: Option<Arc<AtomicUsize>>,
}

impl SessionEventStream {
    pub fn new() -> Self {
        Self {
            saw_text_delta: false,
            context_limit: None,
        }
    }

    pub fn with_context_limit(mut self, limit: Arc<AtomicUsize>) -> Self {
        self.context_limit = Some(limit);
        self
    }

    pub fn translate(&mut self, event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
        if event_type == "text_delta" {
            self.saw_text_delta = true;
        } else if event_type == "user_message" {
            self.saw_text_delta = false;
        }

        // Late thinking summaries arrive after text_delta and
        // duplicate incremental thinking deltas — drop them.
        if event_type == "thinking" && self.saw_text_delta {
            return Vec::new();
        }

        let raw = session_event_to_chat_msgs(event_type, data);

        // When the daemon's setup task emits `context_limit_resolved`, also
        // stamp the atomic so that subsequent `message_complete` events pick
        // up the real total for their `ContextUsage` patching.
        if event_type == "context_limit_resolved" {
            if let Some(ref limit) = self.context_limit {
                for msg in &raw {
                    if let ChatAppMsg::ContextLimitResolved { limit: l, .. } = msg {
                        limit.store(*l, Ordering::Relaxed);
                    }
                }
            }
        }

        // For message_complete, filter out the TextDelta if granular deltas
        // were seen, and patch the ContextUsage with the real context limit.
        if event_type == "message_complete" {
            let saw_deltas = self.saw_text_delta;
            let total_limit = self
                .context_limit
                .as_ref()
                .map(|l| l.load(Ordering::Relaxed))
                .unwrap_or(0);
            raw.into_iter()
                .filter_map(|m| match m {
                    ChatAppMsg::TextDelta(_) if saw_deltas => None,
                    ChatAppMsg::ContextUsage { used, .. } => Some(ChatAppMsg::ContextUsage {
                        used,
                        total: total_limit,
                    }),
                    other => Some(other),
                })
                .collect()
        } else {
            raw
        }
    }
}

impl Default for SessionEventStream {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared event-pump used by both replay and live consumers.
///
/// Filters out events for other sessions via `session_filter`, feeds the
/// survivors through `SessionEventStream`, and forwards the resulting
/// `ChatAppMsg`s to the app's event channel. Returns when `event_rx`
/// closes, the filter rejects an event that the caller wants to stop on
/// (via returning `None` from `on_event`), or `msg_tx` closes.
///
/// `on_event` lets the replay path recognize `replay_complete` and emit
/// a terminal Status message. Live mode passes a no-op.
async fn consume_session_events<F, E>(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    context_limit: Option<Arc<AtomicUsize>>,
    session_filter: F,
    mut on_event: E,
) where
    F: Fn(&crucible_daemon::SessionEvent) -> bool,
    E: FnMut(
        &crucible_daemon::SessionEvent,
        &tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    ) -> bool,
{
    let mut stream = SessionEventStream::new();
    if let Some(limit) = context_limit {
        stream = stream.with_context_limit(limit);
    }
    while let Some(event) = event_rx.recv().await {
        if !session_filter(&event) {
            continue;
        }
        if !on_event(&event, &msg_tx) {
            return;
        }
        for msg in stream.translate(&event.event_type, &event.data) {
            if msg_tx.send(msg).is_err() {
                return;
            }
        }
    }
}

/// Daemon reports fatal turn failures via `ended { reason: "error: ..." }`.
/// Surface them as an `Error` ChatAppMsg so the status bar shows the cause.
/// Shared by both live and replay paths — replay of an error-ending recording
/// renders identically to a live session that ended with that error.
fn promote_ended_error(
    event: &crucible_daemon::SessionEvent,
    tx: &tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
) {
    if event.event_type == "ended" {
        if let Some(reason) = event.data.get("reason").and_then(|v| v.as_str()) {
            if let Some(err) = reason.strip_prefix("error: ") {
                let _ = tx.send(ChatAppMsg::Error(err.to_string()));
            }
        }
    }
}

/// Unified session event consumer for both live and replay modes.
///
/// Drains `event_rx`, filtering events for `session_id` and translating them
/// through `SessionEventStream` into `ChatAppMsg`s on `msg_tx`. Both paths
/// share the `ended: error: ...` → `ChatAppMsg::Error` promotion. Replay
/// additionally terminates on `replay_complete`, emitting a final Status.
///
/// `context_limit` is `Some(_)` for live (so `message_complete` can fill in
/// the total for `ContextUsage`) and `None` for replay (the recorded events
/// already carry the total).
pub(crate) async fn session_event_consumer(
    session_id: String,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    context_limit: Option<Arc<AtomicUsize>>,
) {
    let filter_id = session_id.clone();
    consume_session_events(
        event_rx,
        msg_tx,
        context_limit,
        move |event| event.session_id == filter_id,
        |event, tx| {
            promote_ended_error(event, tx);
            if event.event_type == "replay_complete" {
                let _ = tx.send(ChatAppMsg::Status("Replay complete".to_string()));
                return false;
            }
            true
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replay_consumer_handles_delegation_spawned() {
        use serde_json::json;
        use tokio::time::{timeout, Duration};

        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let replay_session_id = "test-session-delegation-spawned".to_string();
        let session_id_clone = replay_session_id.clone();

        let consumer_task = tokio::spawn(async move {
            session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
        });

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id.clone(),
                event_type: "delegation_spawned".to_string(),
                data: json!({
                    "delegation_id": "d1",
                    "prompt": "test prompt",
                    "target_agent": "opencode"
                }),
            })
            .unwrap();

        let msg = timeout(Duration::from_secs(1), msg_rx.recv())
            .await
            .expect("Timeout waiting for message")
            .expect("Should receive a message");

        match msg {
            ChatAppMsg::DelegationSpawned {
                id,
                prompt,
                target_agent,
            } => {
                assert_eq!(id, "d1");
                assert_eq!(prompt, "test prompt");
                assert_eq!(target_agent, Some("opencode".to_string()));
            }
            other => panic!("Expected DelegationSpawned, got {:?}", other),
        }

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id,
                event_type: "replay_complete".to_string(),
                data: json!({}),
            })
            .unwrap();
        drop(event_tx);

        timeout(Duration::from_secs(1), consumer_task)
            .await
            .expect("Timeout waiting for consumer task")
            .expect("Consumer task should complete");
    }

    #[tokio::test]
    async fn replay_consumer_handles_delegation_completed() {
        use serde_json::json;
        use tokio::time::{timeout, Duration};

        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let replay_session_id = "test-session-delegation-completed".to_string();
        let session_id_clone = replay_session_id.clone();

        let consumer_task = tokio::spawn(async move {
            session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
        });

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id.clone(),
                event_type: "delegation_completed".to_string(),
                data: json!({
                    "delegation_id": "d1",
                    "result_summary": "test summary"
                }),
            })
            .unwrap();

        let msg = timeout(Duration::from_secs(1), msg_rx.recv())
            .await
            .expect("Timeout waiting for message")
            .expect("Should receive a message");

        match msg {
            ChatAppMsg::DelegationCompleted { id, summary } => {
                assert_eq!(id, "d1");
                assert_eq!(summary, "test summary");
            }
            other => panic!("Expected DelegationCompleted, got {:?}", other),
        }

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id,
                event_type: "replay_complete".to_string(),
                data: json!({}),
            })
            .unwrap();
        drop(event_tx);

        timeout(Duration::from_secs(1), consumer_task)
            .await
            .expect("Timeout waiting for consumer task")
            .expect("Consumer task should complete");
    }

    #[tokio::test]
    async fn replay_consumer_handles_delegation_failed() {
        use serde_json::json;
        use tokio::time::{timeout, Duration};

        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let replay_session_id = "test-session-delegation-failed".to_string();
        let session_id_clone = replay_session_id.clone();

        let consumer_task = tokio::spawn(async move {
            session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
        });

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id.clone(),
                event_type: "delegation_failed".to_string(),
                data: json!({
                    "delegation_id": "d1",
                    "error": "test failure"
                }),
            })
            .unwrap();

        let msg = timeout(Duration::from_secs(1), msg_rx.recv())
            .await
            .expect("Timeout waiting for message")
            .expect("Should receive a message");

        match msg {
            ChatAppMsg::DelegationFailed { id, error } => {
                assert_eq!(id, "d1");
                assert_eq!(error, "test failure");
            }
            other => panic!("Expected DelegationFailed, got {:?}", other),
        }

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: replay_session_id,
                event_type: "replay_complete".to_string(),
                data: json!({}),
            })
            .unwrap();
        drop(event_tx);

        timeout(Duration::from_secs(1), consumer_task)
            .await
            .expect("Timeout waiting for consumer task")
            .expect("Consumer task should complete");
    }

    // ─── Setup-event translation (Task 1.3) ─────────────────────────────

    #[test]
    fn translate_session_initialized_produces_payload_msg() {
        use serde_json::json;
        let data = json!({
            "model": "glm-5",
            "mode": "plan",
            "agent_name": "claude",
            "kiln_path": "/k",
            "workspace_path": "/w",
        });
        let msgs = session_event_to_chat_msgs("session_initialized", &data);
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            ChatAppMsg::SessionInitialized(p) => {
                assert_eq!(p.model, "glm-5");
                assert_eq!(p.mode, "plan");
                assert_eq!(p.agent_name.as_deref(), Some("claude"));
            }
            other => panic!("expected SessionInitialized, got {other:?}"),
        }
    }

    #[test]
    fn translate_providers_listed_carries_providers() {
        use serde_json::json;
        let data = json!({
            "providers": [{
                "name": "OpenAI", "provider_type": "openai", "available": true,
                "default_model": null, "models": [], "endpoint": null,
                "reason": null, "is_local": false,
            }],
        });
        let msgs = session_event_to_chat_msgs("providers_listed", &data);
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            ChatAppMsg::ProvidersListed(providers) => {
                assert_eq!(providers.len(), 1);
                assert_eq!(providers[0].name, "OpenAI");
            }
            other => panic!("expected ProvidersListed, got {other:?}"),
        }
    }

    #[test]
    fn translate_context_limit_resolved_parses_source() {
        use crucible_core::protocol::session_events::ContextLimitSource;
        use serde_json::json;
        let data = json!({ "limit": 128_000, "source": "provider_api" });
        let msgs = session_event_to_chat_msgs("context_limit_resolved", &data);
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            ChatAppMsg::ContextLimitResolved { limit, source } => {
                assert_eq!(*limit, 128_000);
                assert_eq!(*source, ContextLimitSource::ProviderApi);
            }
            other => panic!("expected ContextLimitResolved, got {other:?}"),
        }
    }

    #[test]
    fn translate_workspace_indexed_carries_files() {
        use serde_json::json;
        let data = json!({ "files": ["src/lib.rs", "README.md"] });
        let msgs = session_event_to_chat_msgs("workspace_indexed", &data);
        match msgs.as_slice() {
            [ChatAppMsg::WorkspaceIndexed(files)] => assert_eq!(
                files,
                &vec!["src/lib.rs".to_string(), "README.md".to_string()]
            ),
            other => panic!("expected WorkspaceIndexed, got {other:?}"),
        }
    }

    #[test]
    fn translate_kiln_notes_indexed_carries_notes() {
        use serde_json::json;
        let data = json!({ "notes": ["note:Daily.md"] });
        let msgs = session_event_to_chat_msgs("kiln_notes_indexed", &data);
        match msgs.as_slice() {
            [ChatAppMsg::KilnNotesIndexed(notes)] => {
                assert_eq!(notes, &vec!["note:Daily.md".to_string()])
            }
            other => panic!("expected KilnNotesIndexed, got {other:?}"),
        }
    }

    #[test]
    fn translate_plugins_discovered_carries_entries() {
        use serde_json::json;
        let data = json!({
            "plugins": [
                { "name": "kiln-expert", "version": "0.1.0", "state": "loaded", "error": null }
            ]
        });
        let msgs = session_event_to_chat_msgs("plugins_discovered", &data);
        match msgs.as_slice() {
            [ChatAppMsg::PluginsDiscovered(entries)] => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "kiln-expert");
                assert_eq!(entries[0].state, "loaded");
            }
            other => panic!("expected PluginsDiscovered, got {other:?}"),
        }
    }

    #[test]
    fn translate_mcp_servers_ready_maps_to_display_and_collapses_tools() {
        use serde_json::json;
        let data = json!({
            "servers": [
                {
                    "name": "context7",
                    "prefix": "c7_",
                    "tools": ["query-docs", "resolve-library-id"],
                    "connected": true,
                }
            ]
        });
        let msgs = session_event_to_chat_msgs("mcp_servers_ready", &data);
        match msgs.as_slice() {
            [ChatAppMsg::McpServersReady(servers)] => {
                assert_eq!(servers.len(), 1);
                assert_eq!(servers[0].name, "context7");
                // trailing `_` stripped to match legacy McpServerDisplay shape
                assert_eq!(servers[0].prefix, "c7");
                assert_eq!(servers[0].tool_count, 2);
                assert!(servers[0].connected);
            }
            other => panic!("expected McpServersReady, got {other:?}"),
        }
    }

    #[test]
    fn translate_bad_payload_shape_returns_empty() {
        use serde_json::json;
        // Missing required fields — the type-strict deserializer fails and the
        // translator returns an empty vec rather than panicking.
        let msgs = session_event_to_chat_msgs("context_limit_resolved", &json!({}));
        assert!(msgs.is_empty());
    }

    #[test]
    fn translate_unknown_event_returns_empty() {
        use serde_json::json;
        let msgs = session_event_to_chat_msgs("never_heard_of_it", &json!({}));
        assert!(msgs.is_empty());
    }

    #[test]
    fn translate_context_limit_resolved_updates_atomic_through_stream() {
        use serde_json::json;
        let limit = Arc::new(AtomicUsize::new(0));
        let mut stream = SessionEventStream::new().with_context_limit(limit.clone());
        let msgs = stream.translate(
            "context_limit_resolved",
            &json!({ "limit": 4096, "source": "config" }),
        );
        assert_eq!(msgs.len(), 1);
        assert_eq!(limit.load(Ordering::Relaxed), 4096);
    }

    /// `ended { reason: "error: ..." }` must promote to `ChatAppMsg::Error`
    /// through the unified consumer regardless of mode (live vs replay).
    /// This is the Task 2.5 invariant: replay of an error-ending recording
    /// surfaces the error identically to a live session that hit it.
    #[tokio::test]
    async fn consumer_promotes_ended_error_in_both_modes() {
        use serde_json::json;
        use tokio::time::{timeout, Duration};

        for context_limit in [None, Some(Arc::new(AtomicUsize::new(0)))] {
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (event_tx, event_rx) = mpsc::unbounded_channel();

            let session_id = "test-session-ended-error".to_string();
            let sid_clone = session_id.clone();
            let ctx_limit = context_limit.clone();

            let consumer = tokio::spawn(async move {
                session_event_consumer(sid_clone, event_rx, msg_tx, ctx_limit).await;
            });

            event_tx
                .send(crucible_daemon::SessionEvent {
                    session_id: session_id.clone(),
                    event_type: "ended".to_string(),
                    data: json!({ "reason": "error: LLM timeout" }),
                })
                .unwrap();
            drop(event_tx);

            let msg = timeout(Duration::from_secs(1), msg_rx.recv())
                .await
                .expect("timely")
                .expect("some msg");
            match msg {
                ChatAppMsg::Error(s) => assert_eq!(s, "LLM timeout"),
                other => panic!("expected Error, got {:?}", other),
            }

            consumer.abort();
        }
    }
}
