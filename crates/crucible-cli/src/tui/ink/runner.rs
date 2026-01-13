use crate::tui::ink::app::{Action, App};
use crate::tui::ink::event::Event;
use crate::tui::ink::terminal::Terminal;
use crossterm::event::Event as CtEvent;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct InkRunner<A: App> {
    app: A,
    terminal: Terminal,
    tick_rate: Duration,
    msg_rx: Option<mpsc::UnboundedReceiver<A::Msg>>,
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
            let tree = self.app.view();
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
                let action = self.app.update(ev);
                if self.process_action(action)? {
                    break;
                }
            }
        }

        self.terminal.exit()?;
        Ok(())
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

    loop {
        let tree = app.view();
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

        let action = app.update(event);
        if action.is_quit() {
            break;
        }
    }

    terminal.exit()?;
    Ok(())
}
