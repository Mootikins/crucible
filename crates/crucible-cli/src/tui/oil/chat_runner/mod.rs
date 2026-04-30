use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{
    ChatAppMsg, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry,
};
use crate::tui::oil::event::Event;
#[allow(unused_imports)] // WIP: KeyCode, KeyModifiers not yet used
use crossterm::event::{KeyCode, KeyModifiers};
use crucible_core::traits::chat::AgentHandle;
use crucible_lua::SessionCommand;
use crucible_oil::focus::FocusContext;
use crucible_oil::terminal::Terminal;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::tui::oil::commands::SetEffect;

mod actions;
mod commands;
mod render;
mod runner;
mod stream;

#[cfg(test)]
mod tests;

pub use commands::session_event_to_chat_msgs;
pub use render::render_frame;
pub(crate) use stream::session_event_consumer;
pub use stream::SessionEventStream;

/// Parameters for event_loop function.
pub(super) struct EventLoopParams<'a, A: AgentHandle> {
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
pub(super) struct HandleSelectedEventParams<'a, A: AgentHandle> {
    pub event: Option<Event>,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for handle_select_outcome function.
pub(super) struct HandleSelectOutcomeParams<'a, A: AgentHandle> {
    pub select_outcome: EventLoopSelectOutcome,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// Parameters for process_action function.
pub(super) struct ProcessActionParams<'a, A: AgentHandle> {
    pub action: Action<ChatAppMsg>,
    pub app: &'a mut OilChatApp,
    pub agent: &'a mut A,
    pub bridge: &'a AgentEventBridge,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

pub struct OilChatRunner {
    pub(super) terminal: Terminal,
    pub(super) tick_rate: Duration,
    pub(super) mode: ChatMode,
    pub(super) model: String,
    pub(super) context_limit: Arc<AtomicUsize>,
    pub(super) focus: FocusContext,
    pub(super) workspace_files: Vec<String>,
    pub(super) kiln_notes: Vec<String>,
    pub(super) session_dir: Option<PathBuf>,
    pub(super) resume_session_id: Option<String>,
    pub(super) resume_history: Option<Vec<serde_json::Value>>,
    pub(super) mcp_servers: Vec<McpServerDisplay>,
    pub(super) plugin_status: Vec<PluginStatusEntry>,
    pub(super) mcp_config: Option<crucible_core::config::mcp::McpConfig>,
    pub(super) available_models: Vec<String>,
    pub(super) show_thinking: bool,
    pub(super) show_diffs: bool,
    pub(super) session_cmd_rx: Option<mpsc::UnboundedReceiver<SessionCommand>>,
    pub(super) slash_commands: Vec<(String, String)>,
    pub(super) agent_name: Option<String>,
    pub(super) initial_sets: Vec<SetEffect>,
    pub(super) recording_mode: Option<String>,
    pub(super) recording_path: Option<PathBuf>,
    pub(super) replay_path: Option<PathBuf>,
    pub(super) replay_speed: f64,
    pub(super) replay_auto_exit: Option<u64>,
    pub(super) replay_remaining_completes: usize,
    pub(super) is_replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DrainMessagesOutcome {
    Idle,
    Quit,
    Processed,
}

pub(super) enum EventLoopSelectOutcome {
    Event(Option<Event>),
    Continue,
    Quit,
}

pub(super) enum DrainPhaseOutcome {
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
            show_diffs: true,
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

    pub fn with_show_diffs(mut self, show: bool) -> Self {
        self.show_diffs = show;
        self
    }

    pub fn with_slash_commands(mut self, commands: Vec<(String, String)>) -> Self {
        self.slash_commands = commands;
        self
    }

    pub fn with_mcp_config(mut self, config: crucible_core::config::mcp::McpConfig) -> Self {
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

    /// Queue an initial `FetchModels` message so the `:model` popup has data
    /// without a user-triggered round-trip.
    ///
    /// Structurally live-path only: called exclusively from
    /// `run_with_factory`. The replay entry point (added in Task 2.3c) does
    /// not invoke this. If `FetchModels` ever reaches the event loop under
    /// replay anyway, the guard on the `ChatAppMsg::FetchModels` arm
    /// swallows it — see the match-arm comment there.
    pub(super) fn queue_model_prefetch(&self, msg_tx: &mpsc::UnboundedSender<ChatAppMsg>) {
        if msg_tx.send(ChatAppMsg::FetchModels).is_err() {
            tracing::warn!("UI channel closed, initial FetchModels dropped");
        }
    }

    pub(crate) fn abort_background_tasks(background_tasks: &mut Vec<JoinHandle<()>>) {
        for task in background_tasks.drain(..) {
            task.abort();
        }
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

    /// Test-only helper: drive `process_action` directly so tests exercise
    /// the real production path rather than a mirrored copy of its body.
    /// Constructs the dependent params (msg_tx, background_tasks) inline.
    #[cfg(test)]
    pub(crate) async fn process_action_for_test<A: AgentHandle>(
        &mut self,
        action: Action<ChatAppMsg>,
        app: &mut OilChatApp,
        agent: &mut A,
        bridge: &AgentEventBridge,
    ) -> io::Result<bool> {
        let (msg_tx, _msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();
        let mut background_tasks: Vec<JoinHandle<()>> = Vec::new();
        self.process_action(ProcessActionParams {
            action,
            app,
            agent,
            bridge,
            msg_tx: &msg_tx,
            background_tasks: &mut background_tasks,
        })
        .await
    }
}
