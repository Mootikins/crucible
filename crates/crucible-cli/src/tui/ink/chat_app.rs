use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::event::{Event, InputAction, InputBuffer};
use crate::tui::ink::markdown::markdown_to_node_with_width;
use crate::tui::ink::node::*;
use crate::tui::ink::style::{Color, Gap, Style};
use crossterm::event::KeyCode;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

const INPUT_BG: Color = Color::Rgb(40, 44, 52);
const BULLET_PREFIX: &str = " ● ";
const BULLET_PREFIX_WIDTH: usize = BULLET_PREFIX.len();
const FOCUS_INPUT: &str = "input";
const FOCUS_POPUP: &str = "popup";

#[derive(Debug, Clone)]
pub enum ChatAppMsg {
    UserMessage(String),
    TextDelta(String),
    ToolCall { name: String, args: String },
    ToolResultDelta { name: String, delta: String },
    ToolResultComplete { name: String },
    StreamComplete,
    Error(String),
    Status(String),
    ModeChanged(String),
    ContextUsage { used: usize, total: usize },
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
}

impl ChatItem {
    fn id(&self) -> &str {
        match self {
            ChatItem::Message { id, .. } => id,
            ChatItem::ToolCall { id, .. } => id,
        }
    }
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
    Plan,
    Act,
    Auto,
}

impl ChatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
            ChatMode::Auto => "auto",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "act" => ChatMode::Act,
            "auto" => ChatMode::Auto,
            _ => ChatMode::Plan,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::Auto,
            ChatMode::Auto => ChatMode::Plan,
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
        match self {
            InputMode::Normal => INPUT_BG,
            InputMode::Command => Color::Rgb(60, 50, 20),
            InputMode::Shell => Color::Rgb(60, 30, 30),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutocompleteKind {
    #[default]
    None,
    File,
    Note,
    Command,
    SlashCommand,
    ReplCommand,
}

#[derive(Debug, Clone, Default)]
struct StreamingState {
    content: String,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellStatus {
    Running,
    Completed { exit_code: i32 },
    Cancelled,
}

pub struct ShellModal {
    command: String,
    output_lines: Vec<String>,
    status: ShellStatus,
    scroll_offset: usize,
    start_time: Instant,
    duration: Option<Duration>,
    output_path: Option<PathBuf>,
    working_dir: PathBuf,
    output_receiver: Option<Receiver<String>>,
    child_pid: Option<u32>,
}

impl ShellModal {
    fn new(command: String, working_dir: PathBuf) -> Self {
        Self {
            command,
            output_lines: Vec::new(),
            status: ShellStatus::Running,
            scroll_offset: 0,
            start_time: Instant::now(),
            duration: None,
            output_path: None,
            working_dir,
            output_receiver: None,
            child_pid: None,
        }
    }

    fn visible_lines(&self, max_lines: usize) -> &[String] {
        let total = self.output_lines.len();
        if total <= max_lines {
            &self.output_lines
        } else {
            let start = self.scroll_offset.min(total.saturating_sub(max_lines));
            let end = (start + max_lines).min(total);
            &self.output_lines[start..end]
        }
    }

    fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    fn scroll_down(&mut self, lines: usize, max_visible: usize) {
        let max_offset = self.output_lines.len().saturating_sub(max_visible);
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self, max_visible: usize) {
        self.scroll_offset = self.output_lines.len().saturating_sub(max_visible);
    }

    fn is_running(&self) -> bool {
        self.status == ShellStatus::Running
    }

    fn format_header(&self) -> String {
        let status_str = match &self.status {
            ShellStatus::Running => "● running".to_string(),
            ShellStatus::Completed { exit_code } if *exit_code == 0 => {
                format!("✓ exit 0 {:.1?}", self.duration.unwrap_or_default())
            }
            ShellStatus::Completed { exit_code } => {
                format!(
                    "✗ exit {} {:.1?}",
                    exit_code,
                    self.duration.unwrap_or_default()
                )
            }
            ShellStatus::Cancelled => "⏹ cancelled".to_string(),
        };
        format!("$ {}  {}", self.command, status_str)
    }

    fn format_footer(&self) -> String {
        let line_info = format!("({} lines)", self.output_lines.len());
        if self.is_running() {
            format!("Ctrl+C: cancel  {}", line_info)
        } else {
            format!(
                "s: send │ t: truncated │ e: edit │ Enter/Esc: dismiss  {}",
                line_info
            )
        }
    }
}

pub struct InkChatApp {
    items: Vec<ChatItem>,
    input: InputBuffer,
    streaming: StreamingState,
    spinner_frame: usize,
    mode: ChatMode,
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
    notification: Option<(String, std::time::Instant)>,
    shell_modal: Option<ShellModal>,
    shell_history: Vec<String>,
    shell_history_index: Option<usize>,
    session_dir: Option<PathBuf>,
}

impl Default for InkChatApp {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            input: InputBuffer::new(),
            streaming: StreamingState::default(),
            spinner_frame: 0,
            mode: ChatMode::Plan,
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
            context_total: 128000,
            last_ctrl_c: None,
            notification: None,
            shell_modal: None,
            shell_history: Vec::new(),
            shell_history_index: None,
            session_dir: None,
        }
    }
}

const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(2);

impl App for InkChatApp {
    type Msg = ChatAppMsg;

