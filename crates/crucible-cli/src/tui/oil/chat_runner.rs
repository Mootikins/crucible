use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{
    ChatAppMsg, ChatItem, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry,
};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::terminal::Terminal;
use crate::tui::oil::theme::ThemeTokens;
use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyModifiers};
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult, SubagentEventType};
use crucible_lua::SessionCommand;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::context_enricher::ContextEnricher;

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
    enricher: Option<Arc<ContextEnricher>>,
}

impl OilChatRunner {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminal: Terminal::new()?,
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
            enricher: None,
        })
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

    pub fn with_enricher(mut self, enricher: Arc<ContextEnricher>) -> Self {
        self.enricher = Some(enricher);
        self
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
        let ctx =
            ViewContext::with_terminal_size(&self.focus, ThemeTokens::default_ref(), terminal_size);
        let tree = app.view(&ctx);
        let _ = self.terminal.render(&tree)?;

        let selection = self.discover_agent().await;
        let mut agent = create_agent(selection).await?;

        app.set_status("Ready");

        let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();

        // Connect to MCP servers in background and update display
        if !self.mcp_servers.is_empty() {
            if let Some(ref mcp_config) = self.mcp_config {
                let mcp_config = mcp_config.clone();
                let mcp_tx = msg_tx.clone();
                tokio::spawn(async move {
                    use crucible_tools::mcp_gateway::McpGatewayManager;
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
                });
            }
        }

        let interaction_rx = agent.take_interaction_receiver();
        tracing::debug!(
            has_rx = interaction_rx.is_some(),
            "take_interaction_receiver"
        );

        self.event_loop(&mut app, &mut agent, bridge, msg_tx, msg_rx, interaction_rx)
            .await?;

        self.terminal.exit()?;
        Ok(())
    }

    async fn event_loop<A: AgentHandle>(
        &mut self,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        msg_tx: mpsc::UnboundedSender<ChatAppMsg>,
        mut msg_rx: mpsc::UnboundedReceiver<ChatAppMsg>,
        mut interaction_rx: Option<
            mpsc::UnboundedReceiver<crucible_core::interaction::InteractionEvent>,
        >,
    ) -> Result<()> {
        let mut active_stream: Option<BoxStream<'static, ChatResult<ChatChunk>>> = None;
        let mut event_stream = EventStream::new();
        let mut tick_interval = tokio::time::interval(self.tick_rate);
        let mut session_cmd_rx = self.session_cmd_rx.take();

        loop {
            if app.take_needs_full_redraw() {
                self.terminal.force_full_redraw()?;
            }

            let terminal_size = self.terminal.size();
            let ctx = ViewContext::with_terminal_size(
                &self.focus,
                ThemeTokens::default_ref(),
                terminal_size,
            );
            let tree = app.view(&ctx);

            let graduated_keys = if app.has_shell_modal() {
                self.terminal.render_fullscreen(&tree)?
            } else {
                self.terminal.render(&tree)?
            };
            if !graduated_keys.is_empty() {
                app.mark_graduated(graduated_keys);
            }

            while let Ok(msg) = msg_rx.try_recv() {
                let mut action =
                    Self::process_message(&msg, app, agent, bridge, &mut active_stream);
                while let Action::Send(follow_up) = action {
                    action =
                        Self::process_message(&follow_up, app, agent, bridge, &mut active_stream);
                }
                if action.is_quit() {
                    return Ok(());
                }
            }

            let event = tokio::select! {
                biased;

                event_opt = event_stream.next() => {
                    match event_opt {
                        Some(Ok(ct_event)) => {
                            tracing::trace!(?ct_event, "received crossterm event");
                            Some(self.convert_event(ct_event)?)
                        },
                        Some(Err(e)) => return Err(e.into()),
                        None => {
                            tracing::warn!("EventStream returned None - stream ended");
                            return Ok(());
                        }
                    }
                }

                Some(chunk_result) = async {
                    match &mut active_stream {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
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
                                    && msg_tx.send(ChatAppMsg::ThinkingDelta(reasoning.clone())).is_err()
                                {
                                    tracing::warn!("UI channel closed, ThinkingDelta dropped");
                                }
                            }

                            if let Some(ref tool_calls) = chunk.tool_calls {
                                for tc in tool_calls {
                                    let args_val = tc.arguments.clone().unwrap_or_default();
                                    if msg_tx.send(ChatAppMsg::ToolCall {
                                        name: tc.name.clone(),
                                        args: args_val.to_string(),
                                        call_id: tc.id.clone(),
                                    }).is_err() {
                                        tracing::warn!(tool = %tc.name, "UI channel closed, ToolCall dropped");
                                    }
                                }
                            }

                            if let Some(ref tool_results) = chunk.tool_results {
                                for tr in tool_results {
                                    if let Some(ref error) = tr.error {
                                        let _ = msg_tx.send(ChatAppMsg::ToolResultError {
                                            name: tr.name.clone(),
                                            error: error.clone(),
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
                                            error: event.error.clone().unwrap_or_else(|| "Unknown error".to_string()),
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
                                if msg_tx.send(ChatAppMsg::ContextUsage {
                                    used: usage.total_tokens as usize,
                                    total: self.context_limit,
                                }).is_err() {
                                    tracing::warn!("UI channel closed, ContextUsage dropped");
                                }
                            }

                            if chunk.done {
                                active_stream = None;
                                if msg_tx.send(ChatAppMsg::StreamComplete).is_err() {
                                    tracing::warn!("UI channel closed, StreamComplete dropped");
                                }
                            }
                        }
                        Err(e) => {
                            active_stream = None;
                            if msg_tx.send(ChatAppMsg::Error(e.to_string())).is_err() {
                                tracing::warn!("UI channel closed, Error dropped");
                            }
                        }
                    }
                    None
                }

                _ = tick_interval.tick() => {
                    tracing::trace!("tick");
                    Some(Event::Tick)
                }

                Some(cmd) = async {
                    match &mut session_cmd_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    Self::handle_session_command(cmd, agent, app).await;
                    None
                }

                Some(interaction_event) = async {
                    match &mut interaction_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
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
                        let _ = app.on_message(msg);
                    }
                    None
                }
            };

            if let Some(ev) = event {
                let action = app.update(ev.clone());
                tracing::trace!(?ev, ?action, "processed event");
                if self
                    .process_action(action, app, agent, bridge, &mut active_stream, &msg_tx)
                    .await?
                {
                    tracing::trace!("quit action received, breaking loop");
                    break;
                }
            }
        }

        Ok(())
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
        AgentSelection::Internal
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
            _ => {}
        }
        app.on_message(msg.clone())
    }

    async fn process_action<A: AgentHandle>(
        &mut self,
        action: Action<ChatAppMsg>,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
    ) -> io::Result<bool> {
        match action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                match &msg {
                    ChatAppMsg::ClearHistory => {
                        if active_stream.is_some() {
                            if let Err(e) = agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream");
                            }
                            *active_stream = None;
                        }
                        agent.clear_history().await;
                        tracing::info!("New session started (history cleared)");
                    }
                    ChatAppMsg::StreamCancelled => {
                        if active_stream.is_some() {
                            if let Err(e) = agent.cancel().await {
                                tracing::warn!(error = %e, "Failed to cancel agent stream on daemon");
                            }
                            tracing::info!(
                                "Dropping active stream due to cancellation (from action)"
                            );
                            *active_stream = None;
                        }
                    }
                    ChatAppMsg::SwitchModel(model_id) => {
                        tracing::info!(model = %model_id, "Model switch requested");
                        match agent.switch_model(model_id).await {
                            Ok(()) => {
                                tracing::info!(model = %model_id, "Model switched successfully");
                            }
                            Err(e) => {
                                tracing::warn!(model = %model_id, error = %e, "Model switch not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::FetchModels => {
                        tracing::info!("Fetching available models");
                        let models = agent.fetch_available_models().await;
                        if models.is_empty() {
                            let _ = app.on_message(ChatAppMsg::ModelsFetchFailed(
                                "No models available".to_string(),
                            ));
                        } else {
                            tracing::info!(count = models.len(), "Models fetched successfully");
                            let _ = app.on_message(ChatAppMsg::ModelsLoaded(models));
                        }
                    }
                    ChatAppMsg::McpStatusLoaded(_) | ChatAppMsg::PluginStatusLoaded(_) => {
                        app.on_message(msg.clone());
                    }
                    ChatAppMsg::SetThinkingBudget(budget) => {
                        tracing::info!(budget = budget, "Setting thinking budget");
                        match agent.set_thinking_budget(*budget).await {
                            Ok(()) => {
                                tracing::info!(budget = budget, "Thinking budget set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(budget = budget, error = %e, "Thinking budget not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetTemperature(temp) => {
                        tracing::info!(temperature = temp, "Setting temperature");
                        match agent.set_temperature(*temp).await {
                            Ok(()) => {
                                tracing::info!(temperature = temp, "Temperature set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(temperature = temp, error = %e, "Temperature not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::SetMaxTokens(max_tokens) => {
                        tracing::info!(max_tokens = ?max_tokens, "Setting max_tokens");
                        match agent.set_max_tokens(*max_tokens).await {
                            Ok(()) => {
                                tracing::info!(max_tokens = ?max_tokens, "Max tokens set successfully");
                            }
                            Err(e) => {
                                tracing::warn!(max_tokens = ?max_tokens, error = %e, "Max tokens not supported by this agent");
                            }
                        }
                    }
                    ChatAppMsg::CloseInteraction {
                        request_id,
                        response,
                    } => {
                        tracing::info!(request_id = %request_id, "Sending interaction response");
                        match agent
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
                        if let Err(e) = agent.set_mode_str(mode_id).await {
                            tracing::warn!(mode = %mode_id, error = %e, "Failed to set mode on agent");
                        }
                    }
                    ChatAppMsg::UserMessage(ref content) => {
                        if active_stream.is_none() {
                            if app.precognition()
                                && self.enricher.is_some()
                                && !content.starts_with("/search")
                            {
                                let enricher =
                                    self.enricher.clone().expect("checked is_some above");
                                let content = content.clone();
                                let top_k = app.precognition_results();
                                let tx = msg_tx.clone();
                                tokio::spawn(async move {
                                    match enricher.enrich_with_results_n(&content, top_k).await {
                                        Ok(result) => {
                                            let notes_count = result.notes_found.len();
                                            if notes_count > 0 {
                                                let _ = tx.send(ChatAppMsg::PrecognitionResult {
                                                    notes_count,
                                                });
                                            }
                                            let _ = tx.send(ChatAppMsg::EnrichedMessage {
                                                original: content,
                                                enriched: result.prompt,
                                            });
                                        }
                                        Err(e) => {
                                            tracing::warn!("Precognition enrichment failed: {}", e);
                                            let _ = tx.send(ChatAppMsg::EnrichedMessage {
                                                original: content.clone(),
                                                enriched: content,
                                            });
                                        }
                                    }
                                });
                            } else {
                                bridge.ring.push(SessionEvent::MessageReceived {
                                    content: content.clone(),
                                    participant_id: "user".to_string(),
                                });
                                let stream = agent.send_message_stream(content.clone());
                                *active_stream = Some(stream);
                            }
                        }
                    }
                    ChatAppMsg::ReloadPlugin(ref name) => {
                        tracing::info!(plugin = %name, "Plugin reload requested");
                        let name = name.clone();
                        let tx = msg_tx.clone();
                        tokio::spawn(async move {
                            match crucible_rpc::DaemonClient::connect().await {
                                Ok(client) => match client.plugin_reload(&name).await {
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
                                            "Reloaded '{}' ({} tools, {} services)",
                                            name, tools, services
                                        )));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(ChatAppMsg::Error(format!(
                                            "Plugin reload failed: {}",
                                            e
                                        )));
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.send(ChatAppMsg::Error(format!(
                                        "Cannot connect to daemon: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    ChatAppMsg::ExecuteSlashCommand(ref cmd) => {
                        tracing::info!(command = %cmd, "Forwarding slash command as user message");
                        let stream = agent.send_message_stream(cmd.clone());
                        *active_stream = Some(stream);
                    }
                    ChatAppMsg::ExportSession(ref export_path) => {
                        let session_dir = match app.session_dir() {
                            Some(dir) => dir.to_path_buf(),
                            None => {
                                app.on_message(ChatAppMsg::Error(
                                    "Export failed: no active session".to_string(),
                                ));
                                return Ok(false);
                            }
                        };

                        match crucible_observe::load_events(&session_dir).await {
                            Ok(events) if events.is_empty() => {
                                app.on_message(ChatAppMsg::Error(
                                    "Nothing to export — session has no recorded events"
                                        .to_string(),
                                ));
                            }
                            Ok(events) => {
                                let options = crucible_observe::RenderOptions::default();
                                let md = crucible_observe::render_to_markdown(&events, &options);
                                match tokio::fs::write(&export_path, &md).await {
                                    Ok(_) => {
                                        app.add_system_message(format!(
                                            "Session exported to {}",
                                            export_path.display()
                                        ));
                                    }
                                    Err(e) => {
                                        app.on_message(ChatAppMsg::Error(format!(
                                            "Export failed: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Err(e) => {
                                app.on_message(ChatAppMsg::Error(format!(
                                    "Failed to load session events: {}",
                                    e
                                )));
                            }
                        }
                    }
                    _ => {}
                }
                let action = app.on_message(msg);
                Box::pin(self.process_action(action, app, agent, bridge, active_stream, msg_tx))
                    .await
            }
            Action::Batch(actions) => {
                for action in actions {
                    if Box::pin(self.process_action(
                        action,
                        app,
                        agent,
                        bridge,
                        active_stream,
                        msg_tx,
                    ))
                    .await?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
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
            _ => None,
        }
    }
}
