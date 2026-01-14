use crate::tui::ink::*;
use crossterm::event::KeyCode;
use std::time::Duration;

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    input: InputBuffer,
    streaming: Option<StreamingMessage>,
    spinner_frame: usize,
}

struct ChatMessage {
    id: String,
    role: Role,
    content: String,
}

#[derive(Clone, Copy)]
enum Role {
    User,
    Assistant,
}

struct StreamingMessage {
    content: String,
    complete: bool,
}

pub enum ChatMsg {
    StreamChunk(String),
    StreamComplete,
}

impl App for ChatApp {
    type Msg = ChatMsg;

    fn init() -> Self {
        Self {
            messages: Vec::new(),
            input: InputBuffer::new(),
            streaming: None,
            spinner_frame: 0,
        }
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        col([
            fragment(self.messages.iter().map(|msg| render_message(msg))),
            self.render_streaming(),
            spacer(),
            self.render_input(),
        ])
    }

    fn update(&mut self, event: Event) -> Action<ChatMsg> {
        match event {
            Event::Key(key) => {
                if key.code == KeyCode::Esc {
                    return Action::Quit;
                }

                let action = InputAction::from(key);
                if let Some(submitted) = self.input.handle(action) {
                    self.submit_message(submitted);
                }

                Action::Continue
            }
            Event::Tick => {
                self.spinner_frame = (self.spinner_frame + 1) % 4;
                Action::Continue
            }
            Event::Resize { .. } => Action::Continue,
            Event::Quit => Action::Quit,
        }
    }

    fn on_message(&mut self, msg: ChatMsg) -> Action<ChatMsg> {
        match msg {
            ChatMsg::StreamChunk(chunk) => {
                if let Some(streaming) = &mut self.streaming {
                    streaming.content.push_str(&chunk);
                }
                Action::Continue
            }
            ChatMsg::StreamComplete => {
                if let Some(streaming) = self.streaming.take() {
                    let id = format!("assistant-{}", self.messages.len());
                    self.messages.push(ChatMessage {
                        id,
                        role: Role::Assistant,
                        content: streaming.content,
                    });
                }
                Action::Continue
            }
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(100))
    }
}

impl ChatApp {
    fn submit_message(&mut self, content: String) {
        let id = format!("user-{}", self.messages.len());
        self.messages.push(ChatMessage {
            id,
            role: Role::User,
            content,
        });

        self.streaming = Some(StreamingMessage {
            content: String::new(),
            complete: false,
        });
    }

    fn render_streaming(&self) -> Node {
        match &self.streaming {
            None => Node::Empty,
            Some(s) if s.content.is_empty() => col([
                text(""),
                spinner(Some("Thinking...".into()), self.spinner_frame),
            ]),
            Some(s) => col([
                text(""),
                row([
                    styled(" . ", Style::new().fg(Color::DarkGray)),
                    text(&s.content),
                ]),
                spinner(Some("Generating...".into()), self.spinner_frame),
            ]),
        }
    }

    fn render_input(&self) -> Node {
        row([
            styled(" > ", Style::new().fg(Color::Cyan)),
            text_input(self.input.content(), self.input.cursor()),
        ])
    }
}

fn render_message(msg: &ChatMessage) -> Node {
    let (prefix, style) = match msg.role {
        Role::User => (
            " > ",
            Style::new().fg(Color::Cyan).bg(Color::Rgb(40, 40, 60)),
        ),
        Role::Assistant => (" . ", Style::new().fg(Color::DarkGray)),
    };

    scrollback(
        &msg.id,
        [col([
            text(""),
            row([styled(prefix, style), text(&msg.content)]),
        ])],
    )
}
