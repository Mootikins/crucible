use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::terminal::Terminal;
use crossterm::event::{Event as CtEvent, KeyCode, KeyModifiers};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct InkRunner<A: App> {
    app: A,
    terminal: Terminal,
    tick_rate: Duration,
    msg_rx: Option<mpsc::UnboundedReceiver<A::Msg>>,
    focus: FocusContext,
}

impl<A: App> InkRunner<A> {
    pub fn new() -> io::Result<Self> {
        let app = A::init();
        let tick_rate = app.tick_rate().unwrap_or(Duration::from_millis(100));

        Ok(Self {
            app,
            terminal: Terminal::new()?,
            tick_rate,
            msg_rx: None,
            focus: FocusContext::new(),
        })
    }

    pub fn with_message_channel(mut self) -> (Self, mpsc::UnboundedSender<A::Msg>) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.msg_rx = Some(rx);
        (self, tx)
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.terminal.enter()?;

        loop {
            let ctx = ViewContext::new(&self.focus);
            let tree = self.app.view(&ctx);
            self.terminal.render(&tree)?;

            if let Some(rx) = &mut self.msg_rx {
                while let Ok(msg) = rx.try_recv() {
                    let action = self.app.on_message(msg);
                    if action.is_quit() {
                        self.terminal.exit()?;
                        return Ok(());
                    }
                }
            }

            let event = self.poll_event()?;
            if let Some(ev) = event {
                if self.handle_focus_keys(&ev) {
                    continue;
                }
                let action = self.app.update(ev);
                if self.process_action(action)? {
                    break;
                }
            }
        }

        self.terminal.exit()?;
        Ok(())
    }

    fn handle_focus_keys(&mut self, event: &Event) -> bool {
        if let Event::Key(key) = event {
            if key.code == KeyCode::Tab {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus.focus_prev();
                } else {
                    self.focus.focus_next();
                }
                return true;
            }
        }
        false
    }

    fn poll_event(&mut self) -> io::Result<Option<Event>> {
        if let Some(ct_event) = self.terminal.poll_event(self.tick_rate)? {
            let event = match ct_event {
                CtEvent::Key(key) => Event::Key(key),
                CtEvent::Resize(w, h) => {
                    self.terminal.handle_resize()?;
                    Event::Resize {
                        width: w,
                        height: h,
                    }
                }
                _ => Event::Tick,
            };
            Ok(Some(event))
        } else {
            Ok(Some(Event::Tick))
        }
    }

    fn process_action(&mut self, action: Action<A::Msg>) -> io::Result<bool> {
        match action {
            Action::Quit => Ok(true),
            Action::Continue => Ok(false),
            Action::Send(msg) => {
                let action = self.app.on_message(msg);
                self.process_action(action)
            }
            Action::Batch(actions) => {
                for action in actions {
                    if self.process_action(action)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

pub fn run_sync<A: App>(app: A) -> io::Result<()> {
    let mut terminal = Terminal::new()?;
    terminal.enter()?;

    let tick_rate = app.tick_rate().unwrap_or(Duration::from_millis(100));
    let mut app = app;
    let mut focus = FocusContext::new();

    loop {
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        terminal.render(&tree)?;

        let event = if let Some(ct_event) = terminal.poll_event(tick_rate)? {
            match ct_event {
                CtEvent::Key(key) => Event::Key(key),
                CtEvent::Resize(w, h) => {
                    terminal.handle_resize()?;
                    Event::Resize {
                        width: w,
                        height: h,
                    }
                }
                _ => Event::Tick,
            }
        } else {
            Event::Tick
        };

        if let Event::Key(key) = &event {
            if key.code == KeyCode::Tab {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    focus.focus_prev();
                } else {
                    focus.focus_next();
                }
                continue;
            }
        }

        let action = app.update(event);
        if action.is_quit() {
            break;
        }
    }

    terminal.exit()?;
    Ok(())
}
