use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_container::ContainerList;
#[allow(unused_imports)] // WIP: Drawer, DrawerKind not yet used
use crate::tui::oil::components::{
    Drawer, DrawerKind, InteractionModal, InteractionModalMsg, InteractionModalOutput,
    InteractionMode, NotificationArea, PopupComponent, ShellHistoryItem, ShellModal, ShellModalMsg,
    ShellModalOutput, ShellStatus, StatusComponent,
};
use crate::tui::oil::config::RuntimeConfig;
#[cfg(test)]
use crate::tui::oil::event::InputAction;
use crate::tui::oil::event::{Event, InputBuffer};
use crate::tui::oil::node::*;
use crate::tui::oil::style::Gap;
#[allow(unused_imports)] // WIP: KeyCode not yet used
use crossterm::event::KeyCode;
#[allow(unused_imports)] // WIP: AskRequest, AskResponse, PermAction, PermRequest not yet used
use crucible_core::interaction::{
    AskRequest, AskResponse, InteractionRequest, InteractionResponse, PermAction, PermRequest,
    PermResponse, PermissionScope,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

const FOCUS_INPUT: &str = "input";
#[allow(dead_code)] // WIP: FOCUS_POPUP not yet used
const FOCUS_POPUP: &str = "popup";
const POPUP_HEIGHT: usize = 10;
pub const INPUT_MAX_CONTENT_LINES: usize = 3;

#[allow(dead_code)] // WIP: MAX_DISPLAY_ITEMS not yet used
const MAX_DISPLAY_ITEMS: usize = 512;
const MAX_SHELL_HISTORY: usize = 100;

// ─── Submodules ──────────────────────────────────────────────────────────────

mod autocomplete;
mod command_handling;
mod defaults;
mod input_handling;
mod message_handlers;
pub mod messages;
pub mod model_state;
pub mod popup_state;
mod rendering;
mod shell;
pub mod state;

pub use messages::ChatAppMsg;
pub use model_state::{McpServerDisplay, ModelListState, PluginStatusEntry};
use popup_state::{PermissionState, PopupState, PrecognitionState, ShellHistoryState};
#[cfg(test)]
use state::AutocompleteKind;
use state::MessageQueueState;
pub use state::{ChatMode, InputMode, Role};

// ─── Main Struct ─────────────────────────────────────────────────────────────

pub struct OilChatApp {
    // ─── Viewport Projection (daemon-derived state) ───────────────────
    // These fields mirror information received from the daemon and
    // represent the authoritative view of the current session.
    /// Semantic containers for chat content (container architecture)
    container_list: ContainerList,
    /// Current chat mode (Normal / Plan / Auto)
    mode: ChatMode,
    /// Display name of the active LLM model
    model: String,
    /// Status text from the daemon (e.g. "Thinking…")
    status: String,
    /// Context window tokens consumed so far
    context_used: usize,
    /// Context window total capacity
    context_total: usize,
    /// Display name of the active LLM provider
    current_provider: String,
    /// MCP servers known to the daemon
    mcp_servers: Vec<McpServerDisplay>,
    plugin_status: Vec<PluginStatusEntry>,
    /// Available models fetched from the provider
    available_models: Vec<String>,
    /// Fetch-state of the model list
    model_list_state: ModelListState,
    /// Flag to prevent duplicate fetch/loading messages when :model is pressed repeatedly
    model_fetch_message_shown: bool,

    // ─── UI Chrome (purely local state) ───────────────────────────────
    // Everything here is display-only and never round-trips to the
    // daemon. Grouped by concern.
    /// Text input buffer for the chat prompt
    input: InputBuffer,
    /// Autocomplete popup state
    popup: PopupState,
    /// Notification banner area
    notification_area: NotificationArea,
    /// Interactive permission / question modal
    interaction_modal: Option<InteractionModal>,
    /// Shell command modal overlay
    shell_modal: Option<ShellModal>,
    /// Spinner animation start time (frame derived from elapsed time, not ticks)
    spinner_epoch: std::time::Instant,
    /// Force a full terminal redraw on next tick
    needs_full_redraw: bool,
    /// Whether to render LLM thinking/reasoning blocks
    show_thinking: bool,
    /// Precognition state (auto-RAG settings)
    precognition: PrecognitionState,
    /// Current terminal size (width, height) — updated in view()
    terminal_size: Cell<(u16, u16)>,

    /// Permission request state
    permission: PermissionState,
    /// Message queue state (deferred messages, counter, Ctrl-C tracking)
    message_queue: MessageQueueState,
    /// Files attached as extra context for the next message
    attached_context: Vec<String>,
    /// When true, discard incoming TextDelta events (stale events after cancel).
    drop_stream_deltas: bool,
    pending_delegate_supersessions: HashSet<String>,

    // ─── I/O / Lifecycle (tech debt — future extraction) ──────────────
    // Callbacks, filesystem state, and registries that ideally move
    // behind a trait or into a dedicated struct later.
    /// Submit callback — fires when the user sends a message
    #[allow(dead_code)] // WIP: on_submit callback not yet used
    on_submit: Option<Box<dyn Fn(String) + Send + Sync>>,
    /// Filesystem path for saving session transcripts
    session_dir: Option<PathBuf>,
    /// Shell command history state
    shell_history: ShellHistoryState,
    /// Runtime configuration (`:set` overrides)
    runtime_config: RuntimeConfig,
    /// Workspace file paths (for @-file autocomplete)
    workspace_files: Vec<String>,
    /// Kiln note names (for #-note autocomplete)
    kiln_notes: Vec<String>,
    /// Known slash commands (name, description) for autocomplete — populated by runner
    slash_commands: Vec<(String, String)>,
    /// Lua statusline layout config (loaded once at startup)
    statusline_config: Option<crucible_lua::statusline::StatuslineConfig>,
}

// ─── App Trait ────────────────────────────────────────────────────────────────

impl App for OilChatApp {
    type Msg = ChatAppMsg;

    fn init() -> Self {
        Self::default()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        self.terminal_size.set(ctx.terminal_size);
        if self.shell_modal.is_some() {
            return self.render_shell_modal();
        }

        tracing::debug!(target: "crucible_cli::tui::oil::model_flow",
            has_modal = self.interaction_modal.is_some(),
            notification_visible = self.notification_area.is_visible(),
            "view: layout branch selected");
        let bottom = if let Some(modal) = &self.interaction_modal {
            let term_width = ctx.terminal_size.0 as usize;
            modal.view(term_width, self.permission.permission_queue.len())
        } else if self.notification_area.is_visible() {
            self.render_messages_drawer(ctx)
        } else {
            col([self.render_input(ctx), self.render_status()])
        };

        col([
            self.render_containers(),
            spacer(),
            text(" "),
            bottom,
            self.render_popup_overlay(ctx),
        ])
        .gap(Gap::row(0))
    }

    fn update(&mut self, event: Event) -> Action<ChatAppMsg> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Tick => {
                // Shell modal tick polls for child process output.
                // Also runs in render_frame() via expire_toasts(), but kept
                // here for tests that call update(Tick) directly.
                self.tick_shell_modal();
                Action::Continue
            }
            Event::Resize { .. } => Action::Continue,
            Event::Quit => Action::Quit,
        }
    }

    fn on_message(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        use messages::MsgCategory;
        match msg.category() {
            MsgCategory::User => {
                if let ChatAppMsg::UserMessage(content) = msg {
                    if !self.is_streaming() {
                        self.submit_user_message(content);
                    }
                }
                Action::Continue
            }
            MsgCategory::Stream => self.handle_stream_msg(msg),
            MsgCategory::Config => self.handle_config_msg(msg),
            MsgCategory::Delegation => self.handle_delegation_msg(msg),
            MsgCategory::Ui => self.handle_ui_msg(msg),
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(100))
    }
}

