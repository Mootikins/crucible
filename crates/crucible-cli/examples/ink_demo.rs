use crossterm::event::KeyCode;
use crucible_cli::tui::ink::*;
use std::time::Duration;

struct DemoChat {
    messages: Vec<Message>,
    input: InputBuffer,
    streaming: Option<String>,
    spinner_frame: usize,
}

struct Message {
    id: String,
    role: Role,
    content: String,
}

#[derive(Clone, Copy)]
enum Role {
    User,
    Assistant,
}

impl App for DemoChat {
    type Msg = ();

    fn init() -> Self {
        Self {
            messages: vec![Message {
                id: "welcome".into(),
                role: Role::Assistant,
                content: "Welcome to Ink TUI demo! Type a message and press Enter.".into(),
            }],
            input: InputBuffer::new(),
            streaming: None,
            spinner_frame: 0,
        }
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        col([
            fragment(self.messages.iter().map(|m| self.render_message(m))),
            self.render_streaming(),
            spacer(),
            self.render_input(),
            self.render_status(),
        ])
    }

    fn update(&mut self, event: Event) -> Action<()> {
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

                if let Some(ref mut content) = self.streaming {
                    let responses = [
                        "The ",
                        "answer ",
                        "to ",
                        "your ",
                        "question ",
                        "is ",
                        "42. ",
                        "This ",
                        "is ",
                        "a ",
                        "simulated ",
                        "streaming ",
                        "response.",
                    ];

                    let word_count = content.split_whitespace().count();
                    if word_count < responses.len() {
                        content.push_str(responses[word_count]);
                    } else {
                        let final_content = std::mem::take(content);
                        self.streaming = None;
                        self.messages.push(Message {
                            id: format!("assistant-{}", self.messages.len()),
                            role: Role::Assistant,
                            content: final_content,
                        });
                    }
                }

                Action::Continue
            }
            Event::Resize { .. } => Action::Continue,
            Event::Quit => Action::Quit,
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(150))
    }
}

impl DemoChat {
    fn submit_message(&mut self, content: String) {
        self.messages.push(Message {
            id: format!("user-{}", self.messages.len()),
            role: Role::User,
            content,
        });
        self.streaming = Some(String::new());
    }

    fn render_message(&self, msg: &Message) -> Node {
        let (prefix, prefix_style) = match msg.role {
            Role::User => (" > ", Style::new().fg(Color::Cyan)),
            Role::Assistant => (" . ", Style::new().fg(Color::DarkGray)),
        };

        scrollback(
            &msg.id,
            [col([
                text(""),
                row([styled(prefix, prefix_style), text(&msg.content)]),
            ])],
        )
    }

    fn render_streaming(&self) -> Node {
        match &self.streaming {
            None => Node::Empty,
            Some(content) if content.is_empty() => col([
                text(""),
                row([spinner(Some("Thinking...".into()), self.spinner_frame)]),
            ]),
            Some(content) => col([
                text(""),
                row([
                    styled(" . ", Style::new().fg(Color::DarkGray)),
                    text(content),
                ]),
                row([spinner(Some("Generating...".into()), self.spinner_frame)]),
            ]),
        }
    }

    fn render_input(&self) -> Node {
        row([
            styled(" > ", Style::new().fg(Color::Cyan)),
            if self.input.is_empty() {
                styled("Type a message...", Style::new().dim())
            } else {
                text(self.input.content())
            },
        ])
    }

    fn render_status(&self) -> Node {
        row([styled(
            " [ESC] quit  [Enter] send ",
            Style::new().fg(Color::DarkGray),
        )])
    }
}

fn main() -> std::io::Result<()> {
    run_sync(DemoChat::init())
}