    fn init() -> Self {
        Self::default()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        if self.shell_modal.is_some() {
            return self.render_shell_modal();
        }

        col([
            self.render_items(),
            self.render_streaming(),
            self.render_error(),
            spacer(),
            self.render_popup(),
            self.render_input(ctx),
            self.render_status(),
        ])
        .gap(Gap::row(0))
    }

    fn update(&mut self, event: Event) -> Action<ChatAppMsg> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Tick => {
                self.spinner_frame = (self.spinner_frame + 1) % 4;
                Action::Continue
            }
            Event::Resize { .. } => Action::Continue,
            Event::Quit => Action::Quit,
        }
    }

    fn on_message(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::UserMessage(content) => {
                self.add_user_message(content);
                Action::Continue
            }
            ChatAppMsg::TextDelta(delta) => {
                self.streaming.content.push_str(&delta);
                self.streaming.active = true;
                Action::Continue
            }
            ChatAppMsg::ToolCall { name, args } => {
                self.message_counter += 1;
                tracing::debug!(
                    tool_name = %name,
                    args_len = args.len(),
                    counter = self.message_counter,
                    "Adding ToolCall to items"
                );
                self.items.push(ChatItem::ToolCall {
                    id: format!("tool-{}", self.message_counter),
                    name,
                    args,
                    result: String::new(),
                    complete: false,
                });
                Action::Continue
            }
            ChatAppMsg::ToolResultDelta { name, delta } => {
                tracing::debug!(
                    tool_name = %name,
                    delta_len = delta.len(),
                    items_count = self.items.len(),
                    "Received ToolResultDelta"
                );
                let found =
                    self.items.iter_mut().rev().find(
                        |item| matches!(item, ChatItem::ToolCall { name: n, .. } if n == &name),
                    );
                if let Some(ChatItem::ToolCall {
                    result,
                    name: found_name,
                    ..
                }) = found
                {
                    tracing::debug!(found_name = %found_name, "Found matching tool call");
                    result.push_str(&delta);
                } else {
                    tracing::warn!(
                        tool_name = %name,
                        existing_tools = ?self.items.iter().filter_map(|i| {
                            match i {
                                ChatItem::ToolCall { name, .. } => Some(name.as_str()),
                                _ => None,
                            }
                        }).collect::<Vec<_>>(),
                        "No matching tool call found for result"
                    );
                }
                Action::Continue
            }
            ChatAppMsg::ToolResultComplete { name } => {
                tracing::debug!(tool_name = %name, "Received ToolResultComplete");
                let found =
                    self.items.iter_mut().rev().find(
                        |item| matches!(item, ChatItem::ToolCall { name: n, .. } if n == &name),
                    );
                if let Some(ChatItem::ToolCall { complete, .. }) = found {
                    *complete = true;
                    tracing::debug!(tool_name = %name, "Marked tool complete");
                } else {
                    tracing::warn!(tool_name = %name, "No matching tool call found for completion");
                }
                Action::Continue
            }
            ChatAppMsg::StreamComplete => {
                self.finalize_streaming();
                Action::Continue
            }
            ChatAppMsg::Error(msg) => {
                self.error = Some(msg);
                self.streaming.active = false;
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
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(100))
    }
}

impl InkChatApp {
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

    pub fn is_streaming(&self) -> bool {
        self.streaming.active
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
    pub fn has_shell_modal(&self) -> bool {
        self.shell_modal.is_some()
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        self.error = None;

        if self.shell_modal.is_some() {
            return self.handle_shell_modal_key(key);
        }

        if key.code == KeyCode::F(1) {
            if self.show_popup {
                self.show_popup = false;
                self.popup_kind = AutocompleteKind::None;
                self.popup_filter.clear();
            } else {
                self.show_popup = true;
                self.popup_kind = AutocompleteKind::Command;
                self.popup_filter.clear();
            }
            self.popup_selected = 0;
            return Action::Continue;
        }

        if self.show_popup {
            return self.handle_popup_key(key);
        }

        if key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
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
            self.notification = Some(("Ctrl+C again to quit".to_string(), now));
            return Action::Continue;
        } else {
            self.last_ctrl_c = None;
        }

        let action = InputAction::from(key);
        if let Some(submitted) = self.input.handle(action) {
            return self.handle_submit(submitted);
        }

        self.check_autocomplete_trigger();

        Action::Continue
    }

