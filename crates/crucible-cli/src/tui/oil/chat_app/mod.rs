use crate::tui::oil::app::{Action, ViewContext};
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{
    CommandPanel, InputComponent, InteractionModal, NotificationArea, ShellModal, StatusComponent,
};
use crate::tui::oil::config::RuntimeConfig;
#[cfg(test)]
use crate::tui::oil::event::InputAction;
use crate::tui::oil::event::{Event, InputBuffer};
use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermResponse};
use crucible_oil::node::*;
use crucible_oil::style::Gap;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;

const POPUP_HEIGHT: usize = 10;
pub const INPUT_MAX_CONTENT_LINES: usize = 3;

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
mod repl_command;
mod shell;
pub mod state;

pub use messages::ChatAppMsg;
pub use model_state::{KilnSummary, McpServerDisplay, ModelListState, PluginStatusEntry};
use popup_state::{PermissionState, PopupState, PrecognitionState, ShellHistoryState};
use state::MessageQueueState;
pub use state::{mode_label, mode_style, next_mode, DEFAULT_MODE, DEFAULT_MODES};

// ─── Main Struct ─────────────────────────────────────────────────────────────

pub struct OilChatApp {
    // ─── Viewport Projection (daemon-derived state) ───────────────────
    // These fields mirror information received from the daemon and
    // represent the authoritative view of the current session.
    /// Container list: ordered chat content with graduation support
    pub(crate) container_list: crate::tui::oil::containers::ContainerList,
    /// The session's mode id, as the daemon named it. Held as a string
    /// because modes are declared in Lua — see `state::DEFAULT_MODE`.
    mode: std::sync::Arc<str>,
    /// Mode ids the daemon offers, in declaration order. Populated by
    /// `session.list_modes`; empty until that lands, which is why `/mode`
    /// cycling falls back to leaving the mode alone.
    pub(crate) available_modes: Vec<String>,
    /// Display name of the active LLM model
    model: String,
    /// Status text from the daemon (e.g. "Thinking…")
    status: String,
    /// Context window tokens consumed so far
    context_used: usize,
    /// Context window total capacity
    context_total: usize,
    /// Latest prompt-cache hit rate (0.0..=1.0) from `message_complete`.
    /// `None` until at least one completion has reported cache token
    /// counts; the statusline `cache_hit_rate` component renders nothing
    /// in that "no data" state to avoid a misleading `cache: 0%`.
    pub(crate) cache_hit_rate: Option<f64>,
    /// Display name of the active LLM provider
    current_provider: String,
    /// MCP servers known to the daemon
    mcp_servers: Vec<McpServerDisplay>,
    plugin_status: Vec<PluginStatusEntry>,
    /// Available models fetched from the provider
    available_models: Vec<String>,
    /// Fetch-state of the model list
    model_list_state: ModelListState,

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
    /// The frame clock. See [`OilChatApp::set_frame_time`].
    frame_time: std::time::Instant,
    /// Force a full terminal redraw on next tick
    needs_full_redraw: bool,
    /// Whether to render LLM thinking/reasoning blocks
    show_thinking: bool,
    /// Whether to render Edit/Write tool diff bodies (header always renders)
    show_diffs: bool,
    /// Precognition state (auto-RAG settings)
    precognition: PrecognitionState,
    /// Current terminal size (width, height) — updated in view()
    terminal_size: Cell<(u16, u16)>,

    /// Permission request state
    permission: PermissionState,
    /// Message queue state (deferred messages, counter, Ctrl-C tracking)
    message_queue: MessageQueueState,
    pending_delegate_supersessions: HashSet<String>,

    // ─── I/O / Lifecycle (tech debt — future extraction) ──────────────
    // Callbacks, filesystem state, and registries that ideally move
    // behind a trait or into a dedicated struct later.
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
    /// Plugin-declared command names. `/name` for one of these dispatches to
    /// the daemon's `plugin.run_command` instead of forwarding to the agent
    /// as a chat message — plugin commands are invocations, not prose.
    plugin_command_names: std::collections::HashSet<String>,
}

