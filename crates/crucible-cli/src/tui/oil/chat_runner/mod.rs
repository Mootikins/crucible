use crate::session::LiveSession;
#[cfg(test)]
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{
    ChatAppMsg, KilnSummary, McpServerDisplay, OilChatApp, PluginStatusEntry,
};
use crate::tui::oil::event::Event;
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
pub use stream::event_msgs;
pub use stream::SessionEventStream;
pub(crate) use stream::{live_session_event_consumer, session_event_consumer};

/// The TUI's own wording of one [`crucible_core::protocol::requests::ResumeWarning`].
///
/// The one place this client turns the daemon's warning list into text, so
/// every caller (today, only the resume of a fresh chat run) reads the same
/// sentence for the same warning.
pub(crate) fn resume_warning_text(
    warning: &crucible_core::protocol::requests::ResumeWarning,
) -> String {
    use crucible_core::protocol::requests::ResumeWarning;
    match warning {
        ResumeWarning::PluginStateReset => {
            "This session's plugin state did not survive the resume. Values a plugin \
             saved with session:set_variable came back; anything else a plugin kept in \
             memory did not, and its session_start hooks ran again."
                .to_string()
        }
        ResumeWarning::PendingWorkCleared => {
            "Work in progress when this session ended did not survive: any running \
             subagent, in-flight tool call, or unanswered prompt is gone. The session \
             starts clean."
                .to_string()
        }
        ResumeWarning::KilnUnavailable { path } => {
            format!(
                "The kiln at {path} no longer resolves. This session keeps searching \
                 its other kilns, but not this one, until it is registered again."
            )
        }
    }
}

/// Parameters for event_loop function.
///
/// `session` is `None` in a replay: a replay reaches no daemon. Owns both
/// channel ends, so it cannot merge with [`StageCtx`]: the event loop still
/// holds the receiver, and it hands the sender out to each stage only as a
/// borrow.
pub(super) struct EventLoopParams<'a> {
    pub app: &'a mut OilChatApp,
    pub session: Option<&'a LiveSession>,
    pub msg_tx: mpsc::UnboundedSender<ChatAppMsg>,
    pub msg_rx: mpsc::UnboundedReceiver<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

/// The context every event-loop stage shares: the app, the optional live
/// session, and the borrowed handles for sending messages and tracking
/// background tasks. Each stage function takes this plus its own one extra
/// argument (the event, the select outcome, or the action).
pub(super) struct StageCtx<'a> {
    pub app: &'a mut OilChatApp,
    pub session: Option<&'a LiveSession>,
    pub msg_tx: &'a mpsc::UnboundedSender<ChatAppMsg>,
    pub background_tasks: &'a mut Vec<JoinHandle<()>>,
}

pub struct OilChatRunner {
    pub(super) terminal: Terminal,
    pub(super) tick_rate: Duration,
    pub(super) mode: std::sync::Arc<str>,
    pub(super) model: String,
    pub(super) context_limit: Arc<AtomicUsize>,
    pub(super) focus: FocusContext,
    pub(super) workspace_files: Vec<String>,
    pub(super) kiln_notes: Vec<String>,
    pub(super) shell_output_dir: Option<PathBuf>,
    pub(super) resume_session_id: Option<String>,
    pub(super) resume_history: Option<Vec<serde_json::Value>>,
    /// The transcript that the daemon folded from `resume_history`.
    pub(super) resume_transcript: Option<crucible_core::transcript::Transcript>,
    pub(super) mcp_servers: Vec<McpServerDisplay>,
    pub(super) connected_kilns: Vec<KilnSummary>,
    pub(super) plugin_status: Vec<PluginStatusEntry>,
    pub(super) show_thinking: bool,
    pub(super) show_diffs: bool,
    pub(super) agent_name: Option<String>,
    pub(super) initial_sets: Vec<SetEffect>,
    pub(super) replay_path: Option<PathBuf>,
    pub(super) replay_speed: f64,
    pub(super) replay_auto_exit: Option<u64>,
    pub(super) replay_remaining_completes: usize,
    pub(super) is_replay: bool,
    /// The full-screen view. `None` is the inline mode (`--inline`).
    pub(super) fullscreen: Option<crate::tui::oil::fullscreen::FullscreenView>,
    /// Whether the shell modal was open at the last frame. The modal leaves
    /// the alternate screen when it closes, so the full-screen mode enters
    /// it again.
    pub(super) shell_was_open: bool,
    pub(super) copier: crate::tui::oil::fullscreen::clipboard::Copier,
    /// The session that `/resume` chose. The event loop stops, and
    /// `run_with_factory` hands the id to the caller as [`ChatExit::Resume`].
    pub(super) next_session: Option<String>,
}

