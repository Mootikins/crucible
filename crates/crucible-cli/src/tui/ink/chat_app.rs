use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::event::{Event, InputAction, InputBuffer};
use crate::tui::ink::markdown::markdown_to_node_with_width;
use crate::tui::ink::node::*;
use crate::tui::ink::style::{Color, Gap, Style};
use crossterm::event::KeyCode;
use std::time::Duration;

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

    pub fn from_str(s: &str) -> Self {
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

#[derive(Debug, Clone, Default)]
struct StreamingState {
    content: String,
    active: bool,
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
    context_used: usize,
    context_total: usize,
    last_ctrl_c: Option<std::time::Instant>,
    notification: Option<String>,
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
            context_used: 0,
            context_total: 128000,
            last_ctrl_c: None,
            notification: None,
        }
    }
}

impl App for InkChatApp {
    type Msg = ChatAppMsg;

    fn init() -> Self {
        Self::default()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Node {
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
                self.mode = ChatMode::from_str(&mode);
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

    pub fn is_streaming(&self) -> bool {
        self.streaming.active
    }

    pub fn input_content(&self) -> &str {
        self.input.content()
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        self.error = None;

        if key.code == KeyCode::F(1) {
            self.show_popup = !self.show_popup;
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
            self.notification = Some("Ctrl+C again to quit".to_string());
            return Action::Continue;
        } else {
            self.last_ctrl_c = None;
            self.notification = None;
        }

        let action = InputAction::from(key);
        if let Some(submitted) = self.input.handle(action) {
            return self.handle_submit(submitted);
        }

        Action::Continue
    }

    fn handle_popup_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        match key.code {
            KeyCode::Esc => {
                self.show_popup = false;
            }
            KeyCode::Up => {
                self.popup_selected = self.popup_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.demo_popup_items().len().saturating_sub(1);
                self.popup_selected = (self.popup_selected + 1).min(max);
            }
            KeyCode::Enter => {
                let items = self.demo_popup_items();
                if let Some(item) = items.get(self.popup_selected) {
                    self.status = format!("Selected: {}", item.0);
                }
                self.show_popup = false;
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
                self.add_system_message("REPL commands: :q(uit), :h(elp)".to_string());
                Action::Continue
            }
            _ => {
                self.error = Some(format!("Unknown REPL command: {}", cmd));
                Action::Continue
            }
        }
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

        let separator = styled(" │ ", Style::new().fg(Color::DarkGray));

        let context_percent = if self.context_total > 0 {
            (self.context_used as f64 / self.context_total as f64 * 100.0).round() as usize
        } else {
            0
        };

        let left = row([
            styled(
                match self.mode {
                    ChatMode::Plan => " Plan",
                    ChatMode::Act => " Act",
                    ChatMode::Auto => " Auto",
                },
                mode_style.bold(),
            ),
            separator,
            styled(
                format!("{}% ctx", context_percent),
                Style::new().fg(Color::DarkGray),
            ),
        ]);

        if let Some(ref notif) = self.notification {
            row([
                left,
                spacer(),
                styled(format!("{} ", notif), Style::new().fg(Color::Yellow)),
            ])
        } else {
            left
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

    fn render_input(&self, ctx: &ViewContext<'_>) -> Node {
        let width = terminal_width();
        let is_focused = ctx.is_focused(FOCUS_INPUT);
        let prompt_style = match self.mode {
            ChatMode::Plan => Style::new().fg(Color::Blue).bg(INPUT_BG),
            ChatMode::Act => Style::new().fg(Color::Green).bg(INPUT_BG),
            ChatMode::Auto => Style::new().fg(Color::Yellow).bg(INPUT_BG),
        };

        let top_edge = styled("▄".repeat(width), Style::new().fg(INPUT_BG));
        let bottom_edge = styled("▀".repeat(width), Style::new().fg(INPUT_BG));

        let prompt = " > ";
        let content = self.input.content();
        let used_width = prompt.len() + content.len() + 1;
        let padding = " ".repeat(width.saturating_sub(used_width));

        let input_node = col([
            top_edge,
            row([
                styled(prompt, prompt_style),
                Node::Input(crate::tui::ink::node::InputNode {
                    value: content.to_string(),
                    cursor: self.input.cursor(),
                    placeholder: None,
                    style: Style::new().bg(INPUT_BG),
                    focused: is_focused,
                }),
                styled(format!("{} ", padding), Style::new().bg(INPUT_BG)),
            ]),
            bottom_edge,
        ]);

        focusable_auto(FOCUS_INPUT, input_node)
    }

    fn demo_popup_items(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("semantic_search", "Search notes by meaning", "tool"),
            ("create_note", "Create a new note", "tool"),
            ("get_outlinks", "Get outgoing links", "tool"),
            ("get_inlinks", "Get incoming links", "tool"),
            ("list_notes", "List all notes", "tool"),
            ("/mode", "Cycle chat mode", "command"),
            ("/clear", "Clear history", "command"),
            ("/help", "Show help", "command"),
        ]
    }

    fn render_popup(&self) -> Node {
        when(self.show_popup, {
            let items: Vec<PopupItemNode> = self
                .demo_popup_items()
                .into_iter()
                .map(|(label, desc, kind)| PopupItemNode {
                    label: label.to_string(),
                    description: Some(desc.to_string()),
                    kind: Some(kind.to_string()),
                })
                .collect();

            focusable(FOCUS_POPUP, popup(items, self.popup_selected, 10))
        })
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

    #[test]
    fn test_mode_cycle() {
        assert_eq!(ChatMode::Plan.cycle(), ChatMode::Act);
        assert_eq!(ChatMode::Act.cycle(), ChatMode::Auto);
        assert_eq!(ChatMode::Auto.cycle(), ChatMode::Plan);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(ChatMode::from_str("plan"), ChatMode::Plan);
        assert_eq!(ChatMode::from_str("act"), ChatMode::Act);
        assert_eq!(ChatMode::from_str("auto"), ChatMode::Auto);
        assert_eq!(ChatMode::from_str("unknown"), ChatMode::Plan);
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
}