// ─── View, update, message ───────────────────────────────────────────────────

/// How long a tool may run before it leaves the transcript as two immutable
/// nodes.
///
/// A tool that finishes inside this window stays one node, so ordinary fast
/// calls are unaffected. Timing is the whole rule: nothing declares itself
/// asynchronous, and a Lua tool that blocks for any reason is covered without
/// a schema change.
pub(crate) const BACKGROUND_TOOL_SPLIT_THRESHOLD: std::time::Duration =
    std::time::Duration::from_millis(500);

impl OilChatApp {
    /// Build the frame for the current state.
    pub fn view(&self, ctx: &ViewContext<'_>) -> Node {
        self.terminal_size.set(ctx.terminal_size);

        if let Some(ref modal) = self.shell_modal {
            let (w, h) = ctx.terminal_size;
            return modal.view(w as usize, h as usize);
        }

        let ctx = &ViewContext {
            frame_time: self.frame_time,
            spinner_frame: self.spinner_frame(),
            show_thinking: self.show_thinking,
            show_diffs: self.show_diffs,
            ..*ctx
        };

        // `top` and `bottom` frame the whole app, so they render regardless of
        // which footer surface is up — a modal or the messages drawer replaces
        // the command panel, not the window chrome around it.
        use crucible_lua::statusline_items::Region;
        let status = self.build_status_component();
        // Neither region may hold the input — `Layout::from_wire` strips a stray
        // one — so an empty node here is unreachable, not a fallback.
        let top_bars = status.render_region(Region::Top, || Node::Empty);
        let bottom_bars = status.render_region(Region::Bottom, || Node::Empty);

        col(top_bars
            .into_iter()
            .chain([
                // Transcript area. Every node renders every frame; the terminal
                // scrolls rows off the top and keeps them in its scrollback.
                flex(
                    1,
                    slot(
                        "content",
                        [col({
                            let nodes = self.container_list.nodes();
                            nodes.iter().enumerate().map(|(i, node)| {
                                let prev = if i > 0 { Some(&nodes[i - 1]) } else { None };
                                node.render(prev, ctx)
                            })
                        })
                        .gap(Gap::row(1))],
                    ),
                ),
                // Pinned footer
                slot(
                    "footer",
                    [col(
                        match (&self.interaction_modal, self.notification_area.is_visible()) {
                            (Some(modal), _) => vec![modal.view(
                                ctx.terminal_size.0 as usize,
                                self.permission.permission_queue.len(),
                            )],
                            (_, true) => vec![self.render_messages_drawer(ctx)],
                            _ => vec![self.build_command_panel(ctx).view(ctx)],
                        },
                    )
                    .gap(Gap::row(1))],
                ),
            ])
            .chain(bottom_bars)
            .chain([
                // Overlay
                self.popup_overlay_view(ctx),
            ]))
        .gap(Gap::row(1))
    }

    /// Apply one terminal event.
    pub fn update(&mut self, event: Event) -> Action<ChatAppMsg> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                self.input.insert_str(&text);
                self.check_autocomplete_trigger()
                    .unwrap_or(Action::Continue)
            }
            Event::Tick => {
                // Shell modal tick polls for child process output.
                // Also runs in render_frame() via expire_toasts(), but kept
                // here for tests that call update(Tick) directly.
                self.tick_shell_modal();
                Action::Continue
            }
            // Dimensions and previous-frame invalidation are handled by
            // Terminal::handle_resize before this event reaches the app.
            // The app holds no width-dependent cached state — every render
            // wraps from source — so there's nothing else to invalidate.
            Event::Resize { .. } => Action::Continue,
        }
    }

    /// Apply one application message.
    pub fn on_message(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
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
}

// ─── Accessors & Lifecycle ───────────────────────────────────────────────────

impl OilChatApp {
    /// Whether the daemon's last-known mode list contains `id`.
    ///
    /// A `false` means our list is stale, not that the mode is invalid — the
    /// daemon is the authority, and it just told us about this one.
    pub(crate) fn knows_mode(&self, id: &str) -> bool {
        self.available_modes
            .iter()
            .any(|m| m.eq_ignore_ascii_case(id))
    }

