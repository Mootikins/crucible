//! Ink-based chat application integrating with AgentEventBridge

use crate::tui::ink::app::{Action, App};
use crate::tui::ink::event::{Event, InputAction, InputBuffer};
use crate::tui::ink::node::*;
use crate::tui::ink::style::{Color, Style};
use crossterm::event::KeyCode;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ChatAppMsg {
    UserMessage(String),
    TextDelta(String),
    ToolCall { name: String, args: String },
    ToolResult { name: String, result: String },
    StreamComplete,
    Error(String),
    Status(String),
    ModeChanged(String),
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub tool_calls: Vec<ToolCallInfo>,
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub args: String,
    pub result: Option<String>,
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
    tool_calls: Vec<ToolCallInfo>,
    active: bool,
}

pub struct InkChatApp {
    messages: Vec<Message>,
    input: InputBuffer,
    streaming: StreamingState,
    spinner_frame: usize,
    mode: ChatMode,
    status: String,
    error: Option<String>,
    message_counter: usize,
    on_submit: Option<Box<dyn Fn(String) + Send + Sync>>,
}

impl Default for InkChatApp {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input: InputBuffer::new(),
            streaming: StreamingState::default(),
            spinner_frame: 0,
            mode: ChatMode::Plan,
            status: "Ready".to_string(),
            error: None,
            message_counter: 0,
            on_submit: None,
        }
    }
}

impl App for InkChatApp {
    type Msg = ChatAppMsg;

    fn init() -> Self {
        Self::default()
    }

