use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_container::ContainerList;
use crate::tui::oil::commands::SetCommand;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{
    Drawer, DrawerKind, InteractionModal, InteractionModalMsg, InteractionModalOutput,
    InteractionMode, NotificationArea, ShellHistoryItem, ShellModal, ShellModalMsg,
    ShellModalOutput, ShellStatus, StatusBar,
};
use crate::tui::oil::config::{ConfigValue, ModSource, RuntimeConfig};
use crate::tui::oil::event::{Event, InputAction, InputBuffer};
use crate::tui::oil::markdown::{
    markdown_to_node_styled, markdown_to_node_with_width, Margins, RenderStyle,
};
use crate::tui::oil::node::*;
use crate::tui::oil::style::{Color, Gap, Padding, Style};
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::utils::terminal_width;
use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};
use crossterm::event::KeyCode;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use crucible_core::interaction::{
    AskRequest, AskResponse, InteractionRequest, InteractionResponse, PermAction, PermRequest,
    PermResponse, PermissionScope,
};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const FOCUS_INPUT: &str = "input";
const FOCUS_POPUP: &str = "popup";
const POPUP_HEIGHT: usize = 10;
pub const INPUT_MAX_CONTENT_LINES: usize = 3;

const MAX_DISPLAY_ITEMS: usize = 512;
const MAX_SHELL_HISTORY: usize = 100;

fn wrap_content(content: &str, max_width: usize) -> Vec<String> {
    if content.is_empty() || max_width == 0 {
        return vec![String::new()];
    }

    let chars: Vec<char> = content.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + max_width).min(chars.len());
        lines.push(chars[start..end].iter().collect());
        start = end;
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

#[derive(Debug, Clone)]
pub enum ChatAppMsg {
    UserMessage(String),
    TextDelta(String),
    ThinkingDelta(String),
    ToolCall {
        name: String,
        args: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultDelta {
        name: String,
        delta: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultComplete {
        name: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultError {
        name: String,
        error: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    StreamComplete,
    StreamCancelled,
    Error(String),
    Status(String),
    ModeChanged(String),
    ContextUsage {
        used: usize,
        total: usize,
    },
    ClearHistory,
    QueueMessage(String),
    SwitchModel(String),
    FetchModels,
    ModelsLoaded(Vec<String>),
    ModelsFetchFailed(String),
    SetThinkingBudget(i64),
    SetTemperature(f64),
    SetMaxTokens(Option<u32>),
    SubagentSpawned {
        id: String,
        prompt: String,
    },
    SubagentCompleted {
        id: String,
        summary: String,
    },
    SubagentFailed {
        id: String,
        error: String,
    },
    ToggleMessages,
    OpenInteraction {
        request_id: String,
        request: InteractionRequest,
    },
    CloseInteraction {
        request_id: String,
        response: InteractionResponse,
    },
}

#[derive(Debug, Clone)]
pub enum ChatItem {
    Message {
        id: String,
        role: Role,
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: String,
        result: String,
        complete: bool,
    },
    /// Shell command execution - display only, never sent to agent
    ShellExecution {
        id: String,
        command: String,
        exit_code: i32,
        output_tail: Vec<String>,
        output_path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatMode {
    #[default]
    Normal,
    Plan,
    Auto,
}

impl ChatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatMode::Normal => "normal",
            ChatMode::Plan => "plan",
            ChatMode::Auto => "auto",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "plan" => ChatMode::Plan,
            "auto" => ChatMode::Auto,
            _ => ChatMode::Normal,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            ChatMode::Normal => ChatMode::Plan,
            ChatMode::Plan => ChatMode::Auto,
            ChatMode::Auto => ChatMode::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Command,
    Shell,
}

impl InputMode {
    pub fn bg_color(&self) -> Color {
        let theme = ThemeTokens::default_ref();
        match self {
            InputMode::Normal => theme.input_bg,
            InputMode::Command => theme.command_bg,
            InputMode::Shell => theme.shell_bg,
        }
    }

    pub fn prompt(&self) -> &'static str {
        match self {
            InputMode::Normal => " > ",
            InputMode::Command => " : ",
            InputMode::Shell => " ! ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AutocompleteKind {
    #[default]
    None,
    File,
    Note,
    Command,
    SlashCommand,
    ReplCommand,
    Model,
    CommandArg {
        command: String,
        arg_index: usize,
    },
    SetOption {
        option: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelListState {
    #[default]
    NotLoaded,
    Loading,
    Loaded,
    Failed(String),
}

pub struct McpServerDisplay {
    pub name: String,
    pub prefix: String,
    pub tool_count: usize,
    pub connected: bool,
}

pub struct OilChatApp {
    /// Semantic containers for chat content (container architecture)
    container_list: ContainerList,
    input: InputBuffer,
    spinner_frame: usize,
    mode: ChatMode,
    model: String,
    status: String,
    error: Option<String>,
    message_counter: usize,
    on_submit: Option<Box<dyn Fn(String) + Send + Sync>>,
    show_popup: bool,
    popup_selected: usize,
    popup_kind: AutocompleteKind,
    popup_filter: String,
    popup_trigger_pos: usize,
    workspace_files: Vec<String>,
    kiln_notes: Vec<String>,
    attached_context: Vec<String>,
    context_used: usize,
    context_total: usize,
    last_ctrl_c: Option<std::time::Instant>,
    shell_modal: Option<ShellModal>,
    shell_history: VecDeque<String>,
    shell_history_index: Option<usize>,
    session_dir: Option<PathBuf>,
    needs_full_redraw: bool,
    mcp_servers: Vec<McpServerDisplay>,
    deferred_messages: VecDeque<String>,
    available_models: Vec<String>,
    model_list_state: ModelListState,
    show_thinking: bool,
    runtime_config: RuntimeConfig,
    current_provider: String,
    notification_area: NotificationArea,
    interaction_modal: Option<InteractionModal>,
    /// Queue of pending permission requests (request_id, request) when multiple arrive rapidly
    permission_queue: VecDeque<(String, PermRequest)>,
    /// Whether to show diff by default in permission prompts (session-scoped)
    perm_show_diff: bool,
    /// Whether to auto-allow all permission prompts for this session
    perm_autoconfirm_session: bool,
}

impl Default for OilChatApp {
    fn default() -> Self {
        Self {
            container_list: ContainerList::new(),
            input: InputBuffer::new(),
            spinner_frame: 0,
            mode: ChatMode::Normal,
            model: String::new(),
            status: String::new(),
            error: None,
            message_counter: 0,
            on_submit: None,
            show_popup: false,
            popup_selected: 0,
            popup_kind: AutocompleteKind::None,
            popup_filter: String::new(),
            popup_trigger_pos: 0,
            workspace_files: Vec::new(),
            kiln_notes: Vec::new(),
            attached_context: Vec::new(),
            context_used: 0,
            context_total: 0,
            last_ctrl_c: None,
            shell_modal: None,
            shell_history: VecDeque::with_capacity(MAX_SHELL_HISTORY),
            shell_history_index: None,
            session_dir: None,
            needs_full_redraw: false,
            mcp_servers: Vec::new(),
            deferred_messages: VecDeque::new(),
            available_models: Vec::new(),
            model_list_state: ModelListState::NotLoaded,
            show_thinking: true,
            runtime_config: RuntimeConfig::empty(),
            current_provider: "local".to_string(),
            notification_area: NotificationArea::new(),
            interaction_modal: None,
            permission_queue: VecDeque::new(),
            perm_show_diff: true,
            perm_autoconfirm_session: false,
        }
    }
}

impl App for OilChatApp {
    type Msg = ChatAppMsg;

    fn init() -> Self {
        Self::default()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        if self.shell_modal.is_some() {
            return self.render_shell_modal();
        }

        if let Some(modal) = &self.interaction_modal {
            match &modal.request {
                InteractionRequest::Ask(_)
                | InteractionRequest::AskBatch(_)
                | InteractionRequest::Permission(_) => {
                    let (term_width, _) = crossterm::terminal::size()
                        .map(|(w, h)| (w as usize, h as usize))
                        .unwrap_or((80, 24));
                    return modal.view(term_width, self.permission_queue.len());
                }
                _ => {} // Other types not yet supported
            }
        }

        let bottom = if self.notification_area.is_visible() {
            self.render_messages_drawer()
        } else {
            col([self.render_input(ctx), self.render_status()])
        };

        col([
            self.render_containers(),
            self.render_error(),
            spacer(),
            // Space character creates a visible blank line for separation before input area
            text(" "),
            bottom,
            self.render_popup_overlay(),
        ])
        .gap(Gap::row(0))
    }

    fn update(&mut self, event: Event) -> Action<ChatAppMsg> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Tick => {
                self.spinner_frame = (self.spinner_frame + 1) % 4;
                self.tick_shell_modal();
                self.notification_area.expire_toasts();
                if self.notification_area.is_empty() {
                    self.notification_area.hide();
                }
                Action::Continue
            }
            Event::Resize { .. } => Action::Continue,
            Event::Quit => Action::Quit,
        }
    }

    fn on_message(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(content) => {
                self.submit_user_message(content);
                Action::Continue
            }
            ChatAppMsg::TextDelta(delta) => {
                self.container_list.append_text(&delta);
                Action::Continue
            }
            ChatAppMsg::ThinkingDelta(delta) => {
                self.container_list.append_thinking(&delta);
                Action::Continue
            }
            ChatAppMsg::ToolCall {
                name,
                args,
                call_id,
            } => {
                if !self.container_list.is_streaming() {
                    self.container_list.mark_turn_active();
                }
                self.message_counter += 1;
                let tool_id = format!("tool-{}", self.message_counter);
                tracing::debug!(
                    tool_name = %name,
                    ?call_id,
                    args_len = args.len(),
                    counter = self.message_counter,
                    "Adding ToolCall"
                );
                let mut tool = CachedToolCall::new(tool_id, &name, &args);
                tool.call_id = call_id;
                self.container_list.add_tool_call(tool);
                Action::Continue
            }
            ChatAppMsg::ToolResultDelta {
                name,
                delta,
                call_id,
            } => {
                tracing::debug!(
                    tool_name = %name,
                    ?call_id,
                    delta_len = delta.len(),
                    "Received ToolResultDelta"
                );
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.append_output(&delta);
                    });
                self.maybe_spill_tool_output(&name);
                Action::Continue
            }
            ChatAppMsg::ToolResultComplete { name, call_id } => {
                tracing::debug!(tool_name = %name, ?call_id, "Received ToolResultComplete");
                self.maybe_spill_tool_output(&name);
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.mark_complete();
                    });
                Action::Continue
            }
            ChatAppMsg::ToolResultError {
                name,
                error,
                call_id,
            } => {
                tracing::debug!(tool_name = %name, ?call_id, error = %error, "Received ToolResultError");
                self.container_list
                    .update_tool(&name, call_id.as_deref(), |t| {
                        t.set_error(error);
                    });
                Action::Continue
            }
            ChatAppMsg::StreamComplete => {
                self.finalize_streaming();
                self.process_deferred_queue()
            }
            ChatAppMsg::StreamCancelled => {
                self.finalize_streaming();
                self.status = "Cancelled".to_string();
                self.process_deferred_queue()
            }
            ChatAppMsg::QueueMessage(content) => {
                self.deferred_messages.push_back(content);
                let count = self.deferred_messages.len();
                self.status = format!(
                    "{} message{} queued",
                    count,
                    if count == 1 { "" } else { "s" }
                );
                Action::Continue
            }
            ChatAppMsg::Error(msg) => {
                self.error = Some(msg);
                self.container_list.cancel_streaming();
                Action::Continue
            }
            ChatAppMsg::Status(status) => {
                self.status = status;
                Action::Continue
            }
            ChatAppMsg::ModeChanged(mode) => {
                self.mode = ChatMode::parse(&mode);
                Action::Continue
            }
            ChatAppMsg::ContextUsage { used, total } => {
                self.context_used = used;
                self.context_total = total;
                Action::Continue
            }
            ChatAppMsg::ClearHistory => Action::Continue,
            ChatAppMsg::SwitchModel(model) => {
                self.model = model;
                self.status = format!("Model: {}", self.model);
                Action::Continue
            }
            ChatAppMsg::FetchModels => {
                self.model_list_state = ModelListState::Loading;
                self.status = "Fetching models...".to_string();
                Action::Continue
            }
            ChatAppMsg::ModelsLoaded(models) => {
                self.available_models = models;
                self.model_list_state = ModelListState::Loaded;
                if self.popup_kind == AutocompleteKind::Model && self.show_popup {
                    self.popup_selected = 0;
                }
                Action::Continue
            }
            ChatAppMsg::ModelsFetchFailed(reason) => {
                self.model_list_state = ModelListState::Failed(reason.clone());
                self.error = Some(format!("Failed to fetch models: {}", reason));
                Action::Continue
            }
            ChatAppMsg::SetThinkingBudget(_) => Action::Continue,
            ChatAppMsg::SetTemperature(_) => Action::Continue,
            ChatAppMsg::SetMaxTokens(_) => Action::Continue,
            ChatAppMsg::SubagentSpawned { id, prompt } => {
                if !self.container_list.is_streaming() {
                    self.container_list.mark_turn_active();
                }
                self.container_list
                    .add_subagent(CachedSubagent::new(id, &prompt));
                Action::Continue
            }
            ChatAppMsg::SubagentCompleted { id, summary } => {
                self.container_list.update_subagent(&id, |s| {
                    s.mark_completed(&summary);
                });
                Action::Continue
            }
            ChatAppMsg::SubagentFailed { id, error } => {
                self.container_list.update_subagent(&id, |s| {
                    s.mark_failed(&error);
                });
                Action::Continue
            }
            ChatAppMsg::ToggleMessages => {
                self.notification_area.toggle();
                Action::Continue
            }
            ChatAppMsg::OpenInteraction {
                request_id,
                request,
            } => {
                self.open_interaction(request_id, request);
                Action::Continue
            }
            ChatAppMsg::CloseInteraction {
                request_id: _,
                response: _,
            } => {
                // Response handling will be implemented in a later task
                self.close_interaction();
                Action::Continue
            }
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(100))
    }
}

impl OilChatApp {
    pub fn with_on_submit<F>(mut self, callback: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.on_submit = Some(Box::new(callback));
        self
    }

