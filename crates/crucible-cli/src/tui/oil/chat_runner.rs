use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::agent_selection::AgentSelection;
use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, InkChatApp, McpServerDisplay};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::terminal::Terminal;
use anyhow::Result;
use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyModifiers};
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult};
use futures::stream::BoxStream;
use futures::StreamExt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct InkChatRunner {
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
    mcp_servers: Vec<McpServerDisplay>,
    available_models: Vec<String>,
    show_thinking: bool,
}

impl InkChatRunner {
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
            mcp_servers: Vec::new(),
            available_models: Vec::new(),
            show_thinking: false,
        })
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

    pub fn with_mcp_servers(mut self, servers: Vec<McpServerDisplay>) -> Self {
        self.mcp_servers = servers;
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

        let mut app = InkChatApp::default();
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
        if !self.available_models.is_empty() {
            app.set_available_models(std::mem::take(&mut self.available_models));
        }
        app.set_show_thinking(self.show_thinking);

        let ctx = ViewContext::new(&self.focus);
        let tree = app.view(&ctx);
        self.terminal.render(&tree)?;

        let selection = self.discover_agent().await;
        let mut agent = create_agent(selection).await?;

        app.set_status("Ready");

        let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();

        self.event_loop(&mut app, &mut agent, bridge, msg_tx, msg_rx)
            .await?;

        self.terminal.exit()?;
        Ok(())
    }

    async fn event_loop<A: AgentHandle>(
        &mut self,
        app: &mut InkChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        msg_tx: mpsc::UnboundedSender<ChatAppMsg>,
        mut msg_rx: mpsc::UnboundedReceiver<ChatAppMsg>,
    ) -> Result<()> {
        let mut active_stream: Option<BoxStream<'static, ChatResult<ChatChunk>>> = None;
        let mut event_stream = EventStream::new();
        let mut tick_interval = tokio::time::interval(self.tick_rate);

        loop {
            if app.take_needs_full_redraw() {
                self.terminal.force_full_redraw()?;
            }

            let ctx = ViewContext::new(&self.focus);
            let tree = app.view(&ctx);

            if app.has_shell_modal() {
                self.terminal.render_fullscreen(&tree)?;
            } else {
                self.terminal.render(&tree)?;
            }

            while let Ok(msg) = msg_rx.try_recv() {
                let action = Self::process_message(&msg, app, agent, bridge, &mut active_stream);
                if action.is_quit() {
                    return Ok(());
                }
                if let Action::Send(follow_up) = action {
                    let follow_action =
                        Self::process_message(&follow_up, app, agent, bridge, &mut active_stream);
                    if follow_action.is_quit() {
                        return Ok(());
                    }
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
                                    }).is_err() {
                                        tracing::warn!(tool = %tc.name, "UI channel closed, ToolCall dropped");
                                    }
                                }
                            }

                            if let Some(ref tool_results) = chunk.tool_results {
                                for tr in tool_results {
                                    if !tr.result.is_empty()
                                        && msg_tx
                                            .send(ChatAppMsg::ToolResultDelta {
                                                name: tr.name.clone(),
                                                delta: tr.result.clone(),
                                            })
                                            .is_err()
                                    {
                                        tracing::warn!(tool = %tr.name, "UI channel closed, ToolResultDelta dropped");
                                    }
                                    if msg_tx.send(ChatAppMsg::ToolResultComplete {
                                        name: tr.name.clone(),
                                    }).is_err() {
                                        tracing::warn!(tool = %tr.name, "UI channel closed, ToolResultComplete dropped");
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
            };

            if let Some(ev) = event {
                if let Event::Key(key) = &ev {
                    if key.code == crossterm::event::KeyCode::Enter && active_stream.is_none() {
                        let content = app.input_content().to_string();
                        let trimmed = content.trim();
                        if !trimmed.is_empty()
                            && !trimmed.starts_with('/')
                            && !trimmed.starts_with(':')
                            && !trimmed.starts_with('!')
                        {
                            bridge.ring.push(SessionEvent::MessageReceived {
                                content: content.clone(),
                                participant_id: "user".to_string(),
                            });

                            let stream = agent.send_message_stream(content);
                            active_stream = Some(stream);
                        }
                    }
                }

                let action = app.update(ev.clone());
                tracing::trace!(?ev, ?action, "processed event");
                if self
                    .process_action(action, app, agent, &mut active_stream)
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
        app: &mut InkChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    ) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::ClearHistory => {
                agent.clear_history();
                tracing::info!("Conversation history cleared");
            }
            ChatAppMsg::StreamCancelled => {
                if active_stream.is_some() {
                    tracing::info!("Dropping active stream due to cancellation");
                    *active_stream = None;
                }
            }
            ChatAppMsg::UserMessage(content) => {
                if active_stream.is_none() {
                    bridge.ring.push(SessionEvent::MessageReceived {
                        content: content.clone(),
                        participant_id: "user".to_string(),
                    });
                    let stream = agent.send_message_stream(content.clone());
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
        app: &mut InkChatApp,
        agent: &mut A,
        active_stream: &mut Option<BoxStream<'static, ChatResult<ChatChunk>>>,
    ) -> io::Result<bool> {
        match action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                match &msg {
                    ChatAppMsg::ClearHistory => {
                        agent.clear_history();
                        tracing::info!("Conversation history cleared");
                    }
                    ChatAppMsg::StreamCancelled => {
                        if active_stream.is_some() {
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
                    _ => {}
                }
                let action = app.on_message(msg);
                Box::pin(self.process_action(action, app, agent, active_stream)).await
            }
            Action::Batch(actions) => {
                for action in actions {
                    if Box::pin(self.process_action(action, app, agent, active_stream)).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}