    fn view(&self) -> Node {
        col([
            self.render_messages(),
            self.render_streaming(),
            self.render_error(),
            spacer(),
            self.render_status(),
            self.render_input(),
        ])
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
                self.streaming.tool_calls.push(ToolCallInfo {
                    name,
                    args,
                    result: None,
                });
                Action::Continue
            }
            ChatAppMsg::ToolResult { name, result } => {
                if let Some(tc) = self
                    .streaming
                    .tool_calls
                    .iter_mut()
                    .find(|t| t.name == name)
                {
                    tc.result = Some(result);
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

        if key.code == KeyCode::Esc {
            return Action::Quit;
        }

        if key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            if self.streaming.active {
                self.streaming.active = false;
                self.status = "Cancelled".to_string();
                return Action::Continue;
            }
            return Action::Quit;
        }

        let action = InputAction::from(key);
        if let Some(submitted) = self.input.handle(action) {
            return self.handle_submit(submitted);
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
            tool_calls: Vec::new(),
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
                self.messages.clear();
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

    fn add_user_message(&mut self, content: String) {
        self.message_counter += 1;
        self.messages.push(Message {
            id: format!("user-{}", self.message_counter),
            role: Role::User,
            content,
            tool_calls: Vec::new(),
        });
    }

    fn add_system_message(&mut self, content: String) {
        self.message_counter += 1;
        self.messages.push(Message {
            id: format!("system-{}", self.message_counter),
            role: Role::System,
            content,
            tool_calls: Vec::new(),
        });
    }

    fn finalize_streaming(&mut self) {
        if !self.streaming.content.is_empty() || !self.streaming.tool_calls.is_empty() {
            self.message_counter += 1;
            self.messages.push(Message {
                id: format!("assistant-{}", self.message_counter),
                role: Role::Assistant,
                content: std::mem::take(&mut self.streaming.content),
                tool_calls: std::mem::take(&mut self.streaming.tool_calls),
            });
        }
        self.streaming.active = false;
        self.status = "Ready".to_string();
    }

    fn render_messages(&self) -> Node {
        fragment(self.messages.iter().map(|msg| self.render_message(msg)))
    }

    fn render_message(&self, msg: &Message) -> Node {
        let (prefix, style) = match msg.role {
            Role::User => (" > ", Style::new().fg(Color::Cyan)),
            Role::Assistant => (" . ", Style::new().fg(Color::DarkGray)),
            Role::System => (" * ", Style::new().fg(Color::Yellow).dim()),
        };

        let tool_nodes: Vec<Node> = msg
            .tool_calls
            .iter()
            .map(|tc| self.render_tool_call(tc))
            .collect();

        let display = format!("{}{}", prefix, &msg.content);

        let content_node = if tool_nodes.is_empty() {
            col([text(""), styled(&display, style)])
        } else {
            col([text(""), styled(&display, style), fragment(tool_nodes)])
        };

        scrollback(&msg.id, [content_node])
    }

    fn render_tool_call(&self, tc: &ToolCallInfo) -> Node {
        let status_icon = if tc.result.is_some() { "✓" } else { "…" };
        col([
            row([
                styled(
                    format!("   {} ", status_icon),
                    Style::new().fg(Color::DarkGray),
                ),
                styled(&tc.name, Style::new().fg(Color::Blue)),
            ]),
            if let Some(ref result) = tc.result {
                let truncated = if result.len() > 100 {
                    format!("{}...", &result[..100])
                } else {
                    result.clone()
                };
                styled(format!("     {}", truncated), Style::new().dim())
            } else {
                Node::Empty
            },
        ])
    }

    fn render_streaming(&self) -> Node {
        if !self.streaming.active {
            return Node::Empty;
        }

        if self.streaming.content.is_empty() && self.streaming.tool_calls.is_empty() {
            col([
                text(""),
                spinner(Some("Thinking...".into()), self.spinner_frame),
            ])
        } else {
            let tool_nodes: Vec<Node> = self
                .streaming
                .tool_calls
                .iter()
                .map(|tc| self.render_tool_call(tc))
                .collect();

            let display = format!(" . {}", &self.streaming.content);

            col([
                text(""),
                styled(&display, Style::new().fg(Color::DarkGray)),
                fragment(tool_nodes),
                spinner(Some("Generating...".into()), self.spinner_frame),
            ])
        }
    }

    fn render_error(&self) -> Node {
        match &self.error {
            Some(err) => styled(format!("Error: {}", err), Style::new().fg(Color::Red)),
            None => Node::Empty,
        }
    }

    fn render_status(&self) -> Node {
        let mode_style = match self.mode {
            ChatMode::Plan => Style::new().fg(Color::Blue),
            ChatMode::Act => Style::new().fg(Color::Green),
            ChatMode::Auto => Style::new().fg(Color::Yellow),
        };

        row([
            styled(format!("[{}]", self.mode.as_str()), mode_style.bold()),
            styled(" ", Style::default()),
            styled(&self.status, Style::new().dim()),
        ])
    }

    fn render_input(&self) -> Node {
        let prompt_style = match self.mode {
            ChatMode::Plan => Style::new().fg(Color::Blue),
            ChatMode::Act => Style::new().fg(Color::Green),
            ChatMode::Auto => Style::new().fg(Color::Yellow),
        };

        row([
            styled(" > ", prompt_style),
            text_input(self.input.content(), self.input.cursor()),
        ])
    }
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
        assert!(app.messages.is_empty());
        assert!(!app.streaming.active);
        assert_eq!(app.mode, ChatMode::Plan);
    }

    #[test]
    fn test_user_message() {
        let mut app = InkChatApp::init();
        app.add_user_message("Hello".to_string());

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, Role::User);
        assert_eq!(app.messages[0].content, "Hello");
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
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Hello World");
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
        assert_eq!(app.messages.len(), 1);
        app.handle_slash_command("/clear");
        assert!(app.messages.is_empty());
    }

    #[test]
    fn test_quit_command() {
        let mut app = InkChatApp::init();
        let action = app.handle_slash_command("/quit");
        assert!(action.is_quit());
    }

    #[test]
    fn test_view_renders() {
        let mut app = InkChatApp::init();
        app.add_user_message("Hello".to_string());
        app.on_message(ChatAppMsg::TextDelta("Hi there".to_string()));

        let _node = app.view();
    }
}