    pub fn set_mode(&mut self, mode: ChatMode) {
        self.mode = mode;
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn set_workspace_files(&mut self, files: Vec<String>) {
        self.workspace_files = files;
    }

    pub fn set_kiln_notes(&mut self, notes: Vec<String>) {
        self.kiln_notes = notes;
    }

    pub fn set_session_dir(&mut self, path: PathBuf) {
        self.session_dir = Some(path);
    }

    pub fn set_mcp_servers(&mut self, servers: Vec<McpServerDisplay>) {
        self.mcp_servers = servers;
    }

    pub fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    pub fn set_show_thinking(&mut self, show: bool) {
        self.show_thinking = show;
    }

    pub fn perm_show_diff(&self) -> bool {
        self.perm_show_diff
    }

    pub fn perm_autoconfirm_session(&self) -> bool {
        self.perm_autoconfirm_session
    }

    /// Get access to the container list for testing/inspection.
    #[cfg(test)]
    pub fn container_list(&self) -> &ContainerList {
        &self.container_list
    }

    pub fn add_notification(&mut self, notification: crucible_core::types::Notification) {
        self.notification_area.add(notification);
    }

    pub fn toggle_messages(&mut self) {
        self.notification_area.toggle();
    }

    pub fn show_messages(&mut self) {
        self.notification_area.show();
    }

    pub fn hide_messages(&mut self) {
        self.notification_area.hide();
    }

    pub fn clear_notifications(&mut self) {
        self.notification_area.clear();
    }

    fn render_messages_drawer(&self) -> Node {
        use crate::tui::oil::components::status_bar::NotificationToastKind;
        use crate::tui::oil::node::{row, styled};
        use crate::tui::oil::style::Style;
        use crate::tui::oil::theme::ThemeTokens;

        let theme = ThemeTokens::default_ref();
        let term_width = terminal_width();

        let content_rows: Vec<Node> = self
            .notification_area
            .history()
            .iter()
            .map(|(notif, instant)| {
                let elapsed = instant.elapsed();
                let secs_ago = elapsed.as_secs();
                let timestamp = if secs_ago < 60 {
                    format!("{:>2}s ago", secs_ago)
                } else if secs_ago < 3600 {
                    format!("{:>2}m ago", secs_ago / 60)
                } else {
                    format!("{:>2}h ago", secs_ago / 3600)
                };

                let (kind_label, badge_kind): (&str, NotificationToastKind) = match &notif.kind {
                    crucible_core::types::NotificationKind::Toast => {
                        ("INFO", NotificationToastKind::Info)
                    }
                    crucible_core::types::NotificationKind::Progress { .. } => {
                        ("INFO", NotificationToastKind::Info)
                    }
                    crucible_core::types::NotificationKind::Warning => {
                        ("WARN", NotificationToastKind::Warning)
                    }
                };

                let bg = theme.input_bg;
                let text_style = Style::new().bg(bg).fg(theme.overlay_text);
                let badge_style = theme.notification_badge(badge_kind.color());

                let timestamp_part = format!(" {}: ", timestamp);
                let message_part = format!(" {}", notif.message);
                let badge_text = format!(" {} ", kind_label);
                let used = timestamp_part.chars().count()
                    + badge_text.chars().count()
                    + message_part.chars().count();
                let padding = if term_width > used {
                    " ".repeat(term_width - used)
                } else {
                    String::new()
                };

                row([
                    styled(timestamp_part, text_style),
                    styled(badge_text, badge_style),
                    styled(message_part, text_style),
                    styled(padding, Style::new().bg(bg)),
                ])
            })
            .collect();

        Drawer::new(DrawerKind::Messages)
            .content_rows(content_rows)
            .width(term_width)
            .view()
    }

    pub fn clear_messages(&mut self) {
        self.notification_area.clear();
    }

    pub fn mark_graduated(&mut self, ids: impl IntoIterator<Item = String>) {
        let ids: Vec<String> = ids.into_iter().collect();
        self.container_list.graduate(&ids);
    }

    pub fn load_previous_messages(&mut self, items: Vec<ChatItem>) {
        self.container_list.clear();
        for item in &items {
            match item {
                ChatItem::Message { role, content, .. } => match role {
                    Role::User => {
                        self.container_list.add_user_message(content.clone());
                    }
                    Role::Assistant => {
                        self.container_list.start_assistant_response();
                        self.container_list.append_text(content);
                        self.container_list.complete_response();
                    }
                    Role::System => {
                        self.container_list.add_system_message(content.clone());
                    }
                },
                ChatItem::ToolCall {
                    id,
                    name,
                    args,
                    result,
                    complete,
                } => {
                    let mut tool = CachedToolCall::new(id, name, args);
                    if !result.is_empty() {
                        tool.append_output(result);
                    }
                    if *complete {
                        tool.complete = true;
                    }
                    self.container_list.add_tool_call(tool);
                }
                ChatItem::ShellExecution {
                    id,
                    command,
                    exit_code,
                    output_tail,
                    output_path,
                } => {
                    self.container_list
                        .add_shell_execution(CachedShellExecution::new(
                            id,
                            command,
                            *exit_code,
                            output_tail.clone(),
                            output_path.clone(),
                        ));
                }
            }
        }
        self.message_counter = self.container_list.len();
    }

    fn push_shell_history(&mut self, cmd: String) {
        if self.shell_history.len() >= MAX_SHELL_HISTORY {
            self.shell_history.pop_front();
        }
        self.shell_history.push_back(cmd);
    }

    pub fn is_streaming(&self) -> bool {
        self.container_list.is_streaming()
    }

    pub fn input_content(&self) -> &str {
        self.input.content()
    }

    #[cfg(test)]
    pub fn is_popup_visible(&self) -> bool {
        self.show_popup
    }

    #[cfg(test)]
    pub fn current_popup_filter(&self) -> &str {
        &self.popup_filter
    }

    #[cfg(test)]
    pub fn current_model(&self) -> &str {
        &self.model
    }

    pub fn has_shell_modal(&self) -> bool {
        self.shell_modal.is_some()
    }

    pub fn open_interaction(&mut self, request_id: String, request: InteractionRequest) {
        if let InteractionRequest::Permission(perm) = &request {
            if self.interaction_modal.is_some() {
                self.permission_queue.push_back((request_id, perm.clone()));
                return;
            }
        }

        self.notification_area.hide();

        self.interaction_modal = Some(InteractionModal::new(request_id, request));
    }

    pub fn close_interaction(&mut self) {
        self.interaction_modal = None;
    }

    pub fn interaction_visible(&self) -> bool {
        self.interaction_modal.is_some()
    }

    #[cfg(test)]
    pub fn shell_output_lines(&self) -> Vec<String> {
        self.shell_modal
            .as_ref()
            .map(|m| m.output_lines().to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn shell_visible_lines(&self, max_lines: usize) -> Vec<String> {
        self.shell_modal
            .as_ref()
            .map(|m| m.visible_lines(max_lines).to_vec())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn shell_scroll_offset(&self) -> usize {
        self.shell_modal
            .as_ref()
            .map(|m| m.scroll_offset())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn set_input_content(&mut self, content: &str) {
        self.input.handle(InputAction::Clear);
        for ch in content.chars() {
            self.input.handle(InputAction::Insert(ch));
        }
    }

    #[cfg(test)]
    pub fn handle_input_action(&mut self, action: InputAction) {
        self.input.handle(action);
    }

    pub fn take_needs_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_full_redraw)
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        self.error = None;

        if self.notification_area.is_visible() {
            self.notification_area.hide();
            return Action::Continue;
        }

        if self.shell_modal.is_some() {
            return self.handle_shell_modal_key(key);
        }
        if self.interaction_modal.is_some() {
            return self.handle_interaction_key(key);
        }
        if self.is_streaming() {
            return self.handle_streaming_key(key);
        }

        if key.code == KeyCode::F(1) {
            self.toggle_command_palette();
            return Action::Continue;
        }

        if self.show_popup {
            return self.handle_popup_key(key);
        }

        if self.is_ctrl_c(key) {
            return self.handle_ctrl_c();
        }
        self.last_ctrl_c = None;

        // Handle Ctrl+T to toggle thinking display (works anytime, not just during streaming)
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        if key.code == KeyCode::Char('t') && ctrl {
            self.show_thinking = !self.show_thinking;
            let state = if self.show_thinking { "on" } else { "off" };
            self.notification_area
                .add(crucible_core::types::Notification::toast(format!(
                    "Thinking display: {}",
                    state
                )));
            return Action::Continue;
        }

        if key.code == KeyCode::BackTab {
            self.mode = self.mode.cycle();
            self.status = format!("Mode: {}", self.mode.as_str());
            return Action::Continue;
        }

        let action = InputAction::from(key);
        if let Some(submitted) = self.input.handle(action) {
            return self.handle_submit(submitted);
        }

        self.check_autocomplete_trigger()
            .unwrap_or(Action::Continue)
    }

    fn is_ctrl_c(&self, key: crossterm::event::KeyEvent) -> bool {
        key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
    }

    fn handle_ctrl_c(&mut self) -> Action<ChatAppMsg> {
        if !self.input.content().is_empty() {
            self.input.handle(InputAction::Clear);
            self.last_ctrl_c = None;
            return Action::Continue;
        }

        let now = std::time::Instant::now();
        if let Some(last) = self.last_ctrl_c {
            if now.duration_since(last) < Duration::from_millis(300) {
                return Action::Quit;
            }
        }
        self.last_ctrl_c = Some(now);
        self.notification_area
            .add(crucible_core::types::Notification::toast(
                "Ctrl+C again to quit",
            ));
        Action::Continue
    }

    fn toggle_command_palette(&mut self) {
        if self.show_popup {
            self.close_popup();
        } else {
            self.show_popup = true;
            self.popup_kind = AutocompleteKind::Command;
            self.popup_filter.clear();
        }
        self.popup_selected = 0;
    }

    fn close_popup(&mut self) {
        self.show_popup = false;
        self.popup_kind = AutocompleteKind::None;
        self.popup_filter.clear();
    }

    fn check_autocomplete_trigger(&mut self) -> Option<Action<ChatAppMsg>> {
        let content = self.input.content();
        let cursor = self.input.cursor();

        if let Some((kind, trigger_pos, filter)) = self.detect_trigger(content, cursor) {
            let needs_model_fetch = kind == AutocompleteKind::Model
                && self.model_list_state == ModelListState::NotLoaded;

            self.popup_kind = kind;
            self.popup_trigger_pos = trigger_pos;
            self.popup_filter = filter;
            self.popup_selected = 0;
            self.show_popup = !self.get_popup_items().is_empty();

            if needs_model_fetch {
                self.show_popup = true;
                return Some(Action::Send(ChatAppMsg::FetchModels));
            }
        } else if self.popup_kind != AutocompleteKind::None {
            self.popup_kind = AutocompleteKind::None;
            self.popup_filter.clear();
            self.show_popup = false;
        }
        None
    }

    fn detect_trigger(
        &self,
        content: &str,
        cursor: usize,
    ) -> Option<(AutocompleteKind, usize, String)> {
        let before_cursor = &content[..cursor];

        if let Some(slash_pos) = before_cursor.rfind('/') {
            let preceded_by_whitespace = slash_pos == 0
                || before_cursor[..slash_pos]
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace);
            if preceded_by_whitespace {
                let filter = &before_cursor[slash_pos + 1..];
                if !filter.contains(char::is_whitespace) {
                    return Some((
                        AutocompleteKind::SlashCommand,
                        slash_pos,
                        filter.to_string(),
                    ));
                }
            }
        }

        if let Some(at_pos) = before_cursor.rfind('@') {
            let after_at = &before_cursor[at_pos + 1..];
            if !after_at.contains(char::is_whitespace) {
                return Some((AutocompleteKind::File, at_pos, after_at.to_string()));
            }
        }

        if let Some(bracket_pos) = before_cursor.rfind("[[") {
            let after_bracket = &before_cursor[bracket_pos + 2..];
            if !after_bracket.contains("]]") {
                return Some((
                    AutocompleteKind::Note,
                    bracket_pos,
                    after_bracket.to_string(),
                ));
            }
        }

        if let Some(colon_pos) = before_cursor.rfind(':') {
            let preceded_by_whitespace = colon_pos == 0
                || before_cursor[..colon_pos]
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace);
            if preceded_by_whitespace {
                let after_colon = &before_cursor[colon_pos + 1..];
                if let Some(space_pos) = after_colon.find(char::is_whitespace) {
                    let command = after_colon[..space_pos].to_string();
                    let args_part = after_colon[space_pos..].trim_start();
                    let filter = args_part
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .to_string();
                    let trigger_pos = cursor - filter.len();

                    if command == "model" {
                        return Some((AutocompleteKind::Model, trigger_pos, filter));
                    }

                    if command == "set" {
                        return Some((
                            AutocompleteKind::SetOption { option: None },
                            trigger_pos,
                            filter,
                        ));
                    }

                    let arg_index = args_part.split_whitespace().count();
                    return Some((
                        AutocompleteKind::CommandArg { command, arg_index },
                        trigger_pos,
                        filter,
                    ));
                } else {
                    return Some((
                        AutocompleteKind::ReplCommand,
                        colon_pos,
                        after_colon.to_string(),
                    ));
                }
            }
        }

        None
    }

    fn handle_streaming_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => {
                tracing::info!("Stream cancel requested via Esc");
                Action::Send(ChatAppMsg::StreamCancelled)
            }
            KeyCode::Char('c') if ctrl => {
                tracing::info!("Stream cancel requested via Ctrl+C");
                Action::Send(ChatAppMsg::StreamCancelled)
            }
            KeyCode::Char('t') if ctrl => {
                self.show_thinking = !self.show_thinking;
                let state = if self.show_thinking { "on" } else { "off" };
                self.notification_area
                    .add(crucible_core::types::Notification::toast(format!(
                        "Thinking display: {}",
                        state
                    )));
                Action::Continue
            }
            KeyCode::BackTab => {
                self.mode = self.mode.cycle();
                self.status = format!("Mode: {}", self.mode.as_str());
                Action::Continue
            }
            KeyCode::Enter if ctrl => {
                let content = self.input.content().to_string();
                if !content.trim().is_empty() {
                    self.input.handle(InputAction::Clear);
                    tracing::info!("Force-send during streaming");
                    Action::Send(ChatAppMsg::StreamCancelled)
                } else {
                    Action::Continue
                }
            }
            KeyCode::Enter => {
                let content = self.input.content().to_string();
                let trimmed = content.trim();
                if trimmed.starts_with(':') || trimmed.starts_with('/') {
                    self.input.handle(InputAction::Clear);
                    return self.handle_submit(content);
                }
                if !trimmed.is_empty() {
                    self.input.handle(InputAction::Clear);
                    Action::Send(ChatAppMsg::QueueMessage(content))
                } else {
                    Action::Continue
                }
            }
            _ => {
                let action = InputAction::from(key);
                self.input.handle(action);
                Action::Continue
            }
        }
    }