    pub(crate) fn set_mode(&mut self, mode: impl Into<std::sync::Arc<str>>) {
        self.mode = mode.into();
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

    /// Register plugin-declared commands: names route `/name` to the daemon's
    /// `plugin.run_command`, and each also joins the slash autocomplete list.
    pub(crate) fn set_plugin_commands(&mut self, commands: Vec<(String, String)>) {
        for (name, description) in commands {
            if !self.slash_commands.iter().any(|(n, _)| n == &name) {
                self.slash_commands
                    .push((name.clone(), format!("{description} (plugin)")));
            }
            self.plugin_command_names.insert(name);
        }
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

    /// Tests seed the model list without a daemon round trip.
    #[cfg(test)]
    pub(crate) fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models.clone();
        if !models.is_empty() {
            self.model_list_state = ModelListState::Loaded;
        }
    }

    #[cfg(test)]
    pub(crate) fn model_list_state(&self) -> &ModelListState {
        &self.model_list_state
    }

    #[cfg(test)]
    pub(crate) fn available_models(&self) -> &[String] {
        &self.available_models
    }

    pub(crate) fn set_show_thinking(&mut self, show: bool) {
        self.show_thinking = show;
    }

    pub(crate) fn set_show_diffs(&mut self, show: bool) {
        self.show_diffs = show;
    }

    #[cfg(test)]
    pub(crate) fn show_diffs(&self) -> bool {
        self.show_diffs
    }

    /// Spinner frame derived from the frame clock (100ms per frame).
    /// Independent of tick events — animates even during rapid streaming.
    pub fn spinner_frame(&self) -> usize {
        let since_epoch = self
            .frame_time
            .saturating_duration_since(self.spinner_epoch);
        (since_epoch.as_millis() / 100) as usize
    }

    /// Set the frame clock.
    ///
    /// The runner reads the wall clock once per loop iteration and stores it
    /// here. Everything that measures time — the spinner, a running tool's
    /// elapsed text, the slow-tool split — reads this value, never
    /// `Instant::now()`. So the frame is a function of the app state alone: a
    /// replay that never calls this renders the same on any machine, however
    /// slow, and a test can move time forward by an exact amount.
    pub fn set_frame_time(&mut self, now: std::time::Instant) {
        self.frame_time = now;
    }

    /// The frame clock. Messages stamp new tools and agents with it.
    pub(crate) fn frame_time(&self) -> std::time::Instant {
        self.frame_time
    }

    // ─── View Helpers (chrome composition) ─────────────────────────────

    /// Build the footer command panel (turn indicator + input + status).
    fn build_command_panel<'a>(&'a self, ctx: &ViewContext<'_>) -> CommandPanel<'a> {
        use crate::tui::oil::components::TurnIndicator;
        use crate::tui::oil::components::{InputComponent, InputMode as ComponentInputMode};

        // Turn indicator (bare spinner — thinking content renders inline)
        let mut indicator = TurnIndicator::new();
        indicator.active = self.container_list.is_streaming();

        // Input
        let input_mode = ComponentInputMode::from_content(self.input.content());
        let is_focused = self.interaction_modal.is_none();
        let term_width = ctx.terminal_size.0 as usize;
        let input = InputComponent::new(self.input.content(), self.input.cursor(), term_width)
            .mode(input_mode)
            .focused(is_focused)
            .show_popup(self.panel_popup_is_open());

        CommandPanel {
            turn_indicator: indicator,
            input,
            status: self.build_status_component(),
        }
    }

    /// The frame's status snapshot, shared by every anchor.
    ///
    /// Bars at different anchors must agree, so they all read one snapshot
    /// rather than each rebuilding from `self`.
    fn build_status_component(&self) -> StatusComponent<'_> {
        let mut status = StatusComponent::new()
            .mode(&self.mode)
            .model(&self.model)
            .context(self.context_used, self.context_total)
            .cache_hit_rate(self.cache_hit_rate)
            .streaming(self.container_list.is_streaming())
            .background_tasks(self.container_list.background_task_count())
            .status(&self.status);
        if let Some((text, kind)) = self.notification_area.active_toast() {
            status = status.toast(text, kind);
        }
        let counts = self.notification_area.warning_counts();
        if !counts.is_empty() {
            status = status.counts(counts);
        }
        status
    }