/// How one run of the chat TUI ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatExit {
    /// The user quit.
    Quit,
    /// `/resume` chose this session. The caller runs the TUI again on it,
    /// through the same path as `cru chat --resume`.
    Resume(String),
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
            mode: crate::tui::oil::chat_app::DEFAULT_MODE.into(),
            model: String::new(),
            context_limit: Arc::new(AtomicUsize::new(0)),
            focus: FocusContext::new(),
            workspace_files: Vec::new(),
            kiln_notes: Vec::new(),
            shell_output_dir: None,
            resume_session_id: None,
            resume_history: None,
            resume_transcript: None,
            mcp_servers: Vec::new(),
            connected_kilns: Vec::new(),
            plugin_status: Vec::new(),
            show_thinking: false,
            show_diffs: true,
            agent_name: None,
            initial_sets: Vec::new(),
            replay_path: None,
            replay_speed: 1.0,
            replay_auto_exit: None,
            replay_remaining_completes: 0,
            is_replay: false,
            fullscreen: None,
            shell_was_open: false,
            copier: Default::default(),
            next_session: None,
        }
    }

    /// Draw on the alternate screen or on the main screen.
    ///
    /// Without this call the runner draws inline, as a headless test
    /// runner does; the chat command always names the screen.
    pub fn with_screen(mut self, screen: crucible_core::config::ChatScreen) -> Self {
        use crucible_core::config::ChatScreen;
        use crucible_oil::terminal::ScreenMode;
        match screen {
            ChatScreen::Fullscreen => {
                self.terminal.set_mode(ScreenMode::Fullscreen {
                    mouse_capture: true,
                });
                self.fullscreen = Some(crate::tui::oil::fullscreen::FullscreenView::new());
            }
            ChatScreen::Inline => {
                self.terminal.set_mode(ScreenMode::Inline);
                self.fullscreen = None;
            }
        }
        self
    }

    pub fn with_context_limit(mut self, limit: usize) -> Self {
        self.context_limit = Arc::new(AtomicUsize::new(limit));
        self
    }

    pub fn with_mode(mut self, mode: impl Into<std::sync::Arc<str>>) -> Self {
        self.mode = mode.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// The folder where the shell modal saves the output of a command.
    pub fn with_shell_output_dir(mut self, path: PathBuf) -> Self {
        self.shell_output_dir = Some(path);
        self
    }

    pub fn with_resume_session(mut self, session_id: impl Into<String>) -> Self {
        self.resume_session_id = Some(session_id.into());
        self
    }

    pub fn with_resume_history(
        mut self,
        history: Vec<serde_json::Value>,
        transcript: crucible_core::transcript::Transcript,
    ) -> Self {
        self.resume_history = Some(history);
        self.resume_transcript = Some(transcript);
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

    /// The kilns the startup banner names. Empty says so in as many words.
    pub fn with_connected_kilns(mut self, kilns: Vec<KilnSummary>) -> Self {
        self.connected_kilns = kilns;
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

    /// Start the initial model fetch so the `:model` popup has data without
    /// a user-triggered round-trip: queue `FetchModels` (reducer transition
    /// to Loading) and spawn the daemon RPC that resolves it.
    ///
    /// Both halves are needed. Messages drained from the UI channel only hit
    /// the reducer, never `process_action`, so sending `FetchModels` alone
    /// wedges the picker at Loading forever — nothing re-fetches while
    /// Loading.
    ///
    /// Structurally live-path only: called exclusively from
    /// `run_with_factory`. The replay entry point (added in Task 2.3c) does
    /// not invoke this. If `FetchModels` ever reaches the event loop under
    /// replay anyway, the guard on the `ChatAppMsg::FetchModels` arm
    /// swallows it — see the match-arm comment there.
    pub(super) fn queue_model_prefetch(
        &self,
        msg_tx: &mpsc::UnboundedSender<ChatAppMsg>,
        background_tasks: &mut Vec<JoinHandle<()>>,
        session_models_source: Option<String>,
    ) {
        if msg_tx.send(ChatAppMsg::FetchModels).is_err() {
            tracing::warn!("UI channel closed, initial FetchModels dropped");
            return;
        }
        Self::spawn_model_fetch(msg_tx, background_tasks, session_models_source);
        // The mode list rides the same prefetch. `process_action` reads it
        // from the session, so there is nothing to spawn here — only the
        // message to queue.
        if msg_tx.send(ChatAppMsg::FetchModes).is_err() {
            tracing::warn!("UI channel closed, initial FetchModes dropped");
        }
        if msg_tx.send(ChatAppMsg::FetchCommands).is_err() {
            tracing::warn!("UI channel closed, initial FetchCommands dropped");
        }
    }

    pub(crate) fn abort_background_tasks(background_tasks: &mut Vec<JoinHandle<()>>) {
        for task in background_tasks.drain(..) {
            task.abort();
        }
    }

    /// Test-only helper: drive `process_action` directly so tests exercise
    /// the real production path rather than a mirrored copy of its body.
    /// Constructs the dependent params (msg_tx, background_tasks) inline.
    #[cfg(test)]
    pub(crate) async fn process_action_for_test(
        &mut self,
        action: Action<ChatAppMsg>,
        app: &mut OilChatApp,
        session: Option<&LiveSession>,
    ) -> io::Result<bool> {
        let (msg_tx, _msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();
        let mut background_tasks: Vec<JoinHandle<()>> = Vec::new();
        self.process_action(
            StageCtx {
                app,
                session,
                msg_tx: &msg_tx,
                background_tasks: &mut background_tasks,
            },
            action,
        )
        .await
    }

    /// Like `process_action_for_test`, but returns the messages the runner
    /// queued back onto the loop. Corrections that must land *after* the
    /// trailing `on_message` dispatch go through the channel, so applying them
    /// is the only way a test can observe them.
    #[cfg(test)]
    pub(crate) async fn process_action_collecting_msgs(
        &mut self,
        action: Action<ChatAppMsg>,
        app: &mut OilChatApp,
        session: Option<&LiveSession>,
    ) -> Vec<ChatAppMsg> {
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();
        let mut background_tasks: Vec<JoinHandle<()>> = Vec::new();
        self.process_action(
            StageCtx {
                app,
                session,
                msg_tx: &msg_tx,
                background_tasks: &mut background_tasks,
            },
            action,
        )
        .await
        .expect("process_action should not fail");
        drop(msg_tx);
        let mut queued = Vec::new();
        while let Ok(msg) = msg_rx.try_recv() {
            queued.push(msg);
        }
        queued
    }
}