    fn handle_popup_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        match key.code {
            KeyCode::Esc => {
                self.close_popup();
            }
            KeyCode::Up => {
                self.popup_selected = self.popup_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.get_popup_items().len().saturating_sub(1);
                self.popup_selected = (self.popup_selected + 1).min(max);
            }
            KeyCode::Enter | KeyCode::Tab => {
                return self.select_popup_item();
            }
            KeyCode::Backspace => {
                if self.popup_filter.is_empty() {
                    self.close_popup();
                }
                self.input.handle(InputAction::Backspace);
                self.check_autocomplete_trigger();
            }
            KeyCode::Char(c) if self.is_ctrl_c(key) => {
                self.close_popup();
            }
            KeyCode::Char(c) => {
                self.input.handle(InputAction::Insert(c));
                self.check_autocomplete_trigger();
            }
            _ => {}
        }
        Action::Continue
    }

    fn select_popup_item(&mut self) -> Action<ChatAppMsg> {
        let items = self.get_popup_items();
        let Some(item) = items.get(self.popup_selected) else {
            return Action::Continue;
        };

        let label = item.label.clone();
        let kind = self.popup_kind.clone();
        self.insert_autocomplete_selection(&label);

        match kind {
            AutocompleteKind::SlashCommand => {
                self.input.handle(InputAction::Clear);
                self.handle_slash_command(&label)
            }
            AutocompleteKind::ReplCommand => {
                self.input.handle(InputAction::Clear);
                self.handle_repl_command(&label)
            }
            _ => Action::Continue,
        }
    }

    fn handle_submit(&mut self, content: String) -> Action<ChatAppMsg> {
        let content = content.trim().to_string();
        if content.is_empty() {
            return Action::Continue;
        }

        if content.starts_with('/') {
            return self.handle_slash_command(&content);
        }

        if content.starts_with(':') {
            return self.handle_repl_command(&content);
        }

        if content.starts_with('!') {
            return self.handle_shell_command(&content);
        }

        if let Some(ref callback) = self.on_submit {
            callback(content.clone());
        }

        self.submit_user_message(content);
        self.status = "Thinking...".to_string();

        Action::Continue
    }

    fn handle_slash_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
        let command = parts[0].to_lowercase();

        match command.as_str() {
            "quit" | "exit" | "q" => Action::Quit,
            "mode" => {
                self.set_mode_with_status(self.mode.cycle());
                Action::Continue
            }
            "default" | "normal" => {
                self.set_mode_with_status(ChatMode::Normal);
                Action::Continue
            }
            "plan" => {
                self.set_mode_with_status(ChatMode::Plan);
                Action::Continue
            }
            "auto" => {
                self.set_mode_with_status(ChatMode::Auto);
                Action::Continue
            }
            "help" => {
                self.add_system_message(
                    "Commands: /mode, /normal, /plan, /auto, /help, /quit".to_string(),
                );
                Action::Continue
            }
            _ => {
                self.error = Some(format!("Unknown command: /{}", command));
                Action::Continue
            }
        }
    }

    fn set_mode_with_status(&mut self, mode: ChatMode) {
        self.mode = mode;
        self.status = format!("Mode: {}", mode.as_str());
    }

    fn handle_repl_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let command = &cmd[1..];

        if command == "set" || command.starts_with("set ") {
            return self.handle_set_command(command);
        }

        if command == "config show" || command == "config" {
            return self.handle_config_show_command();
        }

        match command {
            "q" | "quit" => Action::Quit,
            "help" | "h" => {
                self.add_system_message(
                    "[core] :quit :help :clear :palette :model :set :export <path> :messages\n[mcp] :mcp"
                        .to_string(),
                );
                Action::Continue
            }
            "messages" | "msgs" | "notifications" => {
                self.notification_area.toggle();
                Action::Continue
            }
            "palette" | "commands" => {
                self.show_popup = true;
                self.popup_kind = AutocompleteKind::Command;
                self.popup_filter.clear();
                self.popup_selected = 0;
                Action::Continue
            }
            "mcp" => {
                self.handle_mcp_command();
                Action::Continue
            }
            "model" => {
                if self.model_list_state == ModelListState::NotLoaded {
                    self.show_popup = true;
                    self.popup_kind = AutocompleteKind::Model;
                    self.popup_filter.clear();
                    self.popup_selected = 0;
                    self.popup_trigger_pos = 0;
                    return Action::Send(ChatAppMsg::FetchModels);
                }
                if self.available_models.is_empty() {
                    self.add_system_message(
                        "No models available. Type :model <name> to switch.".to_string(),
                    );
                } else {
                    self.show_popup = true;
                    self.popup_kind = AutocompleteKind::Model;
                    self.popup_filter.clear();
                    self.popup_selected = 0;
                    self.popup_trigger_pos = 0;
                }
                Action::Continue
            }
            _ if command.starts_with("model ") => {
                let model_name = command.strip_prefix("model ").unwrap().trim();
                if model_name.is_empty() {
                    self.error = Some("Usage: :model <name>".to_string());
                    Action::Continue
                } else {
                    Action::Send(ChatAppMsg::SwitchModel(model_name.to_string()))
                }
            }
            "clear" => {
                self.container_list.clear();
                self.message_counter = 0;
                self.status = "Cleared".to_string();
                Action::Send(ChatAppMsg::ClearHistory)
            }
            _ if command.starts_with("export ") => {
                let path = command.strip_prefix("export ").unwrap().trim();
                self.handle_export_command(path);
                Action::Continue
            }
            _ => {
                self.error = Some(format!("Unknown REPL command: {}", cmd));
                Action::Continue
            }
        }
    }

    fn handle_export_command(&mut self, path: &str) {
        if path.is_empty() {
            self.error = Some("Usage: :export <path>".to_string());
            return;
        }

        let export_path = std::path::Path::new(path);
        let content = self.format_session_for_export();

        match std::fs::write(export_path, &content) {
            Ok(_) => {
                self.add_system_message(format!("Session exported to {}", path));
            }
            Err(e) => {
                self.error = Some(format!("Export failed: {}", e));
            }
        }
    }

    fn handle_set_command(&mut self, command: &str) -> Action<ChatAppMsg> {
        let input = command.strip_prefix("set").unwrap_or(command).trim();

        match SetCommand::parse(input) {
            Ok(cmd) => {
                match cmd {
                    SetCommand::ShowModified => {
                        let output = self.runtime_config.format_modified();
                        self.add_system_message(output);
                    }
                    SetCommand::ShowAll => {
                        let output = self.runtime_config.format_all();
                        self.add_system_message(output);
                    }
                    SetCommand::Query { key } => {
                        let output = self.runtime_config.format_query(&key);
                        self.add_system_message(output);
                    }
                    SetCommand::QueryHistory { key } => {
                        let output = self.runtime_config.format_history(&key);
                        self.add_system_message(output);
                    }
                    SetCommand::Enable { key } => {
                        if let Some(current) = self.runtime_config.get(&key) {
                            if current.as_bool().is_some() {
                                self.runtime_config.set(
                                    &key,
                                    ConfigValue::Bool(true),
                                    ModSource::Command,
                                );
                                self.sync_runtime_to_fields(&key);
                                self.add_system_message(format!("  {}=true", key));
                            } else {
                                let output = self.runtime_config.format_query(&key);
                                self.add_system_message(output);
                            }
                        } else {
                            self.runtime_config.set(
                                &key,
                                ConfigValue::Bool(true),
                                ModSource::Command,
                            );
                            self.sync_runtime_to_fields(&key);
                            self.add_system_message(format!("  {}=true", key));
                        }
                    }
                    SetCommand::Disable { key } => {
                        match self.runtime_config.disable(&key, ModSource::Command) {
                            Ok(()) => {
                                self.sync_runtime_to_fields(&key);
                                self.add_system_message(format!("  {}=false", key));
                            }
                            Err(e) => {
                                self.error = Some(e.to_string());
                            }
                        }
                    }
                    SetCommand::Toggle { key } => {
                        match self.runtime_config.toggle(&key, ModSource::Command) {
                            Ok(new_val) => {
                                self.sync_runtime_to_fields(&key);
                                self.add_system_message(format!("  {}={}", key, new_val));
                            }
                            Err(e) => {
                                self.error = Some(e.to_string());
                            }
                        }
                    }
                    SetCommand::Reset { key } => {
                        self.runtime_config.reset(&key);
                        self.sync_runtime_to_fields(&key);
                        let output = self.runtime_config.format_query(&key);
                        self.add_system_message(format!("Reset: {}", output.trim()));
                    }
                    SetCommand::Pop { key } => {
                        if self.runtime_config.pop(&key).is_some() {
                            self.sync_runtime_to_fields(&key);
                            let output = self.runtime_config.format_query(&key);
                            self.add_system_message(output);
                        } else {
                            self.add_system_message(format!("  {} is at base value", key));
                        }
                    }
                    SetCommand::Set { key, value } => {
                        if key == "model" {
                            self.model = value.clone();
                            self.runtime_config.set_dynamic(
                                &key,
                                ConfigValue::String(value.clone()),
                                ModSource::Command,
                                &self.current_provider.clone(),
                            );
                            self.add_system_message(format!("  model={}", value));
                            return Action::Send(ChatAppMsg::SwitchModel(value));
                        }

                        if key == "thinkingbudget" {
                            use crate::tui::oil::config::ThinkingPreset;
                            if let Some(preset) = ThinkingPreset::by_name(&value) {
                                let budget = preset.to_budget();
                                self.runtime_config
                                    .set_str(&key, &value, ModSource::Command);
                                self.add_system_message(format!(
                                    "  thinkingbudget={} ({})",
                                    value, budget
                                ));
                                return Action::Send(ChatAppMsg::SetThinkingBudget(budget));
                            } else {
                                let valid = ThinkingPreset::names().collect::<Vec<_>>().join(", ");
                                self.error =
                                    Some(format!("Unknown preset '{}'. Valid: {}", value, valid));
                                return Action::Continue;
                            }
                        }

                        if key == "temperature" {
                            match value.parse::<f64>() {
                                Ok(temp) if (0.0..=2.0).contains(&temp) => {
                                    self.runtime_config
                                        .set_str(&key, &value, ModSource::Command);
                                    self.add_system_message(format!("  temperature={}", temp));
                                    return Action::Send(ChatAppMsg::SetTemperature(temp));
                                }
                                Ok(_) => {
                                    self.error =
                                        Some("Temperature must be between 0.0 and 2.0".to_string());
                                    return Action::Continue;
                                }
                                Err(_) => {
                                    self.error =
                                        Some(format!("Invalid temperature value: {}", value));
                                    return Action::Continue;
                                }
                            }
                        }

                        if key == "maxtokens" {
                            let max_tokens = if value == "none" || value == "null" {
                                None
                            } else {
                                match value.parse::<u32>() {
                                    Ok(n) => Some(n),
                                    Err(_) => {
                                        self.error = Some(format!(
                                            "Invalid max_tokens value: {} (use a number or 'none')",
                                            value
                                        ));
                                        return Action::Continue;
                                    }
                                }
                            };
                            self.runtime_config
                                .set_str(&key, &value, ModSource::Command);
                            let display = max_tokens.map_or("none".to_string(), |n| n.to_string());
                            self.add_system_message(format!("  maxtokens={}", display));
                            return Action::Send(ChatAppMsg::SetMaxTokens(max_tokens));
                        }

                        if key.starts_with("perm.") {
                            return self.handle_perm_set(&key, &value);
                        }

                        self.runtime_config
                            .set_str(&key, &value, ModSource::Command);
                        self.sync_runtime_to_fields(&key);
                        self.add_system_message(format!("  {}={}", key, value));
                    }
                }
                Action::Continue
            }
            Err(e) => {
                self.error = Some(format!("Parse error: {}", e));
                Action::Continue
            }
        }
    }

    fn handle_config_show_command(&mut self) -> Action<ChatAppMsg> {
        let mut output = String::from("Configuration:\n");

        let temp = self
            .runtime_config
            .get("temperature")
            .unwrap_or(ConfigValue::String("0.7".to_string()));
        output.push_str(&format!("  temperature: {}\n", temp));

        let tokens = self
            .runtime_config
            .get("maxtokens")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  max_tokens: {}\n", tokens));

        let budget = self
            .runtime_config
            .get("thinkingbudget")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  thinking_budget: {}\n", budget));

        let mode = self
            .runtime_config
            .get("mode")
            .unwrap_or(ConfigValue::String("normal".to_string()));
        output.push_str(&format!("  mode: {}\n", mode));

        self.add_system_message(output);
        Action::Continue
    }

    fn handle_perm_set(&mut self, key: &str, value: &str) -> Action<ChatAppMsg> {
        let valid_keys = ["perm.show_diff", "perm.autoconfirm_session"];

        if !valid_keys.contains(&key) {
            self.error = Some(format!(
                "Unknown permission setting: {}. Valid: {}",
                key,
                valid_keys.join(", ")
            ));
            return Action::Continue;
        }

        let bool_value = match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                self.error = Some(format!(
                    "Invalid value for {}: '{}'. Use true/false",
                    key, value
                ));
                return Action::Continue;
            }
        };

        self.runtime_config
            .set(key, ConfigValue::Bool(bool_value), ModSource::Command);
        self.sync_runtime_to_fields(key);

        self.notification_area
            .add(crucible_core::types::Notification::toast(format!(
                "Permission setting updated: {}={}",
                key, bool_value
            )));

        Action::Continue
    }

    fn sync_runtime_to_fields(&mut self, key: &str) {
        match key {
            "thinking" => {
                if let Some(val) = self.runtime_config.get("thinking") {
                    self.show_thinking = val.as_bool().unwrap_or(true);
                }
            }
            "model" => {
                if let Some(ConfigValue::String(m)) = self
                    .runtime_config
                    .get_dynamic("model", &self.current_provider.clone())
                {
                    self.model = m;
                }
            }
            "perm.show_diff" => {
                if let Some(val) = self.runtime_config.get("perm.show_diff") {
                    self.perm_show_diff = val.as_bool().unwrap_or(true);
                }
            }
            "perm.autoconfirm_session" => {
                if let Some(val) = self.runtime_config.get("perm.autoconfirm_session") {
                    self.perm_autoconfirm_session = val.as_bool().unwrap_or(false);
                }
            }
            _ => {}
        }
    }

    fn format_session_for_export(&self) -> String {
        self.container_list.format_for_export()
    }

    fn handle_mcp_command(&mut self) {
        if self.mcp_servers.is_empty() {
            self.add_system_message("No MCP servers configured".to_string());
            return;
        }

        let mut lines = vec![format!("MCP Servers ({}):", self.mcp_servers.len())];
        for server in &self.mcp_servers {
            let status = if server.connected { "●" } else { "○" };
            lines.push(format!(
                "  {} {} ({}_) - {} tools",
                status, server.name, server.prefix, server.tool_count
            ));
        }
        self.add_system_message(lines.join("\n"));
    }

    fn handle_shell_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let shell_cmd = cmd[1..].trim().to_string();
        if shell_cmd.is_empty() {
            self.error = Some("Empty shell command".to_string());
            return Action::Continue;
        }

        if !self
            .shell_history
            .back()
            .is_some_and(|last| last == &shell_cmd)
        {
            self.push_shell_history(shell_cmd.clone());
        }
        self.shell_history_index = None;

        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        match ShellModal::spawn(shell_cmd.clone(), working_dir) {
            Ok(modal) => {
                self.enter_alternate_screen();
                self.shell_modal = Some(modal);
            }
            Err(e) => {
                self.error = Some(e);
            }
        }

        Action::Continue
    }

    fn handle_shell_modal_tick(&mut self) {
        let visible_lines = self.modal_visible_lines();

        if let Some(ref mut modal) = self.shell_modal {
            let output = modal.update(ShellModalMsg::Tick, visible_lines);
            self.handle_shell_modal_output(output);
        }
    }

    fn handle_shell_modal_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        let visible_lines = self.modal_visible_lines();

        if let Some(ref mut modal) = self.shell_modal {
            let output = modal.update(ShellModalMsg::Key(key), visible_lines);
            self.handle_shell_modal_output(output);
        }

        Action::Continue
    }

    fn handle_shell_modal_output(&mut self, output: ShellModalOutput) {
        match output {
            ShellModalOutput::None => {}
            ShellModalOutput::Close(history_item) => {
                self.save_shell_output();

                self.message_counter += 1;
                self.container_list
                    .add_shell_execution(CachedShellExecution::new(
                        format!("shell-{}", self.message_counter),
                        &history_item.command,
                        history_item.exit_code,
                        history_item.output_tail,
                        history_item.output_path,
                    ));

                self.shell_modal = None;
                self.leave_alternate_screen();
            }
            ShellModalOutput::InsertOutput { content, truncated } => {
                let label = if truncated { " (truncated)" } else { "" };
                self.input.insert_str(&format!(
                    "Here is the output of a shell command I ran{}:\n\n```\n{}\n```\n",
                    label, content
                ));
            }
        }
    }

    fn handle_interaction_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        let modal = match &mut self.interaction_modal {
            Some(m) => m,
            None => return Action::Continue,
        };

        match modal.update(InteractionModalMsg::Key(key)) {
            InteractionModalOutput::None => Action::Continue,
            InteractionModalOutput::Close => {
                self.close_interaction();
                Action::Continue
            }
            InteractionModalOutput::PermissionResponse {
                request_id,
                response,
            } => {
                self.close_interaction_and_show_next();
                Action::Send(ChatAppMsg::CloseInteraction {
                    request_id,
                    response: InteractionResponse::Permission(response),
                })
            }
            InteractionModalOutput::AskResponse {
                request_id,
                response,
            } => {
                self.close_interaction();
                Action::Send(ChatAppMsg::CloseInteraction {
                    request_id,
                    response,
                })
            }
            InteractionModalOutput::ToggleDiff => Action::Continue,
            InteractionModalOutput::Notify(msg) => {
                self.notify_toast(msg);
                Action::Continue
            }
        }
    }

    fn notify_toast(&mut self, msg: impl Into<String>) {
        self.notification_area
            .add(crucible_core::types::Notification::toast(msg));
    }

    fn close_interaction_and_show_next(&mut self) {
        self.interaction_modal = None;
        if let Some((next_id, next_perm)) = self.permission_queue.pop_front() {
            self.interaction_modal = Some(InteractionModal::new(
                next_id,
                InteractionRequest::Permission(next_perm),
            ));
        }
    }

    fn modal_visible_lines(&self) -> usize {
        let term_height = crossterm::terminal::size()
            .map(|(_, h)| h as usize)
            .unwrap_or(24);
        term_height.saturating_sub(2)
    }

    fn tick_shell_modal(&mut self) {
        let visible_lines = self.modal_visible_lines();
        if let Some(ref mut modal) = self.shell_modal {
            let output = modal.update(ShellModalMsg::Tick, visible_lines);
            self.handle_shell_modal_output(output);
        }
    }

    fn cancel_shell(&mut self) {
        if let Some(ref mut modal) = self.shell_modal {
            modal.cancel();
        }
    }

    fn close_shell_modal_with_history(&mut self, history_item: ShellHistoryItem) {
        self.message_counter += 1;
        self.container_list
            .add_shell_execution(CachedShellExecution::new(
                format!("shell-{}", self.message_counter),
                &history_item.command,
                history_item.exit_code,
                history_item.output_tail,
                history_item.output_path,
            ));
        self.shell_modal = None;
        self.leave_alternate_screen();
    }

    fn enter_alternate_screen(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, EnterAlternateScreen, cursor::Hide);
        let _ = stdout.flush();
    }

    fn leave_alternate_screen(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, cursor::Show);
        let _ = stdout.flush();
        self.needs_full_redraw = true;
    }

    fn save_shell_output(&mut self) -> Option<PathBuf> {
        let session_dir = self.session_dir.clone()?;
        let modal = self.shell_modal.as_mut()?;
        modal.save_output(&session_dir)
    }

    fn maybe_spill_tool_output(&mut self, name: &str) {
        if !self.container_list.tool_should_spill(name) {
            return;
        }

        let Some(session_dir) = self.session_dir.clone() else {
            return;
        };

        let tool_dir = session_dir.join("tools");
        if let Err(e) = std::fs::create_dir_all(&tool_dir) {
            tracing::error!(path = %tool_dir.display(), error = %e, "Failed to create tool output directory");
            return;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let name_slug: String = name
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.txt", timestamp, name_slug);
        let path = tool_dir.join(&filename);

        if let Some(output) = self.container_list.get_tool_output(name) {
            if let Err(e) = std::fs::write(&path, &output) {
                tracing::error!(path = %path.display(), error = %e, "Failed to write tool output");
                return;
            }
            self.container_list.set_tool_output_path(name, path);
        }
    }

    fn send_shell_output(&mut self, truncated: bool) {
        let path = self.save_shell_output();

        if let Some(ref modal) = self.shell_modal {
            let path_str = path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|n| format!("shell/{}", n))
                .unwrap_or_else(|| "(not saved)".to_string());

            let exit_str = match modal.status() {
                ShellStatus::Completed { exit_code } => format!("exit {}", exit_code),
                ShellStatus::Cancelled => "cancelled".to_string(),
                ShellStatus::Running => "running".to_string(),
            };

            let mut message = format!(
                "[Shell: {}]\n$ {} ({})\n\n",
                path_str,
                modal.command(),
                exit_str
            );

            let output_lines = modal.output_lines();
            if truncated {
                let total = output_lines.len();
                let show_lines = 50.min(total);
                if total > show_lines {
                    message.push_str(&format!(
                        "[Truncated: showing last {} of {} lines]\n\n",
                        show_lines, total
                    ));
                }
                for line in output_lines.iter().rev().take(show_lines).rev() {
                    message.push_str(line);
                    message.push('\n');
                }
            } else {
                for line in output_lines {
                    message.push_str(line);
                    message.push('\n');
                }
            }

            self.add_system_message(message);
        }
    }

    fn open_shell_output_in_editor(&mut self) {
        let path = match self.save_shell_output() {
            Some(p) => p,
            None => {
                self.error = Some("Failed to save output file".to_string());
                return;
            }
        };

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        crossterm::terminal::disable_raw_mode().ok();
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen).ok();

        let status = Command::new(&editor).arg(&path).status();

        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen).ok();
        crossterm::terminal::enable_raw_mode().ok();

        if let Err(e) = status {
            self.error = Some(format!("Failed to open editor: {}", e));
        }
    }

    fn render_shell_modal(&self) -> Node {
        let (term_width, term_height) = crossterm::terminal::size()
            .map(|(w, h)| (w as usize, h as usize))
            .unwrap_or((80, 24));

        self.shell_modal
            .as_ref()
            .map(|m| m.view(term_width, term_height))
            .unwrap_or(Node::Empty)
    }

    fn add_user_message(&mut self, content: String) {
        self.message_counter += 1;
        self.container_list.add_user_message(content);
    }

    /// Add user message and mark the turn as active so the turn-level
    /// spinner renders while waiting for the first token.
    /// Use this (not `add_user_message`) when sending to the daemon.
    fn submit_user_message(&mut self, content: String) {
        self.add_user_message(content);
        self.container_list.mark_turn_active();
    }

    fn add_system_message(&mut self, content: String) {
        self.message_counter += 1;
        self.container_list.add_system_message(content);
    }

    fn finalize_streaming(&mut self) {
        if self.container_list.is_streaming() {
            self.message_counter += 1;
            self.container_list.complete_response();
        }

        self.status = "Ready".to_string();
    }

    fn process_deferred_queue(&mut self) -> Action<ChatAppMsg> {
        if let Some(queued) = self.deferred_messages.pop_front() {
            self.submit_user_message(queued.clone());
            self.status = "Thinking...".to_string();
            Action::Send(ChatAppMsg::UserMessage(queued))
        } else {
            Action::Continue
        }
    }

    /// Render chat content using the container-based architecture.
    ///
    /// This renders all viewport containers (non-graduated) in order.
    /// Each container is wrapped in scrollback with its stable ID.
    fn render_containers(&self) -> Node {
        let term_width = terminal_width();
        let containers = self.container_list.viewport_containers();

        if containers.is_empty() {
            return Node::Empty;
        }

        let mut nodes: Vec<Node> = containers
            .iter()
            .enumerate()
            .map(|(i, c)| {
                use crate::tui::oil::chat_container::ViewParams;
                let abs_idx = self.container_list.viewport_start_index() + i;
                let params = ViewParams {
                    width: term_width,
                    spinner_frame: self.spinner_frame,
                    show_thinking: self.show_thinking,
                    is_continuation: self.container_list.is_continuation(abs_idx),
                    is_complete: self.container_list.is_response_complete(abs_idx),
                };
                c.view_with_params(&params)
            })
            .collect();

        // Turn-level spinner: shown when the turn is active but no container
        // is currently displaying a spinner (e.g. after tools complete, before
        // next TextDelta or StreamComplete).
        if self.container_list.needs_turn_spinner() {
            nodes.push(
                row([text(" "), spinner(None, self.spinner_frame)]).with_margin(Padding {
                    top: 1,
                    ..Default::default()
                }),
            );
        }

        // When graduated content exists above in stdout, insert a spacer line
        // so the viewport starts with a blank line (visual separation from
        // the graduated user prompt / previous content).
        if self.container_list.viewport_start_index() > 0 {
            nodes.insert(0, text(" "));
        }

        col(nodes)
    }

    fn render_error(&self) -> Node {
        maybe(self.error.clone(), |err| {
            styled(
                format!("Error: {}", err),
                ThemeTokens::default_ref().error_style(),
            )
        })
    }

    fn render_status(&self) -> Node {
        use crate::tui::oil::component::Component;

        let mut status_bar = StatusBar::new()
            .mode(self.mode)
            .model(&self.model)
            .context(self.context_used, self.context_total)
            .status(&self.status);

        if let Some((text, kind)) = self.notification_area.active_toast() {
            status_bar = status_bar.toast(text, kind);
        }
        let counts = self.notification_area.warning_counts();
        if !counts.is_empty() {
            status_bar = status_bar.counts(counts);
        }

        let focus = crate::tui::oil::focus::FocusContext::default();
        let ctx = ViewContext::new(&focus);
        status_bar.view(&ctx)
    }

    fn detect_input_mode(&self) -> InputMode {
        let content = self.input.content();
        if content.starts_with(':') {
            InputMode::Command
        } else if content.starts_with('!') {
            InputMode::Shell
        } else {
            InputMode::Normal
        }
    }

    fn render_input(&self, ctx: &ViewContext<'_>) -> Node {
        let width = terminal_width();
        let is_focused = !self.show_popup || ctx.is_focused(FOCUS_INPUT);
        let input_mode = self.detect_input_mode();

        let prompt = input_mode.prompt();
        let bg = input_mode.bg_color();

        let top_edge = styled("▄".repeat(width), Style::new().fg(bg));
        let bottom_edge = styled("▀".repeat(width), Style::new().fg(bg));

        let content = self.input.content();
        let display_content = match input_mode {
            InputMode::Command => content.strip_prefix(':').unwrap_or(content),
            InputMode::Shell => content.strip_prefix('!').unwrap_or(content),
            InputMode::Normal => content,
        };

        let cursor_offset = if matches!(input_mode, InputMode::Command | InputMode::Shell) {
            1
        } else {
            0
        };
        let display_cursor = self.input.cursor().saturating_sub(cursor_offset);

        let content_width = width.saturating_sub(prompt.len() + 1);
        let all_lines = wrap_content(display_content, content_width);

        let (cursor_line, cursor_col) = if content_width > 0 && !all_lines.is_empty() {
            let line_idx = display_cursor / content_width;
            let col_in_line = display_cursor % content_width;
            (line_idx.min(all_lines.len() - 1), col_in_line)
        } else {
            (0, display_cursor)
        };

        let (visible_lines, visible_cursor_line) =
            Self::clamp_input_lines(&all_lines, cursor_line, INPUT_MAX_CONTENT_LINES);

        let mut rows: Vec<Node> = Vec::with_capacity(INPUT_MAX_CONTENT_LINES + 2);
        rows.push(top_edge);

        for (i, line) in visible_lines.iter().enumerate() {
            let line_len = line.chars().count();
            let line_padding = " ".repeat(content_width.saturating_sub(line_len) + 1);
            let is_first_visible = i == 0 && visible_lines.len() == all_lines.len();
            let line_prefix = if is_first_visible { prompt } else { "   " };

            if i == visible_cursor_line && is_focused {
                rows.push(row([
                    styled(line_prefix, Style::new().bg(bg)),
                    Node::Input(crate::tui::oil::node::InputNode {
                        value: line.to_string(),
                        cursor: cursor_col.min(line_len),
                        placeholder: None,
                        style: Style::new().bg(bg),
                        focused: true,
                    }),
                    styled(line_padding, Style::new().bg(bg)),
                ]));
            } else {
                rows.push(styled(
                    format!("{}{}{}", line_prefix, line, line_padding),
                    Style::new().bg(bg),
                ));
            }
        }

        rows.push(bottom_edge);

        let input_node = col(rows);

        focusable_auto(FOCUS_INPUT, input_node)
    }

    fn clamp_input_lines(
        lines: &[String],
        cursor_line: usize,
        max_lines: usize,
    ) -> (Vec<String>, usize) {
        if lines.len() <= max_lines {
            return (lines.to_vec(), cursor_line);
        }

        let half = max_lines / 2;
        let start = if cursor_line <= half {
            0
        } else if cursor_line >= lines.len() - half {
            lines.len() - max_lines
        } else {
            cursor_line - half
        };

        let end = (start + max_lines).min(lines.len());
        let visible = lines[start..end].to_vec();
        let adjusted_cursor = cursor_line - start;

        (visible, adjusted_cursor)
    }

    fn get_popup_items(&self) -> Vec<PopupItemNode> {
        let filter = self.popup_filter.to_lowercase();

        match self.popup_kind {
            AutocompleteKind::File => {
                Self::filter_to_popup_items(&self.workspace_files, &filter, "file", 15)
            }
            AutocompleteKind::Note => {
                Self::filter_to_popup_items(&self.kiln_notes, &filter, "note", 15)
            }
            AutocompleteKind::Command => Self::filter_commands(
                &[
                    ("semantic_search", "Search notes by meaning", "tool"),
                    ("create_note", "Create a new note", "tool"),
                    ("/mode", "Cycle chat mode", "command"),
                    ("/help", "Show help", "command"),
                ],
                &filter,
            ),
            AutocompleteKind::SlashCommand => Self::filter_commands(
                &[
                    ("/mode", "Cycle chat mode", "command"),
                    ("/default", "Set default mode (ask permissions)", "command"),
                    ("/plan", "Set plan mode (read-only)", "command"),
                    ("/auto", "Set auto mode (full access)", "command"),
                    ("/help", "Show help", "command"),
                    ("/quit", "Exit chat", "command"),
                ],
                &filter,
            ),
            AutocompleteKind::ReplCommand => Self::filter_commands(
                &[
                    (":quit", "Exit chat", "core"),
                    (":help", "Show help", "core"),
                    (":clear", "Clear conversation history", "core"),
                    (":palette", "Open command palette", "core"),
                    (":model", "Switch model", "core"),
                    (":mcp", "List MCP servers", "mcp"),
                    (":export", "Export session to file", "core"),
                    (":set", "View/modify runtime options", "core"),
                ],
                &filter,
            ),
            AutocompleteKind::Model => {
                Self::filter_to_popup_items(&self.available_models, &filter, "model", 15)
            }
            AutocompleteKind::CommandArg {
                ref command,
                arg_index,
            } => self.get_command_arg_completions(command, arg_index, &filter),
            AutocompleteKind::SetOption { ref option } => {
                self.get_set_option_completions(option.as_deref(), &filter)
            }
            AutocompleteKind::None => vec![],
        }
    }

    fn filter_to_popup_items(
        items: &[String],
        filter: &str,
        kind: &str,
        limit: usize,
    ) -> Vec<PopupItemNode> {
        items
            .iter()
            .filter(|s| filter.is_empty() || s.to_lowercase().contains(filter))
            .take(limit)
            .map(|s| PopupItemNode {
                label: s.clone(),
                description: None,
                kind: Some(kind.to_string()),
            })
            .collect()
    }

    fn filter_commands(commands: &[(&str, &str, &str)], filter: &str) -> Vec<PopupItemNode> {
        commands
            .iter()
            .filter(|(label, _, _)| filter.is_empty() || label.to_lowercase().contains(filter))
            .map(|(label, desc, kind)| PopupItemNode {
                label: label.to_string(),
                description: Some(desc.to_string()),
                kind: Some(kind.to_string()),
            })
            .collect()
    }

    fn get_set_option_completions(&self, option: Option<&str>, filter: &str) -> Vec<PopupItemNode> {
        use crate::tui::oil::config::{CompletionSource, SHORTCUTS, THINKING_PRESETS};

        match option {
            None => SHORTCUTS
                .iter()
                .filter(|s| filter.is_empty() || s.short.to_lowercase().contains(filter))
                .map(|s| {
                    let current_value = self.runtime_config.get(s.short);
                    let value_str = current_value.map(|v| format!("={}", v)).unwrap_or_default();
                    PopupItemNode {
                        label: s.short.to_string(),
                        description: Some(format!("{}{}", s.description, value_str)),
                        kind: Some("option".to_string()),
                    }
                })
                .collect(),
            Some(opt) => {
                let source = self.runtime_config.completions_for(opt);
                match source {
                    CompletionSource::Models => {
                        Self::filter_to_popup_items(&self.available_models, filter, "model", 15)
                    }
                    CompletionSource::ThinkingPresets => THINKING_PRESETS
                        .iter()
                        .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(filter))
                        .map(|p| PopupItemNode {
                            label: p.name.to_string(),
                            description: p.tokens.map(|t| format!("~{} tokens", t)),
                            kind: Some("preset".to_string()),
                        })
                        .collect(),
                    CompletionSource::Themes => Self::filter_commands(
                        &[
                            ("base16-ocean.dark", "", "theme"),
                            ("Solarized (dark)", "", "theme"),
                            ("Solarized (light)", "", "theme"),
                            ("InspiredGitHub", "", "theme"),
                        ],
                        filter,
                    )
                    .into_iter()
                    .map(|mut p| {
                        p.description = None;
                        p
                    })
                    .collect(),
                    CompletionSource::Static(values) => values
                        .iter()
                        .filter(|v| filter.is_empty() || v.to_lowercase().contains(filter))
                        .map(|v| PopupItemNode {
                            label: v.to_string(),
                            description: None,
                            kind: Some("value".to_string()),
                        })
                        .collect(),
                    CompletionSource::None => vec![],
                }
            }
        }
    }

    fn get_command_arg_completions(
        &self,
        command: &str,
        _arg_index: usize,
        filter: &str,
    ) -> Vec<PopupItemNode> {
        match command {
            "export" => self.complete_file_paths(filter),
            "mcp" => self.complete_mcp_servers(filter),
            _ => self.complete_file_paths(filter),
        }
    }

    fn complete_file_paths(&self, filter: &str) -> Vec<PopupItemNode> {
        Self::filter_to_popup_items(&self.workspace_files, filter, "path", 15)
    }

    fn complete_mcp_servers(&self, filter: &str) -> Vec<PopupItemNode> {
        self.mcp_servers
            .iter()
            .filter(|s| filter.is_empty() || s.name.to_lowercase().contains(filter))
            .map(|s| PopupItemNode {
                label: s.name.clone(),
                description: Some(format!("{} tools", s.tool_count)),
                kind: Some("mcp".to_string()),
            })
            .collect()
    }

    fn render_popup_overlay(&self) -> Node {
        let show = self.show_popup && self.popup_kind != AutocompleteKind::None;
        let items = if show { self.get_popup_items() } else { vec![] };

        if show && !items.is_empty() {
            let input_height = self.calculate_input_height();
            let status_height = 1;
            let offset_from_bottom = input_height + status_height;

            let popup_node =
                focusable(FOCUS_POPUP, popup(items, self.popup_selected, POPUP_HEIGHT));
            overlay_from_bottom(popup_node, offset_from_bottom)
        } else {
            Node::Empty
        }
    }

    fn calculate_input_height(&self) -> usize {
        let width = terminal_width();
        let content = self.input.content();
        let display_content = if content.starts_with(':') || content.starts_with('!') {
            &content[1..]
        } else {
            content
        };
        let content_width = width.saturating_sub(4);
        let lines = wrap_content(display_content, content_width);
        let visible_lines = lines.len().min(INPUT_MAX_CONTENT_LINES);
        visible_lines + 2
    }

    fn insert_autocomplete_selection(&mut self, label: &str) {
        match &self.popup_kind {
            AutocompleteKind::File => {
                self.replace_at_trigger(format!("@{} ", label));
                self.add_context_if_new(format!("@{}", label));
            }
            AutocompleteKind::Note => {
                self.replace_at_trigger(format!("[[{}]] ", label));
                self.add_context_if_new(format!("[[{}]]", label));
            }
            AutocompleteKind::Command => {
                self.status = format!("Selected: {}", label);
            }
            AutocompleteKind::SlashCommand | AutocompleteKind::ReplCommand => {
                self.set_input(label);
            }
            AutocompleteKind::Model => {
                self.set_input(&format!(":model {}", label));
            }
            AutocompleteKind::CommandArg { .. } => {
                self.replace_at_trigger(format!("{} ", label));
            }
            AutocompleteKind::SetOption { option } => {
                let cmd = match option {
                    None => format!(":set {}", label),
                    Some(opt) => format!(":set {}={}", opt, label),
                };
                self.set_input(&cmd);
            }
            AutocompleteKind::None => {}
        }

        self.close_popup();
    }

    fn replace_at_trigger(&mut self, replacement: String) {
        let content = self.input.content().to_string();
        let trigger_pos = self.popup_trigger_pos;
        let prefix = &content[..trigger_pos];
        let suffix = &content[self.input.cursor()..];
        let new_content = format!("{}{}{}", prefix, replacement, suffix);
        let new_cursor = prefix.len() + replacement.len();

        self.set_input_and_cursor(&new_content, new_cursor);
    }

    fn set_input(&mut self, content: &str) {
        self.input.handle(InputAction::Clear);
        for ch in content.chars() {
            self.input.handle(InputAction::Insert(ch));
        }
    }

    fn set_input_and_cursor(&mut self, content: &str, cursor: usize) {
        self.set_input(content);
        while self.input.cursor() > cursor {
            self.input.handle(InputAction::Left);
        }
    }

    fn add_context_if_new(&mut self, item: String) {
        if !self.attached_context.contains(&item) {
            self.attached_context.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::chat_container::ChatContainer;
    use crate::tui::oil::focus::FocusContext;
    use crate::tui::oil::render::render_to_string;

    #[test]
    fn test_mode_cycle() {
        assert_eq!(ChatMode::Normal.cycle(), ChatMode::Plan);
        assert_eq!(ChatMode::Plan.cycle(), ChatMode::Auto);
        assert_eq!(ChatMode::Auto.cycle(), ChatMode::Normal);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(ChatMode::parse("normal"), ChatMode::Normal);
        assert_eq!(ChatMode::parse("default"), ChatMode::Normal);
        assert_eq!(ChatMode::parse("plan"), ChatMode::Plan);
        assert_eq!(ChatMode::parse("auto"), ChatMode::Auto);
        assert_eq!(ChatMode::parse("unknown"), ChatMode::Normal);
    }

    #[test]
    fn test_app_init() {
        let app = OilChatApp::init();
        assert!(app.container_list().is_empty());
        assert!(!app.is_streaming());
        assert_eq!(app.mode, ChatMode::Normal);
    }

    #[test]
    fn test_user_message() {
        let mut app = OilChatApp::init();
        app.add_user_message("Hello".to_string());

        assert_eq!(app.container_list().len(), 1);
        if let ChatContainer::UserMessage { content, .. } =
            &app.container_list().all_containers()[0]
        {
            assert_eq!(content, "Hello");
        } else {
            panic!("Expected UserMessage");
        }
    }

    #[test]
    fn test_streaming_flow() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::TextDelta("Hello ".to_string()));
        assert!(app.is_streaming());

        app.on_message(ChatAppMsg::TextDelta("World".to_string()));
        assert!(app.is_streaming());

        app.on_message(ChatAppMsg::StreamComplete);
        assert!(!app.is_streaming());

        // Verify content via container list
        let containers = app.container_list().all_containers();
        assert_eq!(containers.len(), 1);
        if let ChatContainer::AssistantResponse { blocks, .. } = &containers[0] {
            let combined = blocks.join("");
            assert_eq!(combined, "Hello World");
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn test_tool_call_flow() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::ToolCall {
            name: "Read".to_string(),
            args: r#"{"path":"file.md","offset":10}"#.to_string(),
            call_id: None,
        });
        let tool = app.container_list().find_tool("Read").unwrap();
        assert_eq!(tool.name.as_ref(), "Read");
        assert!(!tool.complete);

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "Read".to_string(),
            delta: "line 1\n".to_string(),
            call_id: None,
        });
        let tool = app.container_list().find_tool("Read").unwrap();
        assert_eq!(tool.result(), "line 1");

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "Read".to_string(),
            delta: "line 2\n".to_string(),
            call_id: None,
        });
        let tool = app.container_list().find_tool("Read").unwrap();
        assert_eq!(tool.result(), "line 1\nline 2");

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "Read".to_string(),
            call_id: None,
        });
        let tool = app.container_list().find_tool("Read").unwrap();
        assert!(tool.complete);
    }

    #[test]
    fn test_slash_commands() {
        let mut app = OilChatApp::init();

        assert_eq!(app.mode, ChatMode::Normal);
        app.handle_slash_command("/mode");
        assert_eq!(app.mode, ChatMode::Plan);

        app.handle_slash_command("/normal");
        assert_eq!(app.mode, ChatMode::Normal);
    }

    #[test]
    fn test_clear_repl_command() {
        let mut app = OilChatApp::init();

        app.add_user_message("test".to_string());
        assert_eq!(app.container_list().len(), 1);

        let action = app.handle_repl_command(":clear");
        assert!(app.container_list().is_empty());
        assert!(matches!(action, Action::Send(ChatAppMsg::ClearHistory)));
    }

    #[test]
    fn test_messages_command_toggles_notification_area() {
        let mut app = OilChatApp::init();
        assert!(!app.notification_area.is_visible());

        app.handle_repl_command(":messages");
        assert!(app.notification_area.is_visible());

        app.handle_repl_command(":messages");
        assert!(!app.notification_area.is_visible());

        app.handle_repl_command(":msgs");
        assert!(app.notification_area.is_visible());
    }

    #[test]
    fn test_toggle_messages_msg() {
        let mut app = OilChatApp::init();
        assert!(!app.notification_area.is_visible());

        app.on_message(ChatAppMsg::ToggleMessages);
        assert!(app.notification_area.is_visible());

        app.on_message(ChatAppMsg::ToggleMessages);
        assert!(!app.notification_area.is_visible());
    }

    #[test]
    fn test_quit_command() {
        let mut app = OilChatApp::init();
        let action = app.handle_slash_command("/quit");
        assert!(action.is_quit());
    }

    #[test]
    fn test_view_renders() {
        use crate::tui::oil::focus::FocusContext;

        let mut app = OilChatApp::init();
        app.add_user_message("Hello".to_string());
        app.on_message(ChatAppMsg::TextDelta("Hi there".to_string()));

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let _node = app.view(&ctx);
    }

    #[test]
    fn test_tool_call_renders_with_result() {
        use crate::tui::oil::focus::FocusContext;
        use crate::tui::oil::render::render_to_string;

        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"README.md","offset":1,"limit":200}"#.to_string(),
            call_id: None,
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let node = app.view(&ctx);
        let output = render_to_string(&node, 80);

        assert!(output.contains("read_file"), "should show tool name");
        assert!(output.contains("path="), "should show args");

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "# README\nThis is the content.".to_string(),
            call_id: None,
        });

        let node = app.view(&ctx);
        let output = render_to_string(&node, 80);
        assert!(
            output.contains("README") || output.contains("content"),
            "should show streaming output while running"
        );

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
        });

        let node = app.view(&ctx);
        let output = render_to_string(&node, 80);
        assert!(output.contains("✓"), "should show checkmark when complete");
        assert!(
            output.contains("2 lines"),
            "should show line count for read_file when complete"
        );
    }

    #[test]
    fn test_context_usage_updates() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::ContextUsage {
            used: 64000,
            total: 128000,
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(output.contains("50%"), "Should show 50% context usage");
    }

    #[test]
    fn test_context_display_unknown_total() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::ContextUsage {
            used: 5000,
            total: 0,
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("5k tok"),
            "Should show token count when total is unknown: {}",
            output
        );
        assert!(
            !output.contains("%"),
            "Should not show percentage when total is unknown"
        );
    }

    #[test]
    fn test_context_display_no_usage() {
        let app = OilChatApp::init();

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            !output.contains("ctx") && !output.contains("tok"),
            "Should not show context info when no usage: {}",
            output
        );
    }

    #[test]
    fn test_status_shows_mode_indicator() {
        let mut app = OilChatApp::init();
        app.set_mode(ChatMode::Plan);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(output.contains("PLAN"), "Status should show PLAN mode");
    }

    #[test]
    fn test_error_message_clears_streaming() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::TextDelta("partial response".to_string()));
        assert!(app.is_streaming());

        app.on_message(ChatAppMsg::Error("Connection lost".to_string()));
        assert!(!app.is_streaming(), "Error should stop streaming");
    }

    #[test]
    fn test_ctrl_t_toggles_thinking_during_streaming() {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
        assert!(app.is_streaming());

        let initial_show_thinking = app.show_thinking;

        let ctrl_t = crossterm::event::KeyEvent::new(
            KeyCode::Char('t'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        let action = app.handle_key(ctrl_t);

        assert!(
            matches!(action, Action::Continue),
            "Ctrl+T should return Continue, not cancel stream"
        );
        assert!(app.is_streaming(), "Stream should still be active");
        assert_ne!(
            app.show_thinking, initial_show_thinking,
            "Ctrl+T should toggle show_thinking"
        );
        assert!(
            !app.notification_area.is_empty(),
            "Notification should be added to store"
        );
    }

    #[test]
    fn test_shell_history_ring_buffer_evicts_oldest() {
        let mut app = OilChatApp::init();

        for i in 0..(MAX_SHELL_HISTORY + 5) {
            app.push_shell_history(format!("cmd {}", i));
        }

        assert_eq!(app.shell_history.len(), MAX_SHELL_HISTORY);
        assert_eq!(app.shell_history.front().unwrap(), "cmd 5");
        assert_eq!(
            app.shell_history.back().unwrap(),
            &format!("cmd {}", MAX_SHELL_HISTORY + 4)
        );
    }

    #[test]
    fn test_interaction_modal_open_close_cycle() {
        use crucible_core::interaction::AskRequest;

        let mut app = OilChatApp::init();
        assert!(!app.interaction_visible());

        let request = InteractionRequest::Ask(AskRequest::new("Choose an option"));
        app.open_interaction("req-123".to_string(), request);

        assert!(app.interaction_visible());
        let modal = app.interaction_modal.as_ref().unwrap();
        assert_eq!(modal.request_id, "req-123");
        assert_eq!(modal.selected, 0);
        assert!(modal.filter.is_empty());
        assert!(modal.other_text.is_empty());
        assert_eq!(modal.mode, InteractionMode::Selecting);

        app.close_interaction();
        assert!(!app.interaction_visible());
        assert!(app.interaction_modal.is_none());
    }

    #[test]
    fn test_interaction_modal_replaces_previous() {
        use crucible_core::interaction::AskRequest;

        let mut app = OilChatApp::init();

        let request1 = InteractionRequest::Ask(AskRequest::new("First question"));
        app.open_interaction("req-1".to_string(), request1);
        assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "req-1");

        let request2 = InteractionRequest::Ask(AskRequest::new("Second question"));
        app.open_interaction("req-2".to_string(), request2);
        assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "req-2");
        assert!(app.interaction_visible());
    }

    #[test]
    fn test_interaction_modal_close_when_none_is_noop() {
        let mut app = OilChatApp::init();
        assert!(!app.interaction_visible());

        app.close_interaction();
        assert!(!app.interaction_visible());
    }

    #[test]
    fn test_perm_request_bash_renders() {
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request =
            InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
        app.open_interaction("perm-1".to_string(), request);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("PERMISSION"),
            "Should show PERMISSION badge"
        );
        assert!(output.contains("BASH"), "Should show BASH type label");
        assert!(
            output.contains("npm install lodash"),
            "Should show command tokens"
        );
        assert!(output.contains("y"), "Should show allow key");
        assert!(output.contains("n"), "Should show deny key");
        assert!(output.contains("Esc"), "Should show cancel key");
    }

    #[test]
    fn test_perm_request_write_renders() {
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::write([
            "home", "user", "project", "src", "main.rs",
        ]));
        app.open_interaction("perm-2".to_string(), request);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(output.contains("WRITE"), "Should show WRITE type label");
        assert!(
            output.contains("/home/user/project/src/main.rs"),
            "Should show path segments"
        );
    }

    #[test]
    fn test_perm_request_y_allows() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::bash(["ls", "-la"]));
        app.open_interaction("perm-3".to_string(), request);

        let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(!app.interaction_visible(), "Modal should close after y");
        match action {
            Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
                InteractionResponse::Permission(perm) => {
                    assert!(perm.allowed, "Should be allowed");
                }
                _ => panic!("Expected Permission response"),
            },
            _ => panic!("Expected CloseInteraction action"),
        }
    }

    #[test]
    fn test_perm_request_n_denies() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
        app.open_interaction("perm-4".to_string(), request);

        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(!app.interaction_visible(), "Modal should close after n");
        match action {
            Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
                InteractionResponse::Permission(perm) => {
                    assert!(!perm.allowed, "Should be denied");
                }
                _ => panic!("Expected Permission response"),
            },
            _ => panic!("Expected CloseInteraction action"),
        }
    }

    #[test]
    fn test_perm_request_escape_denies() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::read(["etc", "passwd"]));
        app.open_interaction("perm-5".to_string(), request);

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(
            !app.interaction_visible(),
            "Modal should close after Escape"
        );
        match action {
            Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
                InteractionResponse::Permission(perm) => {
                    assert!(!perm.allowed, "Escape should deny permission");
                }
                _ => panic!("Expected Permission response"),
            },
            _ => panic!("Expected CloseInteraction action"),
        }
    }

    #[test]
    fn test_perm_request_h_toggles_diff_collapsed() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request =
            InteractionRequest::Permission(PermRequest::write(["home", "user", "file.txt"]));
        app.open_interaction("perm-6".to_string(), request);

        assert!(app.interaction_visible(), "Modal should be visible");

        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(
            app.interaction_visible(),
            "Modal should remain visible after h"
        );
        assert!(
            matches!(action, Action::Continue),
            "h should return Continue, not close modal"
        );
    }

    #[test]
    fn test_perm_request_p_saves_pattern_and_allows() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
        app.open_interaction("perm-7".to_string(), request);

        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(
            !app.interaction_visible(),
            "Modal should close after p (pattern saved)"
        );
        match action {
            Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
                InteractionResponse::Permission(perm) => {
                    assert!(perm.allowed, "p should allow");
                    assert!(perm.pattern.is_some(), "p should set a pattern");
                    assert_eq!(
                        perm.pattern.as_deref(),
                        Some("npm install"),
                        "pattern should match command"
                    );
                }
                _ => panic!("Expected Permission response"),
            },
            _ => panic!("Expected Send(CloseInteraction) action"),
        }
    }

    #[test]
    fn test_perm_request_other_keys_ignored() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();
        let request = InteractionRequest::Permission(PermRequest::bash(["ls", "-la"]));
        app.open_interaction("perm-8".to_string(), request);

        for c in ['a', 'b', 'x', 'z', '1', '!'] {
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            let action = app.handle_key(key);

            assert!(
                app.interaction_visible(),
                "Modal should remain visible after '{}'",
                c
            );
            assert!(
                matches!(action, Action::Continue),
                "'{}' should be ignored and return Continue",
                c
            );
        }
    }

    #[test]
    fn test_perm_queue_second_request_queued_when_first_pending() {
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();

        let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        app.open_interaction("perm-1".to_string(), request1);
        assert!(app.interaction_visible());
        assert_eq!(app.permission_queue.len(), 0);

        let request2 = InteractionRequest::Permission(PermRequest::bash(["cat", "file.txt"]));
        app.open_interaction("perm-2".to_string(), request2);

        assert!(app.interaction_visible());
        assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-1");
        assert_eq!(app.permission_queue.len(), 1);
    }

    #[test]
    fn test_perm_queue_shows_next_after_response() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();

        let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        app.open_interaction("perm-1".to_string(), request1);

        let request2 = InteractionRequest::Permission(PermRequest::bash(["cat"]));
        app.open_interaction("perm-2".to_string(), request2);

        let request3 = InteractionRequest::Permission(PermRequest::bash(["rm"]));
        app.open_interaction("perm-3".to_string(), request3);

        assert_eq!(app.permission_queue.len(), 2);

        let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        app.handle_key(key);

        assert!(app.interaction_visible());
        assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-2");
        assert_eq!(app.permission_queue.len(), 1);

        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key);

        assert!(app.interaction_visible());
        assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-3");
        assert_eq!(app.permission_queue.len(), 0);

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key(key);

        assert!(!app.interaction_visible());
        assert_eq!(app.permission_queue.len(), 0);
    }

    #[test]
    fn test_perm_queue_indicator_shows_in_header() {
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();

        let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        app.open_interaction("perm-1".to_string(), request1);

        let request2 = InteractionRequest::Permission(PermRequest::bash(["cat"]));
        app.open_interaction("perm-2".to_string(), request2);

        let request3 = InteractionRequest::Permission(PermRequest::bash(["rm"]));
        app.open_interaction("perm-3".to_string(), request3);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("[1/3]"),
            "Should show queue indicator [1/3], got: {}",
            output
        );
    }

    #[test]
    fn test_perm_queue_no_indicator_for_single_request() {
        use crucible_core::interaction::PermRequest;

        let mut app = OilChatApp::init();

        let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        app.open_interaction("perm-1".to_string(), request);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            !output.contains("[1/1]"),
            "Should not show queue indicator for single request"
        );
        assert!(output.contains("BASH"), "Should show BASH type label");
    }

    #[test]
    fn messages_drawer_closes_on_escape() {
        let mut app = OilChatApp::init();
        app.notification_area
            .add(crucible_core::types::Notification::toast("test"));
        app.notification_area.show();
        assert!(app.notification_area.is_visible());

        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        )));
        assert!(!app.notification_area.is_visible());
    }

    #[test]
    fn messages_drawer_closes_on_q() {
        let mut app = OilChatApp::init();
        app.notification_area
            .add(crucible_core::types::Notification::toast("test"));
        app.notification_area.show();
        assert!(app.notification_area.is_visible());

        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        )));
        assert!(!app.notification_area.is_visible());
    }

    #[test]
    fn add_notification_does_not_open_drawer() {
        let mut app = OilChatApp::init();
        app.add_notification(crucible_core::types::Notification::toast("test"));
        assert!(
            !app.notification_area.is_visible(),
            "Adding a notification should not open the drawer"
        );
    }

    #[test]
    fn notify_toast_does_not_open_drawer() {
        let mut app = OilChatApp::init();
        app.notify_toast("test toast");
        assert!(
            !app.notification_area.is_visible(),
            "notify_toast should not open the drawer"
        );
    }

    #[test]
    fn drawer_any_key_dismisses_without_fallthrough() {
        let mut app = OilChatApp::init();
        app.notification_area
            .add(crucible_core::types::Notification::toast("test"));
        app.notification_area.show();
        assert!(app.notification_area.is_visible());

        // Press 'a' — should dismiss drawer but NOT insert 'a' into input
        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        )));
        assert!(!app.notification_area.is_visible());
        assert!(
            !app.input.content().contains('a'),
            "Key should not fall through to input after dismissing drawer"
        );
    }

    #[test]
    fn messages_drawer_closes_on_permission() {
        let mut app = OilChatApp::init();
        app.notification_area
            .add(crucible_core::types::Notification::toast("test"));
        app.notification_area.show();
        assert!(app.notification_area.is_visible());

        app.open_interaction(
            "req-1".to_string(),
            InteractionRequest::Permission(PermRequest::bash(["ls", "-la"])),
        );
        assert!(!app.notification_area.is_visible());
        assert!(app.interaction_visible());
    }

    #[test]
    fn messages_command_works_during_streaming() {
        let mut app = OilChatApp::init();
        app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
        assert!(app.is_streaming());

        app.notification_area
            .add(crucible_core::types::Notification::toast("test"));

        // Type :messages and submit
        for c in ":messages".chars() {
            app.update(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            )));
        }
        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )));

        assert!(
            app.notification_area.is_visible(),
            ":messages should open drawer even during streaming"
        );
        assert!(app.is_streaming(), "Stream should still be active");
    }

    #[test]
    fn mode_cycling_works_during_streaming() {
        let mut app = OilChatApp::init();
        app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
        assert!(app.is_streaming());
        assert_eq!(app.mode, ChatMode::Normal);

        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::BackTab,
            crossterm::event::KeyModifiers::NONE,
        )));

        assert_ne!(
            app.mode,
            ChatMode::Normal,
            "BackTab should cycle mode during streaming"
        );
        assert!(app.is_streaming(), "Stream should still be active");
    }
}