    fn check_autocomplete_trigger(&mut self) {
        let content = self.input.content();
        let cursor = self.input.cursor();

        if let Some((kind, trigger_pos, filter)) = self.detect_trigger(content, cursor) {
            self.popup_kind = kind;
            self.popup_trigger_pos = trigger_pos;
            self.popup_filter = filter;
            self.popup_selected = 0;
            self.show_popup = !self.get_popup_items().is_empty();
        } else if self.popup_kind != AutocompleteKind::None {
            self.popup_kind = AutocompleteKind::None;
            self.popup_filter.clear();
            self.show_popup = false;
        }
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
                let filter = &before_cursor[colon_pos + 1..];
                if !filter.contains(char::is_whitespace) {
                    return Some((AutocompleteKind::ReplCommand, colon_pos, filter.to_string()));
                }
            }
        }

        None
    }

    fn handle_popup_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        match key.code {
            KeyCode::Esc => {
                self.show_popup = false;
                self.popup_kind = AutocompleteKind::None;
                self.popup_filter.clear();
            }
            KeyCode::Up => {
                self.popup_selected = self.popup_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.get_popup_items().len().saturating_sub(1);
                self.popup_selected = (self.popup_selected + 1).min(max);
            }
            KeyCode::Enter | KeyCode::Tab => {
                let items = self.get_popup_items();
                if let Some(item) = items.get(self.popup_selected) {
                    let label = item.label.clone();
                    let kind = self.popup_kind;
                    self.insert_autocomplete_selection(&label);
                    if kind == AutocompleteKind::SlashCommand {
                        self.input.handle(InputAction::Clear);
                        return self.handle_slash_command(&label);
                    }
                    if kind == AutocompleteKind::ReplCommand {
                        self.input.handle(InputAction::Clear);
                        return self.handle_repl_command(&label);
                    }
                }
            }
            KeyCode::Backspace => {
                if self.popup_filter.is_empty() {
                    self.show_popup = false;
                    self.popup_kind = AutocompleteKind::None;
                }
                self.input.handle(InputAction::Backspace);
                self.check_autocomplete_trigger();
            }
            KeyCode::Char(c) => {
                // Ctrl+C closes popup instead of inserting 'c'
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && c == 'c'
                {
                    self.show_popup = false;
                    self.popup_kind = AutocompleteKind::None;
                    self.popup_filter.clear();
                    return Action::Continue;
                }
                self.input.handle(InputAction::Insert(c));
                self.check_autocomplete_trigger();
            }
            _ => {}
        }
        Action::Continue
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

        self.add_user_message(content);

        self.streaming = StreamingState {
            content: String::new(),
            active: true,
        };
        self.status = "Thinking...".to_string();

        Action::Continue
    }

    fn handle_slash_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
        let command = parts[0].to_lowercase();
        let _args = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match command.as_str() {
            "quit" | "exit" | "q" => Action::Quit,
            "mode" => {
                self.mode = self.mode.cycle();
                self.status = format!("Mode: {}", self.mode.as_str());
                Action::Continue
            }
            "plan" => {
                self.mode = ChatMode::Plan;
                self.status = "Mode: plan".to_string();
                Action::Continue
            }
            "act" => {
                self.mode = ChatMode::Act;
                self.status = "Mode: act".to_string();
                Action::Continue
            }
            "auto" => {
                self.mode = ChatMode::Auto;
                self.status = "Mode: auto".to_string();
                Action::Continue
            }
            "clear" => {
                self.items.clear();
                self.message_counter = 0;
                self.status = "Cleared".to_string();
                Action::Continue
            }
            "help" => {
                self.add_system_message(
                    "Commands: /mode, /plan, /act, /auto, /clear, /help, /quit".to_string(),
                );
                Action::Continue
            }
            _ => {
                self.error = Some(format!("Unknown command: /{}", command));
                Action::Continue
            }
        }
    }

    fn handle_repl_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let command = &cmd[1..];
        match command {
            "q" | "quit" => Action::Quit,
            "help" | "h" => {
                self.add_system_message(
                    "REPL commands: :q(uit), :h(elp), :palette, :commands".to_string(),
                );
                Action::Continue
            }
            "palette" | "commands" => {
                self.show_popup = true;
                self.popup_kind = AutocompleteKind::Command;
                self.popup_filter.clear();
                self.popup_selected = 0;
                Action::Continue
            }
            _ => {
                self.error = Some(format!("Unknown REPL command: {}", cmd));
                Action::Continue
            }
        }
    }

    fn handle_shell_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let shell_cmd = cmd[1..].trim().to_string();
        if shell_cmd.is_empty() {
            self.error = Some("Empty shell command".to_string());
            return Action::Continue;
        }

        if !self
            .shell_history
            .last()
            .is_some_and(|last| last == &shell_cmd)
        {
            self.shell_history.push(shell_cmd.clone());
        }
        self.shell_history_index = None;

        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut modal = ShellModal::new(shell_cmd.clone(), working_dir.clone());

        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        modal.output_receiver = Some(rx);

        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

        match Command::new(shell)
            .arg(shell_arg)
            .arg(&shell_cmd)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                modal.child_pid = Some(child.id());

                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                std::thread::spawn(move || {
                    Self::stream_output(stdout, stderr, tx, child);
                });

                self.shell_modal = Some(modal);
            }
            Err(e) => {
                self.error = Some(format!("Failed to execute command: {}", e));
            }
        }

        Action::Continue
    }

    fn stream_output(
        stdout: Option<std::process::ChildStdout>,
        stderr: Option<std::process::ChildStderr>,
        tx: Sender<String>,
        mut child: Child,
    ) {
        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        let stdout_handle = stdout.map(|out| {
            std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stdout.send(line).is_err() {
                        break;
                    }
                }
            })
        });

        let stderr_handle = stderr.map(|err| {
            std::thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stderr.send(format!("\x1b[31m{}\x1b[0m", line)).is_err() {
                        break;
                    }
                }
            })
        });

        if let Some(h) = stdout_handle {
            let _ = h.join();
        }
        if let Some(h) = stderr_handle {
            let _ = h.join();
        }

        let exit_code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        let _ = tx.send(format!("\x00EXIT:{}", exit_code));
    }

    fn poll_shell_output(&mut self) {
        if let Some(ref mut modal) = self.shell_modal {
            if let Some(ref rx) = modal.output_receiver {
                while let Ok(line) = rx.try_recv() {
                    if let Some(code_str) = line.strip_prefix("\x00EXIT:") {
                        if let Ok(code) = code_str.parse::<i32>() {
                            modal.status = ShellStatus::Completed { exit_code: code };
                            modal.duration = Some(modal.start_time.elapsed());
                        }
                    } else {
                        modal.output_lines.push(line);
                    }
                }
            }
        }
    }

    fn handle_shell_modal_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        self.poll_shell_output();

        let is_running = self
            .shell_modal
            .as_ref()
            .map(|m| m.is_running())
            .unwrap_or(false);

        let visible_lines = self.modal_visible_lines();
        let half_page = visible_lines / 2;

        match key.code {
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                if is_running {
                    self.cancel_shell();
                }
            }
            KeyCode::Esc | KeyCode::Enter if !is_running => {
                self.shell_modal = None;
            }
            KeyCode::Char('s') if !is_running => {
                self.send_shell_output(false);
                self.shell_modal = None;
            }
            KeyCode::Char('t') if !is_running => {
                self.send_shell_output(true);
                self.shell_modal = None;
            }
            KeyCode::Char('e') if !is_running => {
                self.open_shell_output_in_editor();
            }
            KeyCode::Up | KeyCode::Char('k') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_up(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_down(1, visible_lines);
                }
            }
            KeyCode::Char('u') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_up(half_page);
                }
            }
            KeyCode::Char('d') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_down(half_page, visible_lines);
                }
            }
            KeyCode::Char('g') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_to_top();
                }
            }
            KeyCode::Char('G') if !is_running => {
                if let Some(ref mut modal) = self.shell_modal {
                    modal.scroll_to_bottom(visible_lines);
                }
            }
            _ => {}
        }
        Action::Continue
    }

    fn modal_visible_lines(&self) -> usize {
        let term_height = crossterm::terminal::size()
            .map(|(_, h)| h as usize)
            .unwrap_or(24);
        (term_height * 80 / 100).saturating_sub(4)
    }

    fn cancel_shell(&mut self) {
        if let Some(ref mut modal) = self.shell_modal {
            if let Some(pid) = modal.child_pid {
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-TERM", &pid.to_string()])
                        .output();
                }
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &pid.to_string(), "/F"])
                        .output();
                }
            }
            modal.status = ShellStatus::Cancelled;
            modal.duration = Some(modal.start_time.elapsed());
        }
    }

    fn save_shell_output(&mut self) -> Option<PathBuf> {
        let modal = self.shell_modal.as_ref()?;
        let session_dir = self.session_dir.clone()?;

        let shell_dir = session_dir.join("shell");
        std::fs::create_dir_all(&shell_dir).ok()?;

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let cmd_slug: String = modal
            .command
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.output", timestamp, cmd_slug);
        let path = shell_dir.join(&filename);

        let mut content = String::new();
        content.push_str(&format!("$ {}\n", modal.command));
        content.push_str(&format!(
            "Exit: {}\n",
            match &modal.status {
                ShellStatus::Completed { exit_code } => exit_code.to_string(),
                ShellStatus::Cancelled => "cancelled".to_string(),
                ShellStatus::Running => "running".to_string(),
            }
        ));
        if let Some(duration) = modal.duration {
            content.push_str(&format!("Duration: {:.2?}\n", duration));
        }
        content.push_str(&format!("Cwd: {}\n", modal.working_dir.display()));
        content.push_str("---\n");
        for line in &modal.output_lines {
            content.push_str(line);
            content.push('\n');
        }

        std::fs::write(&path, content).ok()?;

        if let Some(ref mut m) = self.shell_modal {
            m.output_path = Some(path.clone());
        }

        Some(path)
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

            let exit_str = match &modal.status {
                ShellStatus::Completed { exit_code } => format!("exit {}", exit_code),
                ShellStatus::Cancelled => "cancelled".to_string(),
                ShellStatus::Running => "running".to_string(),
            };

            let mut message = format!(
                "[Shell: {}]\n$ {} ({})\n\n",
                path_str, modal.command, exit_str
            );

            if truncated {
                let total = modal.output_lines.len();
                let show_lines = 50.min(total);
                if total > show_lines {
                    message.push_str(&format!(
                        "[Truncated: showing last {} of {} lines]\n\n",
                        show_lines, total
                    ));
                }
                for line in modal.output_lines.iter().rev().take(show_lines).rev() {
                    message.push_str(line);
                    message.push('\n');
                }
            } else {
                for line in &modal.output_lines {
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
        let modal = match &self.shell_modal {
            Some(m) => m,
            None => return Node::Empty,
        };

        let term_height = crossterm::terminal::size()
            .map(|(_, h)| h as usize)
            .unwrap_or(24);
        let modal_height = (term_height * 80 / 100).max(10);
        let content_height = modal_height.saturating_sub(4);

        let header_bg = Color::Rgb(50, 55, 65);
        let body_bg = Color::Rgb(30, 34, 42);
        let footer_bg = Color::Rgb(40, 44, 52);

        let header = styled(
            format!(" {} ", modal.format_header()),
            Style::new().bg(header_bg).bold(),
        );

        let visible = modal.visible_lines(content_height);
        let body_lines: Vec<Node> = visible.iter().map(|line| text(line.clone())).collect();

        let body = col(body_lines).with_style(Style::new().bg(body_bg));

        let footer = styled(
            format!(" {} ", modal.format_footer()),
            Style::new().bg(footer_bg).dim(),
        );

        col([header, body, spacer(), footer])
    }

    fn format_tool_args(args: &str) -> String {
        if args.is_empty() || args == "{}" {
            return String::new();
        }

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
            if let Some(obj) = parsed.as_object() {
                let pairs: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        let val = match v {
                            serde_json::Value::String(s) => {
                                let collapsed = s.replace('\n', "↵").replace('\r', "");
                                if collapsed.len() > 30 {
                                    format!("\"{}…\"", &collapsed[..27])
                                } else {
                                    format!("\"{}\"", collapsed)
                                }
                            }
                            other => {
                                let s = other.to_string();
                                if s.len() > 30 {
                                    format!("{}…", &s[..27])
                                } else {
                                    s
                                }
                            }
                        };
                        format!("{}={}", k, val)
                    })
                    .collect();
                return pairs.join(", ");
            }
        }

        let oneline = args.replace('\n', " ").replace("  ", " ");
        if oneline.len() <= 60 {
            oneline
        } else {
            format!("{}…", &oneline[..57])
        }
    }

    fn format_tool_result(name: &str, result: &str) -> Node {
        match name {
            "read_file" => {
                let summary = if let Some(bracket_start) = result.rfind('[') {
                    result[bracket_start..].trim_end_matches(']').to_string()
                } else {
                    format!("{} lines", result.lines().count())
                };
                styled(format!("   {}", summary), Style::new().fg(Color::DarkGray))
            }
            _ => {
                let all_lines: Vec<&str> = result.lines().collect();
                let lines: Vec<&str> = all_lines.iter().rev().take(3).rev().copied().collect();
                let truncated = all_lines.len() > 3;

                col(std::iter::once(if truncated {
                    styled("   …", Style::new().fg(Color::DarkGray))
                } else {
                    Node::Empty
                })
                .chain(lines.iter().map(|line| {
                    let truncated_line = if line.len() > 77 {
                        format!("   {}…", &line[..74])
                    } else {
                        format!("   {}", line)
                    };
                    styled(truncated_line, Style::new().fg(Color::DarkGray))
                })))
            }
        }
    }

    fn format_streaming_output(output: &str) -> Node {
        let all_lines: Vec<&str> = output.lines().collect();
        let lines: Vec<&str> = all_lines.iter().rev().take(3).rev().copied().collect();
        let truncated = all_lines.len() > 3;

        col(std::iter::once(if truncated {
            styled("     …", Style::new().fg(Color::DarkGray))
        } else {
            Node::Empty
        })
        .chain(lines.iter().map(|line| {
            let truncated_line = if line.len() > 72 {
                format!("     {}…", &line[..69])
            } else {
                format!("     {}", line)
            };
            styled(truncated_line, Style::new().fg(Color::DarkGray))
        })))
    }

    fn add_user_message(&mut self, content: String) {
        self.message_counter += 1;
        self.items.push(ChatItem::Message {
            id: format!("user-{}", self.message_counter),
            role: Role::User,
            content,
        });
    }

    fn add_system_message(&mut self, content: String) {
        self.message_counter += 1;
        self.items.push(ChatItem::Message {
            id: format!("system-{}", self.message_counter),
            role: Role::System,
            content,
        });
    }

    fn finalize_streaming(&mut self) {
        if !self.streaming.content.is_empty() {
            self.message_counter += 1;
            self.items.push(ChatItem::Message {
                id: format!("assistant-{}", self.message_counter),
                role: Role::Assistant,
                content: std::mem::take(&mut self.streaming.content),
            });
        }
        self.streaming.active = false;
        self.status = "Ready".to_string();
    }

    fn render_items(&self) -> Node {
        fragment(self.items.iter().map(|item| self.render_item(item)))
    }

    fn render_item(&self, item: &ChatItem) -> Node {
        match item {
            ChatItem::Message { id, role, content } => {
                let content_node = match role {
                    Role::User => self.render_user_prompt(content),
                    Role::Assistant => {
                        let content_width = terminal_width().saturating_sub(BULLET_PREFIX_WIDTH);
                        let md_node = markdown_to_node_with_width(content, content_width);
                        col([
                            text(""),
                            row([
                                styled(BULLET_PREFIX, Style::new().fg(Color::DarkGray)),
                                md_node,
                            ]),
                        ])
                    }
                    Role::System => col([
                        text(""),
                        styled(
                            format!(" * {} ", content),
                            Style::new().fg(Color::Yellow).dim(),
                        ),
                    ]),
                };
                scrollback(id, [content_node])
            }
            ChatItem::ToolCall {
                id,
                name,
                args,
                result,
                complete,
            } => {
                let (status_icon, status_color) = if *complete {
                    ("✓", Color::Green)
                } else {
                    ("…", Color::White)
                };

                let args_formatted = Self::format_tool_args(args);

                let header = row([
                    styled(format!(" {} ", status_icon), Style::new().fg(status_color)),
                    styled(name, Style::new().fg(Color::White)),
                    styled(
                        format!("({})", args_formatted),
                        Style::new().fg(Color::DarkGray),
                    ),
                ]);

                let result_node = if result.is_empty() {
                    Node::Empty
                } else if *complete {
                    Self::format_tool_result(name, result)
                } else {
                    Self::format_streaming_output(result)
                };

                let content = col([header, result_node]);
                if *complete {
                    scrollback(id, [content])
                } else {
                    col([text(""), content])
                }
            }
        }
    }

    fn render_streaming(&self) -> Node {
        when(self.streaming.active, {
            let content_width = terminal_width().saturating_sub(BULLET_PREFIX_WIDTH);
            let content_node = markdown_to_node_with_width(&self.streaming.content, content_width);

            if_else(
                !self.streaming.content.is_empty(),
                col([
                    text(""),
                    row([
                        styled(BULLET_PREFIX, Style::new().fg(Color::DarkGray)),
                        content_node,
                    ]),
                    spinner(None, self.spinner_frame),
                ]),
                spinner(Some("Thinking...".into()), self.spinner_frame),
            )
        })
    }

    fn render_error(&self) -> Node {
        maybe(self.error.clone(), |err| {
            styled(format!("Error: {}", err), Style::new().fg(Color::Red))
        })
    }

    fn render_status(&self) -> Node {
        let mode_style = match self.mode {
            ChatMode::Plan => Style::new().fg(Color::Blue),
            ChatMode::Act => Style::new().fg(Color::Green),
            ChatMode::Auto => Style::new().fg(Color::Yellow),
        };

        let separator = " │ ";

        let context_percent = if self.context_total > 0 {
            (self.context_used as f64 / self.context_total as f64 * 100.0).round() as usize
        } else {
            0
        };

        let mode_str = match self.mode {
            ChatMode::Plan => " Plan",
            ChatMode::Act => " Act",
            ChatMode::Auto => " Auto",
        };
        let ctx_str = format!("{}% ctx ", context_percent);

        let attached_count = self.attached_context.len();
        let attached_str = if attached_count > 0 {
            format!("[+{}] ", attached_count)
        } else {
            String::new()
        };

        let active_notification = self.notification.as_ref().and_then(|(msg, set_at)| {
            if set_at.elapsed() < NOTIFICATION_TIMEOUT {
                Some(msg.as_str())
            } else {
                None
            }
        });

        if let Some(notif) = active_notification {
            row([
                styled(mode_str.to_string(), mode_style.bold()),
                styled(separator.to_string(), Style::new().fg(Color::DarkGray)),
                styled(ctx_str, Style::new().fg(Color::DarkGray)),
                styled(attached_str, Style::new().fg(Color::Cyan)),
                spacer(),
                styled(format!(" {} ", notif), Style::new().fg(Color::Yellow)),
            ])
        } else {
            row([
                styled(mode_str.to_string(), mode_style.bold()),
                styled(separator.to_string(), Style::new().fg(Color::DarkGray)),
                styled(ctx_str, Style::new().fg(Color::DarkGray)),
                styled(attached_str, Style::new().fg(Color::Cyan)),
            ])
        }
    }

    fn render_user_prompt(&self, content: &str) -> Node {
        let width = terminal_width();
        let top_edge = styled("▄".repeat(width), Style::new().fg(INPUT_BG));
        let bottom_edge = styled("▀".repeat(width), Style::new().fg(INPUT_BG));

        let prefix = " > ";
        let suffix = " ";
        let used = prefix.len() + content.len() + suffix.len();
        let padding = " ".repeat(width.saturating_sub(used));
        let content_line = styled(
            format!("{}{}{}{}", prefix, content, padding, suffix),
            Style::new().bg(INPUT_BG),
        );

        col([text(""), top_edge, content_line, bottom_edge])
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
        let is_focused = ctx.is_focused(FOCUS_INPUT);
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

        let used_width = prompt.len() + display_content.len() + 1;
        let padding = " ".repeat(width.saturating_sub(used_width));

        let input_node = col([
            top_edge,
            row([
                styled(prompt, Style::new().bg(bg)),
                Node::Input(crate::tui::ink::node::InputNode {
                    value: display_content.to_string(),
                    cursor: self.input.cursor().saturating_sub(
                        if matches!(input_mode, InputMode::Command | InputMode::Shell) {
                            1
                        } else {
                            0
                        },
                    ),
                    placeholder: None,
                    style: Style::new().bg(bg),
                    focused: is_focused,
                }),
                styled(format!("{} ", padding), Style::new().bg(bg)),
            ]),
            bottom_edge,
        ]);

        focusable_auto(FOCUS_INPUT, input_node)
    }

    fn get_popup_items(&self) -> Vec<PopupItemNode> {
        let filter = self.popup_filter.to_lowercase();

        match self.popup_kind {
            AutocompleteKind::File => self
                .workspace_files
                .iter()
                .filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter))
                .take(15)
                .map(|f| PopupItemNode {
                    label: f.clone(),
                    description: None,
                    kind: Some("file".to_string()),
                })
                .collect(),
            AutocompleteKind::Note => self
                .kiln_notes
                .iter()
                .filter(|n| filter.is_empty() || n.to_lowercase().contains(&filter))
                .take(15)
                .map(|n| PopupItemNode {
                    label: n.clone(),
                    description: None,
                    kind: Some("note".to_string()),
                })
                .collect(),
            AutocompleteKind::Command => vec![
                PopupItemNode {
                    label: "semantic_search".to_string(),
                    description: Some("Search notes by meaning".to_string()),
                    kind: Some("tool".to_string()),
                },
                PopupItemNode {
                    label: "create_note".to_string(),
                    description: Some("Create a new note".to_string()),
                    kind: Some("tool".to_string()),
                },
                PopupItemNode {
                    label: "/mode".to_string(),
                    description: Some("Cycle chat mode".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/clear".to_string(),
                    description: Some("Clear history".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/help".to_string(),
                    description: Some("Show help".to_string()),
                    kind: Some("command".to_string()),
                },
            ]
            .into_iter()
            .filter(|c| filter.is_empty() || c.label.to_lowercase().contains(&filter))
            .collect(),
            AutocompleteKind::SlashCommand => vec![
                PopupItemNode {
                    label: "/mode".to_string(),
                    description: Some("Cycle chat mode".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/plan".to_string(),
                    description: Some("Set plan mode".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/act".to_string(),
                    description: Some("Set act mode".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/auto".to_string(),
                    description: Some("Set auto mode".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/clear".to_string(),
                    description: Some("Clear history".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/help".to_string(),
                    description: Some("Show help".to_string()),
                    kind: Some("command".to_string()),
                },
                PopupItemNode {
                    label: "/quit".to_string(),
                    description: Some("Exit chat".to_string()),
                    kind: Some("command".to_string()),
                },
            ]
            .into_iter()
            .filter(|c| filter.is_empty() || c.label.to_lowercase().contains(&filter))
            .collect(),
            AutocompleteKind::ReplCommand => vec![
                PopupItemNode {
                    label: ":quit".to_string(),
                    description: Some("Exit chat".to_string()),
                    kind: Some("repl".to_string()),
                },
                PopupItemNode {
                    label: ":help".to_string(),
                    description: Some("Show help".to_string()),
                    kind: Some("repl".to_string()),
                },
                PopupItemNode {
                    label: ":palette".to_string(),
                    description: Some("Open command palette".to_string()),
                    kind: Some("repl".to_string()),
                },
                PopupItemNode {
                    label: ":commands".to_string(),
                    description: Some("Open command palette".to_string()),
                    kind: Some("repl".to_string()),
                },
            ]
            .into_iter()
            .filter(|c| filter.is_empty() || c.label.to_lowercase().contains(&filter))
            .collect(),
            AutocompleteKind::None => vec![],
        }
    }

    fn render_popup(&self) -> Node {
        when(
            self.show_popup && self.popup_kind != AutocompleteKind::None,
            {
                let items = self.get_popup_items();
                if items.is_empty() {
                    Node::Empty
                } else {
                    focusable(FOCUS_POPUP, popup(items, self.popup_selected, 10))
                }
            },
        )
    }

    fn insert_autocomplete_selection(&mut self, label: &str) {
        match self.popup_kind {
            AutocompleteKind::File => {
                let content = self.input.content().to_string();
                let trigger_pos = self.popup_trigger_pos;
                let prefix = &content[..trigger_pos];
                let replacement = format!("@{} ", label);
                let suffix = &content[self.input.cursor()..];
                let new_content = format!("{}{}{}", prefix, replacement, suffix);
                let new_cursor = prefix.len() + replacement.len();

                self.input.handle(InputAction::Clear);
                for ch in new_content.chars() {
                    self.input.handle(InputAction::Insert(ch));
                }
                while self.input.cursor() > new_cursor {
                    self.input.handle(InputAction::Left);
                }

                let context_item = format!("@{}", label);
                if !self.attached_context.contains(&context_item) {
                    self.attached_context.push(context_item);
                }
            }
            AutocompleteKind::Note => {
                let content = self.input.content().to_string();
                let trigger_pos = self.popup_trigger_pos;
                let prefix = &content[..trigger_pos];
                let replacement = format!("[[{}]] ", label);
                let suffix = &content[self.input.cursor()..];
                let new_content = format!("{}{}{}", prefix, replacement, suffix);
                let new_cursor = prefix.len() + replacement.len();

                self.input.handle(InputAction::Clear);
                for ch in new_content.chars() {
                    self.input.handle(InputAction::Insert(ch));
                }
                while self.input.cursor() > new_cursor {
                    self.input.handle(InputAction::Left);
                }

                let context_item = format!("[[{}]]", label);
                if !self.attached_context.contains(&context_item) {
                    self.attached_context.push(context_item);
                }
            }
            AutocompleteKind::Command => {
                self.status = format!("Selected: {}", label);
            }
            AutocompleteKind::SlashCommand => {
                self.input.handle(InputAction::Clear);
                for ch in label.chars() {
                    self.input.handle(InputAction::Insert(ch));
                }
            }
            AutocompleteKind::ReplCommand => {
                self.input.handle(InputAction::Clear);
                for ch in label.chars() {
                    self.input.handle(InputAction::Insert(ch));
                }
            }
            AutocompleteKind::None => {}
        }

        self.popup_kind = AutocompleteKind::None;
        self.show_popup = false;
        self.popup_filter.clear();
    }
}

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::ink::focus::FocusContext;
    use crate::tui::ink::render::render_to_string;

    #[test]
    fn test_mode_cycle() {
        assert_eq!(ChatMode::Plan.cycle(), ChatMode::Act);
        assert_eq!(ChatMode::Act.cycle(), ChatMode::Auto);
        assert_eq!(ChatMode::Auto.cycle(), ChatMode::Plan);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(ChatMode::parse("plan"), ChatMode::Plan);
        assert_eq!(ChatMode::parse("act"), ChatMode::Act);
        assert_eq!(ChatMode::parse("auto"), ChatMode::Auto);
        assert_eq!(ChatMode::parse("unknown"), ChatMode::Plan);
    }

    #[test]
    fn test_app_init() {
        let app = InkChatApp::init();
        assert!(app.items.is_empty());
        assert!(!app.streaming.active);
        assert_eq!(app.mode, ChatMode::Plan);
    }

    #[test]
    fn test_user_message() {
        let mut app = InkChatApp::init();
        app.add_user_message("Hello".to_string());

        assert_eq!(app.items.len(), 1);
        assert!(matches!(
            &app.items[0],
            ChatItem::Message { role: Role::User, content, .. } if content == "Hello"
        ));
    }

    #[test]
    fn test_streaming_flow() {
        let mut app = InkChatApp::init();

        app.on_message(ChatAppMsg::TextDelta("Hello ".to_string()));
        assert!(app.streaming.active);
        assert_eq!(app.streaming.content, "Hello ");

        app.on_message(ChatAppMsg::TextDelta("World".to_string()));
        assert_eq!(app.streaming.content, "Hello World");

        app.on_message(ChatAppMsg::StreamComplete);
        assert!(!app.streaming.active);
        assert_eq!(app.items.len(), 1);
        assert!(matches!(
            &app.items[0],
            ChatItem::Message { content, .. } if content == "Hello World"
        ));
    }

    #[test]
    fn test_tool_call_flow() {
        let mut app = InkChatApp::init();

        app.on_message(ChatAppMsg::ToolCall {
            name: "Read".to_string(),
            args: r#"{"path":"file.md","offset":10}"#.to_string(),
        });
        assert_eq!(app.items.len(), 1);
        assert!(matches!(
            &app.items[0],
            ChatItem::ToolCall { name, complete: false, .. } if name == "Read"
        ));

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "Read".to_string(),
            delta: "line 1\n".to_string(),
        });
        assert!(matches!(
            &app.items[0],
            ChatItem::ToolCall { result, .. } if result == "line 1\n"
        ));

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "Read".to_string(),
            delta: "line 2\n".to_string(),
        });
        assert!(matches!(
            &app.items[0],
            ChatItem::ToolCall { result, .. } if result == "line 1\nline 2\n"
        ));

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "Read".to_string(),
        });
        assert!(matches!(
            &app.items[0],
            ChatItem::ToolCall { complete: true, .. }
        ));
    }

    #[test]
    fn test_slash_commands() {
        let mut app = InkChatApp::init();

        assert_eq!(app.mode, ChatMode::Plan);
        app.handle_slash_command("/mode");
        assert_eq!(app.mode, ChatMode::Act);

        app.handle_slash_command("/plan");
        assert_eq!(app.mode, ChatMode::Plan);

        app.add_user_message("test".to_string());
        assert_eq!(app.items.len(), 1);
        app.handle_slash_command("/clear");
        assert!(app.items.is_empty());
    }

    #[test]
    fn test_quit_command() {
        let mut app = InkChatApp::init();
        let action = app.handle_slash_command("/quit");
        assert!(action.is_quit());
    }

    #[test]
    fn test_view_renders() {
        use crate::tui::ink::focus::FocusContext;

        let mut app = InkChatApp::init();
        app.add_user_message("Hello".to_string());
        app.on_message(ChatAppMsg::TextDelta("Hi there".to_string()));

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let _node = app.view(&ctx);
    }

    #[test]
    fn test_tool_call_renders_with_result() {
        use crate::tui::ink::focus::FocusContext;
        use crate::tui::ink::render::render_to_string;

        let mut app = InkChatApp::init();

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"README.md","offset":1,"limit":200}"#.to_string(),
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let node = app.view(&ctx);
        let output = render_to_string(&node, 80);

        assert!(output.contains("read_file"), "should show tool name");
        assert!(output.contains("path="), "should show args");
        assert!(output.contains("…"), "should show pending ellipsis");

        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "# README\nThis is the content.".to_string(),
        });

        let node = app.view(&ctx);
        let output = render_to_string(&node, 80);
        assert!(
            output.contains("README") || output.contains("content"),
            "should show streaming output while running"
        );

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
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
    fn test_format_tool_args() {
        let args = r#"{"path":"file.md","offset":10}"#;
        let formatted = InkChatApp::format_tool_args(args);
        assert!(formatted.contains("path="));
        assert!(formatted.contains("offset="));
    }

    #[test]
    fn test_format_tool_args_with_newlines() {
        let args = r#"{"content":"line1\nline2\nline3"}"#;
        let formatted = InkChatApp::format_tool_args(args);
        assert!(formatted.contains("↵"), "newlines should be collapsed to ↵");
        assert!(
            !formatted.contains('\n'),
            "should not contain literal newlines"
        );
    }

    #[test]
    fn test_format_tool_args_empty_object() {
        let formatted = InkChatApp::format_tool_args("{}");
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_format_tool_args_truncates_long_values() {
        let long_val = "a".repeat(100);
        let args = format!(r#"{{"content":"{}"}}"#, long_val);
        let formatted = InkChatApp::format_tool_args(&args);

        assert!(formatted.contains("…"), "Long values should be truncated");
        assert!(
            formatted.len() < 100,
            "Formatted output should be shorter than input"
        );
    }

    #[test]
    fn test_context_usage_updates() {
        let mut app = InkChatApp::init();

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
    fn test_context_percentage_zero_total() {
        let mut app = InkChatApp::init();

        app.on_message(ChatAppMsg::ContextUsage {
            used: 1000,
            total: 0,
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(output.contains("0%"), "Should show 0% when total is 0");
    }

    #[test]
    fn test_status_shows_mode_indicator() {
        let mut app = InkChatApp::init();
        app.set_mode(ChatMode::Act);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(output.contains("Act"), "Status should show Act mode");
    }

    #[test]
    fn test_error_message_clears_streaming() {
        let mut app = InkChatApp::init();

        app.on_message(ChatAppMsg::TextDelta("partial response".to_string()));
        assert!(app.is_streaming());

        app.on_message(ChatAppMsg::Error("Connection lost".to_string()));
        assert!(!app.is_streaming(), "Error should stop streaming");
    }
}
