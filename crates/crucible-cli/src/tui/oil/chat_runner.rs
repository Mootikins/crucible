use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{
    ChatAppMsg, ChatItem, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry,
};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::terminal::Terminal;
use crate::tui::oil::theme;
use anyhow::{Context, Result};
#[allow(unused_imports)] // WIP: KeyCode, KeyModifiers not yet used
use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyModifiers};
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult, SubagentEventType};
use crucible_lua::SessionCommand;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::tui::oil::commands::{SetEffect, SetRpcAction};

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
    pub active_stream: &'a mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for handle_select_outcome function.
struct HandleSelectOutcomeParams<'a, A: AgentHandle> {
    pub select_outcome: EventLoopSelectOutcome,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub active_stream: &'a mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for process_action function.
struct ProcessActionParams<'a, A: AgentHandle> {
    pub action: Action<ChatAppMsg>,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub active_stream: &'a mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

pub struct OilChatRunner {
    terminal: Terminal,
    tick_rate: Duration,
    mode: ChatMode,
    model: String,
    context_limit: usize,
    focus: FocusContext,
    workspace_files: Vec<String>,
    kiln_notes: Vec<String>,
    session_dir: Option<PathBuf>,
    resume_session_id: Option<String>,
    resume_history: Option<Vec<ChatItem>>,
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
            context_limit: 0,
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
        self.context_limit = limit;
        self
    }

    pub fn with_mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_workspace_files(mut self, files: Vec<String>) -> Self {
        self.workspace_files = files;
        self
    }

    pub fn with_kiln_notes(mut self, notes: Vec<String>) -> Self {
        self.kiln_notes = notes;
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

    pub fn with_resume_history(mut self, history: Vec<ChatItem>) -> Self {
        self.resume_history = Some(history);
        self
    }

    pub fn with_mcp_servers(mut self, servers: Vec<McpServerDisplay>) -> Self {
        self.mcp_servers = servers;
        self
    }

    pub fn with_plugin_status(mut self, entries: Vec<PluginStatusEntry>) -> Self {
        self.plugin_status = entries;
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

    #[cfg(test)]
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
        Fut: std::future::Future<Output = Result<A>>,
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

        // Hydrate viewport with conversation history from a resumed session
        if let Some(history) = self.resume_history.take() {
            if !history.is_empty() {
                tracing::info!(
                    count = history.len(),
                    "Loading resume history into viewport"
                );
                app.load_previous_messages(history);
            }
        }

        let terminal_size = self.terminal.size();
        let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
        let tree = app.view(&ctx);
        let _ = self.terminal.render(&tree)?;

        let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();
        let mut background_tasks: Vec<JoinHandle<()>> = Vec::new();

        if let Some(replay_path) = self.replay_path.clone() {
            let (mut agent, replay_session_id, event_rx) =
                crate::factories::create_daemon_replay_agent(&replay_path, self.replay_speed)
                    .await?;
            let user_msgs = extract_user_messages_from_recording(&replay_path)?;

            tracing::info!(
                session_id = %replay_session_id,
                speed = self.replay_speed,
                "Connected to daemon replay session"
            );

            self.is_replay = true;
            self.replay_remaining_completes = user_msgs.len().max(1);
            app.set_precognition(false);
            app.set_status("Replay");

            if let Some((first, rest)) = user_msgs.split_first() {
                let _ = msg_tx.send(ChatAppMsg::UserMessage(first.clone()));
                for msg in rest {
                    let _ = msg_tx.send(ChatAppMsg::QueueMessage(msg.clone()));
                }
            }

            let msg_tx_clone = msg_tx.clone();
            background_tasks.push(tokio::spawn(replay_event_consumer(
                replay_session_id,
                event_rx,
                msg_tx_clone,
            )));

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
            event_loop_result?;

            self.terminal.exit()?;
            return Ok(());
        }

        let selection = self.discover_agent().await;
        let mut agent = create_agent(selection).await?;
        self.is_replay = false;
        self.replay_remaining_completes = 0;

        app.set_status("Ready");

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

        // Connect to MCP servers in background and update display
        if !self.mcp_servers.is_empty() {
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
        }

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
        event_loop_result?;

        self.terminal.exit()?;
        Ok(())
    }

    async fn event_loop<A: AgentHandle>(
        &mut self,
        mut params: EventLoopParams<'_, A>,
    ) -> Result<()> {
        let mut active_stream: Option<BoxStream<'static, ChatResult<ChatChunk>>> = None;
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

            match self.drain_phase_outcome(
                params.app,
                params.agent,
                params.bridge,
                &mut active_stream,
                &mut params.msg_rx,
                &mut replay_auto_exit_deadline,
            ) {
                DrainPhaseOutcome::Quit => return Ok(()),
                DrainPhaseOutcome::Continue => continue,
                DrainPhaseOutcome::Wait => {}
            }

            let select_outcome = tokio::select! {
                biased;

                event_opt = event_stream.next() => {
                    self.handle_terminal_event(event_opt)?
                }

                Some(chunk_result) = Self::next_active_chunk(&mut active_stream) => {
                    self.handle_stream_chunk(chunk_result, &mut active_stream, &params.msg_tx);
                    EventLoopSelectOutcome::Continue
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
                            active_stream: &mut active_stream,
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
                    active_stream: &mut active_stream,
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

    fn drain_phase_outcome<A: AgentHandle>(
        &mut self,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
        msg_rx: &mut mpsc::UnboundedReceiver<ChatAppMsg>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> DrainPhaseOutcome {
        let drain_outcome = self.drain_pending_messages(
            app,
            agent,
            bridge,
            active_stream,
            msg_rx,
            replay_auto_exit_deadline,
        );

        if drain_outcome == DrainMessagesOutcome::Quit {
            return DrainPhaseOutcome::Quit;
        }
        if !Self::should_wait_for_event(drain_outcome) {
            return DrainPhaseOutcome::Continue;
        }

        DrainPhaseOutcome::Wait
    }

    fn render_app_frame(&mut self, app: &mut OilChatApp) -> Result<()> {
        if app.take_needs_full_redraw() {
            self.terminal.force_full_redraw()?;
        }

        let terminal_size = self.terminal.size();
        let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
        let tree = app.view(&ctx);

        let graduated_keys = if app.has_shell_modal() {
            self.terminal.render_fullscreen(&tree)?
        } else {
            self.terminal.render(&tree)?
        };
        if !graduated_keys.is_empty() {
            app.mark_graduated(graduated_keys);
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
                active_stream: params.active_stream,
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
            active_stream: params.active_stream,
            msg_tx: params.msg_tx,
            background_tasks: params.background_tasks,
        })
        .await
    }

    fn handle_stream_chunk(
        &self,
        chunk_result: ChatResult<ChatChunk>,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
    ) {
        match chunk_result {
            Ok(chunk) => {
                tracing::debug!(
                    delta_len = chunk.delta.len(),
                    done = chunk.done,
                    has_tool_calls = chunk.tool_calls.is_some(),
                    has_tool_results = chunk.tool_results.is_some(),
                    "Received chunk"
                );

                if !chunk.delta.is_empty()
                    && msg_tx.send(ChatAppMsg::TextDelta(chunk.delta)).is_err()
                {
                    tracing::warn!("UI channel closed, TextDelta dropped");
                }

                if let Some(ref reasoning) = chunk.reasoning {
                    if !reasoning.is_empty()
                        && msg_tx
                            .send(ChatAppMsg::ThinkingDelta(reasoning.clone()))
                            .is_err()
                    {
                        tracing::warn!("UI channel closed, ThinkingDelta dropped");
                    }
                }

                if let Some(ref tool_calls) = chunk.tool_calls {
                    for tc in tool_calls {
                        let args_str = match &tc.arguments {
                            Some(v) if !v.is_null() => v.to_string(),
                            _ => String::new(),
                        };
                        if msg_tx
                            .send(ChatAppMsg::ToolCall {
                                name: tc.name.clone(),
                                args: args_str,
                                call_id: tc.id.clone(),
                                description: None,
                                source: None,
                                lua_primary_arg: None,
                            })
                            .is_err()
                        {
                            tracing::warn!(tool = %tc.name, "UI channel closed, ToolCall dropped");
                        }
                    }
                }

                if let Some(ref tool_results) = chunk.tool_results {
                    for tr in tool_results {
                        if let Some(ref error) = tr.error {
                            let cleaned =
                                crucible_core::error_utils::strip_tool_error_prefix(error);
                            let _ = msg_tx.send(ChatAppMsg::ToolResultError {
                                name: tr.name.clone(),
                                error: cleaned,
                                call_id: tr.call_id.clone(),
                            });
                        } else if tr.result.starts_with("Error: ") {
                            let cleaned = crucible_core::error_utils::strip_tool_error_prefix(
                                tr.result.strip_prefix("Error: ").unwrap_or(&tr.result),
                            );
                            let _ = msg_tx.send(ChatAppMsg::ToolResultError {
                                name: tr.name.clone(),
                                error: cleaned,
                                call_id: tr.call_id.clone(),
                            });
                        } else {
                            if !tr.result.is_empty() {
                                let _ = msg_tx.send(ChatAppMsg::ToolResultDelta {
                                    name: tr.name.clone(),
                                    delta: tr.result.clone(),
                                    call_id: tr.call_id.clone(),
                                });
                            }
                            let _ = msg_tx.send(ChatAppMsg::ToolResultComplete {
                                name: tr.name.clone(),
                                call_id: tr.call_id.clone(),
                            });
                        }
                    }
                }

                if let Some(ref subagent_events) = chunk.subagent_events {
                    for event in subagent_events {
                        let msg = match event.event_type {
                            SubagentEventType::Spawned => ChatAppMsg::SubagentSpawned {
                                id: event.id.clone(),
                                prompt: event.prompt.clone().unwrap_or_default(),
                            },
                            SubagentEventType::Completed => ChatAppMsg::SubagentCompleted {
                                id: event.id.clone(),
                                summary: event.summary.clone().unwrap_or_default(),
                            },
                            SubagentEventType::Failed => ChatAppMsg::SubagentFailed {
                                id: event.id.clone(),
                                error: event
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| "Unknown error".to_string()),
                            },
                        };
                        if msg_tx.send(msg).is_err() {
                            tracing::warn!(
                                id = %event.id,
                                event_type = ?event.event_type,
                                "UI channel closed, SubagentEvent dropped"
                            );
                        }
                    }
                }

                if let Some(ref usage) = chunk.usage {
                    if msg_tx
                        .send(ChatAppMsg::ContextUsage {
                            used: usage.total_tokens as usize,
                            total: self.context_limit,
                        })
                        .is_err()
                    {
                        tracing::warn!("UI channel closed, ContextUsage dropped");
                    }
                }

                if let Some(notes_count) = chunk.precognition_notes_count {
                    let notes = chunk.precognition_notes.unwrap_or_default();
                    if notes_count > 0
                        && msg_tx
                            .send(ChatAppMsg::PrecognitionResult { notes_count, notes })
                            .is_err()
                    {
                        tracing::warn!("UI channel closed, PrecognitionResult dropped");
                    }
                }

                if chunk.done {
                    *active_stream = None;
                    if msg_tx.send(ChatAppMsg::StreamComplete).is_err() {
                        tracing::warn!("UI channel closed, StreamComplete dropped");
                    }
                }
            }
            Err(e) => {
                *active_stream = None;
                if msg_tx.send(ChatAppMsg::Error(e.to_string())).is_err() {
                    tracing::warn!("UI channel closed, Error dropped");
                }
            }
        }
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

    async fn next_active_chunk(
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    ) -> Option<ChatResult<ChatChunk>> {
        match active_stream {
            Some(stream) => stream.next().await,
            None => std::future::pending().await,
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

    fn process_message<A: AgentHandle>(
        msg: &ChatAppMsg,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    ) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(content) => {
                if active_stream.is_none() && !app.precognition() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: content.clone(),
                        participant_id: "user".to_string(),
                    });
                    let stream = agent.send_message_stream(content.clone());
                    *active_stream = Some(stream);
                }
            }
            ChatAppMsg::EnrichedMessage {
                original, enriched, ..
            } => {
                if active_stream.is_none() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: original.clone(),
                        participant_id: "user".to_string(),
                    });
                    let stream = agent.send_message_stream(enriched.clone());
                    *active_stream = Some(stream);
                }
            }
            ChatAppMsg::FetchModels => {
                tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "drain_pending_messages: received FetchModels");
            }
            _ => {}
        }
        app.on_message(msg.clone())
    }

    fn drain_pending_messages<A: AgentHandle>(
        &mut self,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
        msg_rx: &mut mpsc::UnboundedReceiver<ChatAppMsg>,
        replay_auto_exit_deadline: &mut Option<tokio::time::Instant>,
    ) -> DrainMessagesOutcome {
        let mut processed_any = false;

        while let Ok(msg) = msg_rx.try_recv() {
            processed_any = true;

            if self.is_replay {
                if matches!(msg, ChatAppMsg::Status(ref s) if s == "Replay complete") {
                    self.replay_remaining_completes = 0;
                    if self.replay_auto_exit.is_some() {
                        *replay_auto_exit_deadline = Some(tokio::time::Instant::now());
                    }
                }
                let action = app.on_message(msg);
                if action.is_quit() {
                    return DrainMessagesOutcome::Quit;
                }
                continue;
            }

            let mut action = Self::process_message(&msg, app, agent, bridge, active_stream);
            while let Action::Send(follow_up) = action {
                action = Self::process_message(&follow_up, app, agent, bridge, active_stream);
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
                        if params.active_stream.is_some() {
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
                        if params.active_stream.is_some() {
                            if let Err(e) = params.agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream");
                            }
                            *params.active_stream = None;
                        }
                        params.agent.clear_history().await;
                        params.app.reset_session();
                        tracing::info!("New session started (history cleared)");
                    }
                    ChatAppMsg::StreamCancelled => {
                        if params.active_stream.is_some() {
                            if let Err(e) = params.agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream on daemon");
                            }
                            tracing::info!(
                                "Dropping active stream due to cancellation (from action)"
                            );
                            *params.active_stream = None;
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
                    ChatAppMsg::FetchModels => {
                        if self.is_acp_session() {
                            params.app.add_notification(
                                crucible_core::types::Notification::warning(
                                    "Model listing not available for ACP agents".to_string(),
                                ),
                            );
                            return Ok(false);
                        }
                        tracing::debug!(target: "crucible_cli::tui::oil::model_flow", "process_action: FetchModels starting async fetch");
                        let models = params.agent.fetch_available_models().await;
                        tracing::debug!(target: "crucible_cli::tui::oil::model_flow", count = models.len(), "process_action: fetch_available_models returned");
                        if models.is_empty() {
                            let _ = params.app.on_message(ChatAppMsg::ModelsFetchFailed(
                                "No models available".to_string(),
                            ));
                        } else {
                            tracing::info!(count = models.len(), "Models fetched successfully");
                            let _ = params.app.on_message(ChatAppMsg::ModelsLoaded(models));
                        }
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
                        if params.active_stream.is_none() {
                            params.bridge.ring.push(SessionEvent::MessageReceived {
                                content: content.clone(),
                                participant_id: "user".to_string(),
                            });
                            let stream = params.agent.send_message_stream(content.clone());
                            *params.active_stream = Some(stream);
                        }
                    }
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
                    ChatAppMsg::ExecuteSlashCommand(ref cmd) if !self.is_replay => {
                        tracing::info!(command = %cmd, "Forwarding slash command as user message");
                        let stream = params.agent.send_message_stream(cmd.clone());
                        *params.active_stream = Some(stream);
                    }
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
                    ChatAppMsg::ReloadPlugin(_)
                    | ChatAppMsg::ExecuteSlashCommand(_)
                    | ChatAppMsg::ExportSession(_) => {}
                    _ => {}
                }
                let action = params.app.on_message(msg);
                Box::pin(self.process_action(ProcessActionParams {
                    action,
                    app: params.app,
                    agent: params.agent,
                    bridge: params.bridge,
                    active_stream: params.active_stream,
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
                        active_stream: params.active_stream,
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

fn extract_user_messages_from_recording(path: &std::path::Path) -> Result<Vec<String>> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open replay file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let Some(header_line) = lines.next() else {
        return Ok(Vec::new());
    };

    let _header_line = header_line
        .with_context(|| format!("Failed reading replay header line from {}", path.display()))?;

    let mut user_messages = Vec::new();
    for line in lines {
        let line = line
            .with_context(|| format!("Failed reading replay event line from {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            if event.get("event").and_then(|v| v.as_str()) == Some("user_message") {
                if let Some(content) = event
                    .get("data")
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    user_messages.push(content.to_string());
                }
            }
        }
    }

    Ok(user_messages)
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
            let description = data
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
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
                    error: err.to_string(),
                    call_id,
                }]
            } else {
                let result_str = result_data
                    .and_then(|r| r.get("result"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
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
        "message_complete" => vec![ChatAppMsg::StreamComplete],
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
        "replay_complete" => vec![],
        _ => {
            tracing::trace!(event_type = %event_type, "Skipping unknown session event");
            vec![]
        }
    }
}

pub(crate) async fn replay_event_consumer(
    replay_session_id: String,
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
) {
    while let Some(event) = event_rx.recv().await {
        if event.session_id != replay_session_id {
            continue;
        }
        if event.event_type == "replay_complete" {
            let _ = msg_tx.send(ChatAppMsg::Status("Replay complete".to_string()));
            return;
        }
        for msg in session_event_to_chat_msgs(&event.event_type, &event.data) {
            if msg_tx.send(msg).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::chat_app::ModelListState;
    use async_trait::async_trait;
    use crucible_core::events::EventRing;
    use crucible_core::traits::chat::{ChatError, ChatResult};
    use futures::stream::{self, BoxStream};
    use std::sync::Arc;

    struct EmptyAgent;

    struct ModelsAgent {
        models: Vec<String>,
    }

    #[async_trait]
    impl AgentHandle for EmptyAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            Box::pin(stream::empty())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn get_mode_id(&self) -> &str {
            "normal"
        }

        async fn cancel(&self) -> ChatResult<()> {
            Ok(())
        }

        async fn clear_history(&mut self) {}

        async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
            Ok(())
        }

        async fn fetch_available_models(&mut self) -> Vec<String> {
            Vec::new()
        }

        async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
            Err(ChatError::NotSupported("set_thinking_budget".to_string()))
        }

        fn get_thinking_budget(&self) -> Option<i64> {
            None
        }

        async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
            Ok(())
        }

        fn get_temperature(&self) -> Option<f64> {
            None
        }

        async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
            Ok(())
        }

        fn get_max_tokens(&self) -> Option<u32> {
            None
        }
    }

    #[async_trait]
    impl AgentHandle for ModelsAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            Box::pin(stream::empty())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn get_mode_id(&self) -> &str {
            "normal"
        }

        async fn cancel(&self) -> ChatResult<()> {
            Ok(())
        }

        async fn clear_history(&mut self) {}

        async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
            Ok(())
        }

        async fn fetch_available_models(&mut self) -> Vec<String> {
            self.models.clone()
        }

        async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
            Err(ChatError::NotSupported("set_thinking_budget".to_string()))
        }

        fn get_thinking_budget(&self) -> Option<i64> {
            None
        }

        async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
            Ok(())
        }

        fn get_temperature(&self) -> Option<f64> {
            None
        }

        async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
            Ok(())
        }

        fn get_max_tokens(&self) -> Option<u32> {
            None
        }
    }

    #[test]
    fn drain_pending_messages_marks_user_turn_active() {
        let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
        let mut app = OilChatApp::default();
        let mut agent = EmptyAgent;
        let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let mut active_stream = None;
        let mut replay_deadline = None;

        msg_tx
            .send(ChatAppMsg::UserMessage("show spinner".to_string()))
            .unwrap();

        let outcome = runner.drain_pending_messages(
            &mut app,
            &mut agent,
            &bridge,
            &mut active_stream,
            &mut msg_rx,
            &mut replay_deadline,
        );

        assert_eq!(outcome, DrainMessagesOutcome::Processed);
        assert!(app.is_streaming());
    }

    #[test]
    fn processed_messages_should_not_wait_for_next_event() {
        assert!(
            !OilChatRunner::should_wait_for_event(DrainMessagesOutcome::Processed),
            "Processed messages should trigger immediate rerender"
        );
    }

    #[test]
    fn non_acp_init_prefetch_moves_model_state_out_of_not_loaded() {
        let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
        let mut app = OilChatApp::default();
        let mut agent = ModelsAgent {
            models: vec!["ollama/llama3".to_string()],
        };
        let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let mut active_stream = None;
        let mut replay_deadline = None;

        runner.queue_model_prefetch(&msg_tx);
        let _ = runner.drain_pending_messages(
            &mut app,
            &mut agent,
            &bridge,
            &mut active_stream,
            &mut msg_rx,
            &mut replay_deadline,
        );

        assert!(
            matches!(
                app.model_list_state(),
                ModelListState::Loading | ModelListState::Loaded
            ),
            "non-ACP init should prefetch models (state should be Loading or Loaded, got {:?})",
            app.model_list_state()
        );
    }

    #[test]
    fn acp_init_prefetch_is_skipped_and_state_stays_not_loaded() {
        let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24))
            .with_agent_name(Some("claude".to_string()));
        let mut app = OilChatApp::default();
        let mut agent = ModelsAgent {
            models: vec!["ollama/llama3".to_string()],
        };
        let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let mut active_stream = None;
        let mut replay_deadline = None;

        runner.queue_model_prefetch(&msg_tx);
        let _ = runner.drain_pending_messages(
            &mut app,
            &mut agent,
            &bridge,
            &mut active_stream,
            &mut msg_rx,
            &mut replay_deadline,
        );

        assert_eq!(
            app.model_list_state(),
            &ModelListState::NotLoaded,
            "ACP init should skip model prefetch"
        );
    }

    #[tokio::test]
    async fn replay_consumer_handles_delegation_spawned() {
        use serde_json::json;
        use tokio::time::{timeout, Duration};

        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let replay_session_id = "test-session-delegation-spawned".to_string();
        let session_id_clone = replay_session_id.clone();

        let consumer_task = tokio::spawn(async move {
            replay_event_consumer(session_id_clone, event_rx, msg_tx).await;
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
            replay_event_consumer(session_id_clone, event_rx, msg_tx).await;
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
            replay_event_consumer(session_id_clone, event_rx, msg_tx).await;
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
}