// ─── Accessors & Lifecycle ───────────────────────────────────────────────────

#[allow(dead_code)] // WIP: multiple methods not yet used
impl OilChatApp {
    fn with_on_submit<F>(mut self, callback: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.on_submit = Some(Box::new(callback));
        self
    }

    pub(crate) fn set_mode(&mut self, mode: ChatMode) {
        self.mode = mode;
    }

    pub(crate) fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub(crate) fn set_workspace_files(&mut self, files: Vec<String>) {
        self.workspace_files = files;
    }

    pub(crate) fn set_kiln_notes(&mut self, notes: Vec<String>) {
        self.kiln_notes = notes;
    }

    pub(crate) fn set_slash_commands(&mut self, commands: Vec<(String, String)>) {
        self.slash_commands = commands;
    }

    pub(crate) fn set_session_dir(&mut self, path: PathBuf) {
        self.session_dir = Some(path);
    }

    pub(crate) fn session_dir(&self) -> Option<&std::path::Path> {
        self.session_dir.as_deref()
    }

    pub(crate) fn set_mcp_servers(&mut self, servers: Vec<McpServerDisplay>) {
        self.mcp_servers = servers;
    }

    pub(crate) fn set_plugin_status(&mut self, entries: Vec<PluginStatusEntry>) {
        self.plugin_status = entries;
    }