    /// Messages drawer (notification history).
    fn render_messages_drawer(&self, ctx: &ViewContext<'_>) -> Node {
        use crate::tui::oil::components::status_bar::NotificationToastKind;
        use crate::tui::oil::components::{NotificationComponent, NotificationEntry};

        let term_width = ctx.terminal_size.0 as usize;
        let entries: Vec<NotificationEntry> = self
            .notification_area
            .history()
            .iter()
            .map(|(notif, instant)| {
                let kind = match &notif.kind {
                    crucible_core::types::NotificationKind::Toast => NotificationToastKind::Info,
                    crucible_core::types::NotificationKind::Progress { .. } => {
                        NotificationToastKind::Info
                    }
                    crucible_core::types::NotificationKind::Warning => {
                        NotificationToastKind::Warning
                    }
                };
                let elapsed = instant.elapsed();
                let created =
                    chrono::Local::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
                let timestamp = created.format("%H:%M:%S").to_string();
                let message = notif.message.trim_end();
                NotificationEntry::new(message, kind, timestamp)
            })
            .collect();

        NotificationComponent::new(entries)
            .visible(true)
            .width(term_width)
            .view(ctx)
    }

    /// Popup overlay for command completion.
    /// Effective completion popup presentation from the `:set` knob.
    /// `auto` (default) = minimal for inline triggers, panel for commands.
    fn completion_style(&self) -> &'static str {
        // Both key spellings are accepted by `:set`; stored under whichever
        // the user typed.
        for key in ["completion_style", "completionstyle"] {
            if let Some(v) = self.runtime_config.get(key) {
                match v.as_string() {
                    Some("panel") => return "panel",
                    Some("minimal") => return "minimal",
                    _ => {}
                }
            }
        }
        "auto"
    }

    /// Rows the completion popup shows before it scrolls.
    ///
    /// `cru.geometry.setup{ popup = { max_visible = N } }` overrides the
    /// built-in. The popup renders this many rows whatever the item count, so
    /// it is also the height the frame must reserve for it.
    fn popup_max_visible(&self) -> usize {
        crate::tui::oil::theme::geometry::active()
            .popup
            .max_visible
            .map_or(POPUP_HEIGHT, usize::from)
    }

    /// Rows the popup must clear: the input itself, plus whatever the author
    /// put under it, plus the bottom region.
    ///
    /// This used to be a hardcoded constant that happened to equal the footer
    /// height back when the footer was always one bar. Now that a region is a
    /// list the author controls, the only correct source is the layout.
    fn popup_offset_from_bottom(&self, ctx: &ViewContext<'_>) -> usize {
        use crate::tui::oil::components::InputMode as ComponentInputMode;

        let layout = crate::tui::oil::theme::bars::active();
        let input_height = InputComponent::new(
            self.input.content(),
            self.input.cursor(),
            ctx.terminal_size.0 as usize,
        )
        .mode(ComponentInputMode::from_content(self.input.content()))
        .height();
        input_height + layout.rows_below_input() + layout.bottom.len()
    }

    /// Whether the popup floats (nvim-pmenu style) rather than extending the
    /// prompt as a panel.
    ///
    /// Inline triggers complete a word inside the message being written;
    /// command triggers (`:` and `/`) change the message type and complete a
    /// whole entry line.
    fn popup_is_minimal(&self) -> bool {
        let inline_trigger = matches!(
            self.popup.kind,
            state::AutocompleteKind::File | state::AutocompleteKind::Note
        );
        match self.completion_style() {
            "panel" => false,
            "minimal" => true,
            _ => inline_trigger,
        }
    }

    /// Whether the panel popup is on screen right now.
    ///
    /// The prompt draws a solid top edge under it, so the two read as one
    /// surface instead of a panel with a half-lit row under it.
    pub(crate) fn panel_popup_is_open(&self) -> bool {
        self.popup.show && !self.popup_is_minimal() && !self.get_popup_items().is_empty()
    }

    /// Rows the frame reserves so the popup never moves the prompt.
    ///
    /// The popup draws over the rows above the prompt. A frame shorter than
    /// the popup would have to grow to hold it, which moves the prompt down
    /// the moment a completion opens. Reserving the tallest popup keeps one
    /// height whether the popup is open or not.
    pub(crate) fn min_viewport_rows(&self, ctx: &ViewContext<'_>) -> u16 {
        let rows = self.popup_max_visible() + self.popup_offset_from_bottom(ctx);
        u16::try_from(rows).unwrap_or(u16::MAX)
    }

    fn popup_overlay_view(&self, ctx: &ViewContext<'_>) -> Node {
        if !self.popup.show {
            return Node::Empty;
        }

        let items = self.get_popup_items();
        if items.is_empty() {
            return Node::Empty;
        }

        use crate::tui::oil::components::{InputMode as ComponentInputMode, PopupOverlay};

        let minimal = self.popup_is_minimal();

        let mode = ComponentInputMode::from_content(self.input.content());
        let max_visible = self.popup_max_visible();
        let offset_from_bottom = self.popup_offset_from_bottom(ctx);

        let overlay = PopupOverlay::new(items)
            .selected(self.popup.selected)
            .max_visible(max_visible)
            .offset_from_bottom(offset_from_bottom);

        let overlay = if minimal {
            // nvim-pmenu style: content-width box anchored at the trigger's
            // display column so item labels align with the word being
            // completed; floats on the themed popup surface.
            let t = crate::tui::oil::theme::active();
            let prompt_width = mode.prompt().len();
            let content_width = (ctx.terminal_size.0 as usize)
                .saturating_sub(prompt_width + 1)
                .max(1);
            let chars_before = self.input.content()[..self.popup.trigger_pos]
                .chars()
                .count();
            let display_pos = mode.display_cursor(chars_before);
            let anchor = prompt_width + (display_pos % content_width);

            use crate::tui::oil::theme::groups;
            overlay
                .bg(groups::bg_or("Popup", t.resolve_color(t.colors.popup_bg)))
                .selected_bg(groups::bg_or(
                    "PopupSelected",
                    t.resolve_color(t.colors.popup_selected_bg),
                ))
                .anchor_col(anchor as u16)
        } else {
            // Panel strip: the popup extends the prompt, so it shares the
            // CURRENT input mode's (possibly user-themed) bg — command-mode
            // completions match the `:` prompt, shell the `!` prompt.
            overlay.bg(mode.bg_color())
        };

        overlay.view(&crucible_oil::focus::FocusContext::default())
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

    pub(crate) fn set_precognition_results(&mut self, count: usize) {
        self.precognition.precognition_results = count;
    }

    #[cfg(test)]
    pub(crate) fn container_list(&self) -> &crate::tui::oil::containers::ContainerList {
        &self.container_list
    }

    #[cfg(test)]
    pub(crate) fn container_list_mut(&mut self) -> &mut crate::tui::oil::containers::ContainerList {
        &mut self.container_list
    }

    pub(crate) fn add_notification(&mut self, notification: crucible_core::types::Notification) {
        self.notification_area.add(notification);
    }

    /// Open the notification panel, so a story can assert on its content.
    #[cfg(test)]
    pub(crate) fn show_messages(&mut self) {
        self.notification_area.show();
    }

    /// Move any tool past the split threshold out of the transcript.
    ///
    /// Called once per frame. A tool that finishes quickly is never split, so
    /// the common case keeps a single node.
    pub(crate) fn split_slow_tools(&mut self) -> bool {
        self.container_list
            .split_slow_tools(self.frame_time, BACKGROUND_TOOL_SPLIT_THRESHOLD)
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

    #[cfg(test)]
    pub(crate) fn input_content(&self) -> &str {
        self.input.content()
    }

    #[cfg(test)]
    pub(crate) fn mode(&self) -> &str {
        &self.mode
    }

    #[cfg(test)]
    pub(crate) fn status_text(&self) -> &str {
        &self.status
    }

    #[cfg(test)]
    pub(crate) fn context_usage(&self) -> (usize, usize) {
        (self.context_used, self.context_total)
    }

    #[cfg(test)]
    pub(crate) fn current_model(&self) -> &str {
        &self.model
    }

    #[cfg(test)]
    pub(crate) fn has_notifications(&self) -> bool {
        !self.notification_area.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn has_interaction_modal(&self) -> bool {
        self.interaction_modal.is_some()
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
            // NOTE: permission_pending was removed — the component model handles
            // graduation via explicit state transitions.

            if self.interaction_modal.is_some() {
                self.permission
                    .permission_queue
                    .push_back((request_id, perm.clone()));
                return Action::Continue;
            }
        }

        self.notification_area.hide();

        self.interaction_modal = Some(
            InteractionModal::new(request_id, request, self.permission.perm_show_diff)
                .with_full_commands(self.permission.perm_full_commands),
        );
        Action::Continue
    }

    fn close_interaction(&mut self) {
        self.interaction_modal = None;
    }

    #[cfg(test)]
    pub(crate) fn interaction_visible(&self) -> bool {
        self.interaction_modal.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_input_content(&mut self, content: &str) {
        self.input.handle(InputAction::Clear);
        for ch in content.chars() {
            self.input.handle(InputAction::Insert(ch));
        }
    }

    pub(crate) fn take_needs_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_full_redraw)
    }

    fn add_user_message(&mut self, content: String) {
        self.container_list.add_user_message(content);
        self.message_queue.message_counter += 1;
    }

    fn submit_user_message(&mut self, content: String) {
        self.add_user_message(content);
        self.container_list.mark_turn_active();
    }

    /// Name the kilns the session draws knowledge from, in the transcript.
    ///
    /// The first thing on screen says what the session is attached to, so a
    /// wrong or empty attachment is visible before the first turn.
    ///
    /// `pending` is how many proposals wait in those kilns. A proposal that
    /// nobody knows about is never reviewed, so the banner ends by saying how
    /// many wait and how to review them. Zero adds nothing.
    pub(crate) fn announce_kilns(&mut self, kilns: &[KilnSummary], pending: usize) {
        if kilns.is_empty() {
            self.add_system_message(
                "No kiln is attached. Notes, search and knowledge tools have nothing to read."
                    .to_string(),
            );
            return;
        }

        let width = kilns
            .iter()
            .map(|k| k.name.chars().count())
            .max()
            .unwrap_or(0);
        let mut text = format!(
            "{} kiln{} attached",
            kilns.len(),
            if kilns.len() == 1 { "" } else { "s" }
        );
        for kiln in kilns {
            text.push_str(&format!(
                "\n     {:width$}  {}",
                kiln.name,
                kiln.path,
                width = width
            ));
        }
        if pending > 0 {
            text.push_str(&format!(
                "\n{pending} proposal{} pending. Review with `cru proposals list`.",
                if pending == 1 { "" } else { "s" }
            ));
        }
        self.add_system_message(text);
    }

    pub(crate) fn add_system_message(&mut self, content: String) {
        self.container_list.add_system_message(content);
        self.message_queue.message_counter += 1;
    }

    fn finalize_streaming(&mut self) {
        self.status = "Ready".to_string();
    }

    pub(crate) fn reset_session(&mut self) {
        self.container_list.clear();
        self.message_queue.message_counter = 0;
        self.context_used = 0;
        self.context_total = 0;
        self.status = "Ready".to_string();
        self.notification_area.clear();
        self.pending_delegate_supersessions.clear();
        self.needs_full_redraw = true;
    }
}

#[cfg(test)]
mod tests;
