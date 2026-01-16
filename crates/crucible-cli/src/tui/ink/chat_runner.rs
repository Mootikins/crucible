use crate::chat::bridge::AgentEventBridge;
use crate::tui::agent_picker::AgentSelection;
use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::chat_app::{ChatAppMsg, ChatMode, InkChatApp};
use crate::tui::ink::event::Event;
use crate::tui::ink::focus::FocusContext;
use crate::tui::ink::terminal::Terminal;
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
    focus: FocusContext,
    workspace_files: Vec<String>,
    kiln_notes: Vec<String>,
    session_dir: Option<PathBuf>,
}

impl InkChatRunner {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminal: Terminal::new()?,
            tick_rate: Duration::from_millis(50),
            mode: ChatMode::Plan,
            focus: FocusContext::new(),
            workspace_files: Vec::new(),
            kiln_notes: Vec::new(),
            session_dir: None,
        })
    }

    pub fn with_mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
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
                let action = app.on_message(msg);
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

                            if !chunk.delta.is_empty() {
                                let _ = msg_tx.send(ChatAppMsg::TextDelta(chunk.delta));
                            }

                            if let Some(ref tool_calls) = chunk.tool_calls {
                                for tc in tool_calls {
                                    let args_val = tc.arguments.clone().unwrap_or_default();
                                    let _ = msg_tx.send(ChatAppMsg::ToolCall {
                                        name: tc.name.clone(),
                                        args: args_val.to_string(),
                                    });
                                }
                            }

                            if let Some(ref tool_results) = chunk.tool_results {
                                tracing::info!(count = tool_results.len(), "Received tool_results in chunk");
                                for tr in tool_results {
                                    tracing::info!(
                                        name = %tr.name,
                                        result_len = tr.result.len(),
                                        error = ?tr.error,
                                        "Processing tool result"
                                    );
                                    if !tr.result.is_empty() {
                                        let _ = msg_tx.send(ChatAppMsg::ToolResultDelta {
                                            name: tr.name.clone(),
                                            delta: tr.result.clone(),
                                        });
                                    }
                                    let _ = msg_tx.send(ChatAppMsg::ToolResultComplete {
                                        name: tr.name.clone(),
                                    });
                                }
                            }

                            if chunk.done {
                                active_stream = None;
                                let _ = msg_tx.send(ChatAppMsg::StreamComplete);
                            }
                        }
                        Err(e) => {
                            active_stream = None;
                            let _ = msg_tx.send(ChatAppMsg::Error(e.to_string()));
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
                if self.process_action(action, app)? {
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

    fn process_action(
        &mut self,
        action: Action<ChatAppMsg>,
        app: &mut InkChatApp,
    ) -> io::Result<bool> {
        match action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                let action = app.on_message(msg);
                self.process_action(action, app)
            }
            Action::Batch(actions) => {
                for action in actions {
                    if self.process_action(action, app)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}