    pub(crate) fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models.clone();
        if !models.is_empty() {
            self.model_list_state = ModelListState::Loaded;
        }
    }

    pub(crate) fn model_list_state(&self) -> &ModelListState {
        &self.model_list_state
    }

    pub(crate) fn set_model_list_state(&mut self, state: ModelListState) {
        self.model_list_state = state;
    }

    pub(crate) fn available_models(&self) -> &[String] {
        &self.available_models
    }

    pub(crate) fn set_show_thinking(&mut self, show: bool) {
        self.show_thinking = show;
    }

    /// Spinner frame derived from wall clock (100ms per frame).
    /// Independent of tick events — animates even during rapid streaming.
    pub fn spinner_frame(&self) -> usize {
        (self.spinner_epoch.elapsed().as_millis() / 100) as usize
    }

    /// Periodic maintenance called each render frame.
    /// Expires stale toasts and ticks shell modal.
    pub fn expire_toasts(&mut self) {
        self.tick_shell_modal();
        self.notification_area.expire_toasts();
        if self.notification_area.is_empty() {
            self.notification_area.hide();
        }
    }

    pub(crate) fn set_precognition(&mut self, val: bool) {
        self.precognition.precognition = val;
    }

    pub(crate) fn precognition(&self) -> bool {
        self.precognition.precognition
    }

    fn precognition_results(&self) -> usize {
        self.precognition.precognition_results
    }

    pub(crate) fn set_precognition_results(&mut self, count: usize) {
        self.precognition.precognition_results = count;
    }

    fn perm_show_diff(&self) -> bool {
        self.permission.perm_show_diff
    }

    fn perm_autoconfirm_session(&self) -> bool {
        self.permission.perm_autoconfirm_session
    }

    /// Get access to the container list for testing/inspection.
    #[cfg(test)]
    pub(crate) fn container_list(&self) -> &ContainerList {
        &self.container_list
    }

    pub(crate) fn add_notification(&mut self, notification: crucible_core::types::Notification) {
        self.notification_area.add(notification);
    }

    pub(crate) fn toggle_messages(&mut self) {
        self.notification_area.toggle();
    }

    pub(crate) fn show_messages(&mut self) {
        self.notification_area.show();
    }

    pub(crate) fn hide_messages(&mut self) {
        self.notification_area.hide();
    }

    pub(crate) fn clear_notifications(&mut self) {
        self.notification_area.clear();
    }

    pub(crate) fn clear_messages(&mut self) {
        self.notification_area.clear();
    }

    /// Drain completed containers and return graduation content for stdout.
    pub(crate) fn drain_graduated(&mut self, width: u16) -> Option<crucible_oil::Graduation> {
        self.container_list
            .drain_completed(width, self.spinner_frame(), self.show_thinking)
    }

    /// Replay stored session events through the live event path.
    /// This ensures resume reconstructs the same state as live streaming —
    /// user messages, thinking, tools, and delegation all come through for free.
    pub(crate) fn load_history_events(&mut self, events: Vec<serde_json::Value>) {
        use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

        self.container_list.clear();
        for event in &events {
            let event_type = event.get("event").and_then(|e| e.as_str()).unwrap_or("");
            let data = event.get("data").cloned().unwrap_or_default();
            for msg in session_event_to_chat_msgs(event_type, &data) {
                self.on_message(msg);
            }
        }
        // Mark the loaded history as complete (not actively streaming)
        self.container_list.complete_response();
        self.message_queue.message_counter = self.container_list.len();
    }

    fn push_shell_history(&mut self, cmd: String) {
        if self.shell_history.shell_history.len() >= MAX_SHELL_HISTORY {
            self.shell_history.shell_history.pop_front();
        }
        self.shell_history.shell_history.push_back(cmd);
    }

    pub(crate) fn is_streaming(&self) -> bool {
        self.container_list.is_streaming()
    }

    pub(crate) fn input_content(&self) -> &str {
        self.input.content()
    }

    #[cfg(test)]
    pub(crate) fn is_popup_visible(&self) -> bool {
        self.popup.show
    }

    #[cfg(test)]
    pub(crate) fn current_popup_filter(&self) -> &str {
        &self.popup.filter
    }

    #[cfg(test)]
    pub(crate) fn current_model(&self) -> &str {
        &self.model
    }

    pub(crate) fn has_shell_modal(&self) -> bool {
        self.shell_modal.is_some()
    }

    pub(crate) fn open_interaction(
        &mut self,
        request_id: String,
        request: InteractionRequest,
    ) -> Action<ChatAppMsg> {
        if self.permission.perm_autoconfirm_session {
            if let InteractionRequest::Permission(_) = &request {
                tracing::info!(request_id = %request_id, "Auto-confirming permission");
                return Action::Send(ChatAppMsg::CloseInteraction {
                    request_id,
                    response: InteractionResponse::Permission(PermResponse::allow()),
                });
            }
        }

        if let InteractionRequest::Permission(perm) = &request {
            // Allow the trailing AssistantResponse (thinking) to graduate.
            // The daemon sends interaction_requested before tool_call, so
            // without this the thinking stays as a spinner in the viewport.
            self.container_list.set_permission_pending(true);

            if self.interaction_modal.is_some() {
                self.permission
                    .permission_queue
                    .push_back((request_id, perm.clone()));
                return Action::Continue;
            }
        }

        self.notification_area.hide();

        self.interaction_modal = Some(InteractionModal::new(
            request_id,
            request,
            self.permission.perm_show_diff,
        ));
        Action::Continue
    }

    fn close_interaction(&mut self) {
        self.interaction_modal = None;
    }

    pub(crate) fn interaction_visible(&self) -> bool {
        self.interaction_modal.is_some()
    }

    #[cfg(test)]
    pub(crate) fn shell_output_lines(&self) -> Vec<String> {
        self.shell_modal
            .as_ref()
            .map(|m| m.output_lines().to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn shell_visible_lines(&self, max_lines: usize) -> Vec<String> {
        self.shell_modal
            .as_ref()
            .map(|m| m.visible_lines(max_lines).to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn shell_scroll_offset(&self) -> usize {
        self.shell_modal
            .as_ref()
            .map(|m| m.scroll_offset())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn set_input_content(&mut self, content: &str) {
        self.input.handle(InputAction::Clear);
        for ch in content.chars() {
            self.input.handle(InputAction::Insert(ch));
        }
    }

    #[cfg(test)]
    pub(crate) fn handle_input_action(&mut self, action: InputAction) {
        self.input.handle(action);
    }

    pub(crate) fn take_needs_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_full_redraw)
    }

    fn add_user_message(&mut self, content: String) {
        self.message_queue.message_counter += 1;
        self.container_list.add_user_message(content);
    }

    /// Add user message and mark the turn as active so the turn-level
    /// spinner renders while waiting for the first token.
    /// Use this (not `add_user_message`) when sending to the daemon.
    fn submit_user_message(&mut self, content: String) {
        self.add_user_message(content);
        self.container_list.mark_turn_active();
    }

    pub(crate) fn add_system_message(&mut self, content: String) {
        self.message_queue.message_counter += 1;
        self.container_list.add_system_message(content);
    }

    fn finalize_streaming(&mut self) {
        if self.container_list.is_streaming() {
            self.message_queue.message_counter += 1;
            self.container_list.complete_response();
        }

        self.status = "Ready".to_string();
    }

    pub(crate) fn reset_session(&mut self) {
        self.container_list.clear();
        self.message_queue.message_counter = 0;
        self.message_queue.deferred_messages.clear();
        self.context_used = 0;
        self.context_total = 0;
        self.status = "Ready".to_string();
        self.notification_area.clear();
        self.pending_delegate_supersessions.clear();
        self.needs_full_redraw = true;
    }

    fn process_deferred_queue(&mut self) -> Action<ChatAppMsg> {
        if let Some(queued) = self.message_queue.deferred_messages.pop_front() {
            self.submit_user_message(queued.clone());
            self.status = "Thinking...".to_string();
            Action::Send(ChatAppMsg::UserMessage(queued))
        } else {
            Action::Continue
        }
    }
}

#[cfg(test)]
mod tests;
