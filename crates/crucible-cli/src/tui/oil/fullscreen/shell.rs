//! Several full-screen panes behind one key: chat sessions, each with its
//! own `OilChatApp` and view state, and a plugin buffer.
//!
//! This proves the view model, not the wiring. A chat pane here is fed by
//! its owner (a test, or the demo's fake agent); no daemon connects to it.
//! The runner of `cru chat` still drives one session.

use super::scroll::Scroll;
use super::{Frame, FullscreenView, ViewAction};
use crate::tui::oil::app::{Action, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crossterm::event::{KeyCode, MouseEvent, MouseEventKind};
use crucible_oil::cell_grid::CellGrid;

/// Moves to the next pane.
pub const SWITCH_KEY: KeyCode = KeyCode::F(4);

/// The rows above a pane: the tab row. A pane draws below them, so a mouse
/// row moves up by this much before the pane reads it.
const PANE_TOP: u16 = 1;

/// A scrolling buffer of lines that a plugin owns.
///
/// The source gives one line on request, so the view reads only the rows on
/// screen: a buffer of a million lines costs what its visible rows cost.
/// A line is drawn as it comes, cut at the screen width; the source wraps.
pub struct PluginBuffer {
    pub title: String,
    len: Box<dyn Fn() -> usize>,
    line: Box<dyn Fn(usize) -> String>,
    scroll: Scroll,
    /// Rows the buffer showed at the last frame.
    height: usize,
}

impl PluginBuffer {
    pub fn new(
        title: impl Into<String>,
        len: impl Fn() -> usize + 'static,
        line: impl Fn(usize) -> String + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            len: Box::new(len),
            line: Box::new(line),
            scroll: Scroll::default(),
            height: 0,
        }
    }

    pub fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// Draw the buffer into `grid` from row `top`, `height` rows.
    fn draw(&mut self, grid: &mut CellGrid, top: usize, height: usize) -> usize {
        self.height = height;
        let total = (self.len)();
        self.scroll.fit(total, height);
        let first = self.scroll.top();
        let mut fetched = 0;
        for y in 0..height.min(total - first.min(total)) {
            grid.blit_line(&(self.line)(first + y), 0, top + y);
            fetched += 1;
        }
        fetched
    }

    fn handle_event(&mut self, event: &Event) -> ViewAction {
        let total = (self.len)();
        let page = self.height.saturating_sub(1).max(1) as isize;
        let delta = match event {
            Event::Key(key) => match key.code {
                KeyCode::PageUp => -page,
                KeyCode::PageDown => page,
                KeyCode::Up => -1,
                KeyCode::Down => 1,
                KeyCode::Home => -(total as isize),
                KeyCode::End => total as isize,
                _ => return ViewAction::Ignored,
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => -3,
                MouseEventKind::ScrollDown => 3,
                _ => return ViewAction::Ignored,
            },
            _ => return ViewAction::Ignored,
        };
        self.scroll.scroll_by(delta, total, self.height);
        ViewAction::Handled
    }
}

/// One chat session in the shell.
pub struct ChatPane {
    pub name: String,
    pub app: OilChatApp,
    pub view: FullscreenView,
}

/// Which pane is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat(usize),
    Buffer,
}

/// What the shell's owner must do after an event.
#[derive(Debug, PartialEq)]
pub enum ShellAction {
    /// Nothing more; draw a frame.
    None,
    /// The view of the active pane asked for this.
    View(ViewAction),
    /// The user sent this message in chat pane `pane`. The owner answers it.
    Sent {
        pane: usize,
        message: String,
    },
    Quit,
}

pub struct FullscreenShell {
    pub chats: Vec<ChatPane>,
    pub buffer: Option<PluginBuffer>,
    /// A short text at the end of the tab row, such as the result of a copy.
    pub note: String,
    active: Pane,
}

impl FullscreenShell {
    pub fn new(chats: Vec<ChatPane>, buffer: Option<PluginBuffer>) -> Self {
        let chats_empty = chats.is_empty();
        Self {
            chats,
            buffer,
            note: String::new(),
            active: if chats_empty {
                Pane::Buffer
            } else {
                Pane::Chat(0)
            },
        }
    }

    pub fn active(&self) -> Pane {
        self.active
    }

    /// The view of the chat pane on screen, if a chat pane is on screen.
    pub fn active_chat_mut(&mut self) -> Option<&mut ChatPane> {
        match self.active {
            Pane::Chat(i) => self.chats.get_mut(i),
            Pane::Buffer => None,
        }
    }

    fn panes(&self) -> Vec<Pane> {
        (0..self.chats.len())
            .map(Pane::Chat)
            .chain(self.buffer.as_ref().map(|_| Pane::Buffer))
            .collect()
    }

    /// Go to the next pane. Each pane keeps its own scroll and selection.
    pub fn switch(&mut self) {
        let panes = self.panes();
        let at = panes.iter().position(|p| *p == self.active).unwrap_or(0);
        self.active = panes[(at + 1) % panes.len()];
    }

    pub fn handle_event(&mut self, event: &Event) -> ShellAction {
        if let Event::Key(key) = event {
            if key.code == SWITCH_KEY {
                self.switch();
                return ShellAction::None;
            }
        }
        let event = match event {
            // The pane reads its own rows, not the screen's.
            Event::Mouse(mouse) => match mouse.row.checked_sub(PANE_TOP) {
                Some(row) => Event::Mouse(MouseEvent { row, ..*mouse }),
                None => return ShellAction::None,
            },
            other => other.clone(),
        };
        let event = &event;
        match self.active {
            Pane::Buffer => match self.buffer.as_mut().map(|b| b.handle_event(event)) {
                Some(ViewAction::Ignored) | None => ShellAction::None,
                Some(action) => ShellAction::View(action),
            },
            Pane::Chat(i) => {
                let pane = &mut self.chats[i];
                match pane.view.handle_event(event, &pane.app) {
                    ViewAction::Ignored => {}
                    action => return ShellAction::View(action),
                }
                let action = pane.app.update(event.clone());
                self.apply(i, action)
            }
        }
    }

    fn apply(&mut self, pane: usize, action: Action<ChatAppMsg>) -> ShellAction {
        match action {
            Action::Quit => ShellAction::Quit,
            Action::Send(ChatAppMsg::UserMessage(message)) => {
                self.chats[pane]
                    .app
                    .on_message(ChatAppMsg::UserMessage(message.clone()));
                ShellAction::Sent { pane, message }
            }
            Action::Send(msg) => {
                let next = self.chats[pane].app.on_message(msg);
                self.apply(pane, next)
            }
            Action::Batch(actions) => {
                let mut last = ShellAction::None;
                for action in actions {
                    match self.apply(pane, action) {
                        ShellAction::None => {}
                        other => last = other,
                    }
                }
                last
            }
            Action::Continue => ShellAction::None,
        }
    }

    /// Build the frame: a tab row, then the active pane below it.
    pub fn frame(&mut self, ctx: &ViewContext<'_>) -> Frame {
        let (width, height) = ctx.terminal_size;
        let mut grid = CellGrid::new(width as usize, height as usize);
        let inner = ViewContext {
            terminal_size: (width, height.saturating_sub(PANE_TOP)),
            ..*ctx
        };
        let mut cursor = None;
        match self.active {
            Pane::Chat(i) => {
                let pane = &mut self.chats[i];
                let frame = pane.view.frame(&mut pane.app, &inner);
                let top = PANE_TOP as usize;
                for y in 0..frame.grid.height() {
                    grid.copy_row_from(y + top, &frame.grid, y);
                }
                cursor = frame.cursor.map(|(x, y)| (x, y + PANE_TOP));
            }
            Pane::Buffer => {
                if let Some(buffer) = self.buffer.as_mut() {
                    buffer.draw(
                        &mut grid,
                        PANE_TOP as usize,
                        height.saturating_sub(PANE_TOP) as usize,
                    );
                }
            }
        }
        grid.blit_line(&self.tab_row(), 0, 0);
        Frame { grid, cursor }
    }

    fn tab_row(&self) -> String {
        let mut row = String::new();
        for pane in self.panes() {
            let label = match pane {
                Pane::Chat(i) => format!(" {} ", self.chats[i].name),
                Pane::Buffer => {
                    let b = self.buffer.as_ref().expect("a buffer pane has a buffer");
                    format!(" {} {}/{} ", b.title, b.scroll.top() + 1, (b.len)())
                }
            };
            if pane == self.active {
                row.push_str(&format!("\x1b[7m{label}\x1b[0m"));
            } else {
                row.push_str(&label);
            }
        }
        row.push_str("\x1b[2m  F4 next pane");
        if !self.note.is_empty() {
            row.push_str(" \u{b7} ");
            row.push_str(&self.note);
        }
        row.push_str("\x1b[0m");
        row
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::fullscreen::fixtures;
    use crate::tui::oil::fullscreen::tests::{frame_at_ctx, screen_text};
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::cell::Cell;
    use std::rc::Rc;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn fake_buffer(lines: usize, fetched: Rc<Cell<usize>>) -> PluginBuffer {
        PluginBuffer::new(
            "log",
            move || lines,
            move |i| {
                fetched.set(fetched.get() + 1);
                format!("log line {i:05}")
            },
        )
    }

    fn shell() -> FullscreenShell {
        let mut first = fixtures::app_with_exchanges(3);
        first.add_system_message("session one".into());
        let mut second = fixtures::app_with_exchanges(1);
        second.add_system_message("session two".into());
        FullscreenShell::new(
            vec![
                ChatPane {
                    name: "one".into(),
                    app: first,
                    view: FullscreenView::new(),
                },
                ChatPane {
                    name: "two".into(),
                    app: second,
                    view: FullscreenView::new(),
                },
            ],
            Some(fake_buffer(10_000, Rc::new(Cell::new(0)))),
        )
    }

    fn frame(shell: &mut FullscreenShell) -> Vec<String> {
        screen_text(&frame_at_ctx(100, 30, |ctx| shell.frame(ctx)))
    }

    #[test]
    fn one_key_switches_between_two_sessions() {
        let mut shell = shell();
        let rows = frame(&mut shell);
        assert!(rows.iter().any(|r| r.contains("session one")), "{rows:#?}");

        shell.handle_event(&key(SWITCH_KEY));
        assert_eq!(shell.active(), Pane::Chat(1));
        let rows = frame(&mut shell);
        assert!(rows.iter().any(|r| r.contains("session two")));
        assert!(!rows.iter().any(|r| r.contains("session one")));
    }

    #[test]
    fn each_session_keeps_its_own_scroll_and_input() {
        let mut shell = shell();
        frame(&mut shell);
        shell.handle_event(&key(KeyCode::PageUp));
        shell.handle_event(&key(KeyCode::Char('x')));
        let held = shell.chats[0].view.scroll();
        assert!(!held.follows());

        shell.handle_event(&key(SWITCH_KEY));
        frame(&mut shell);
        assert!(shell.chats[1].view.scroll().follows());
        shell.handle_event(&key(SWITCH_KEY));
        shell.handle_event(&key(SWITCH_KEY));
        frame(&mut shell);
        assert_eq!(shell.chats[0].view.scroll(), held);
        assert_eq!(shell.chats[0].app.input_content(), "x");
        assert_eq!(shell.chats[1].app.input_content(), "");
    }

    #[test]
    fn a_message_sent_in_a_pane_reaches_only_that_session() {
        let mut shell = shell();
        shell.handle_event(&key(SWITCH_KEY));
        for c in "hi".chars() {
            shell.handle_event(&key(KeyCode::Char(c)));
        }
        let sent = shell.handle_event(&key(KeyCode::Enter));
        assert_eq!(
            sent,
            ShellAction::Sent {
                pane: 1,
                message: "hi".into()
            }
        );
    }

    /// Found in the demo over Zellij: the highlight was one row below the
    /// pointer. The shell draws a pane under its tab row, so a pointer row
    /// must move up by the same row before the pane reads it.
    #[test]
    fn a_drag_in_a_pane_selects_the_text_under_the_pointer() {
        use crossterm::event::MouseButton;
        let mut app = crate::tui::oil::OilChatApp::default();
        app.add_system_message("above the target".into());
        app.add_system_message("target words here".into());
        app.add_system_message("below the target".into());
        let mut shell = FullscreenShell::new(
            vec![ChatPane {
                name: "one".into(),
                app,
                view: FullscreenView::new(),
            }],
            None,
        );
        let rows = frame(&mut shell);
        let row = rows
            .iter()
            .position(|r| r.contains("target words"))
            .expect("the target is on screen") as u16;
        let col = rows[row as usize].find("target words").unwrap() as u16;
        let at = |kind, column| {
            Event::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };
        shell.handle_event(&at(MouseEventKind::Down(MouseButton::Left), col));
        shell.handle_event(&at(MouseEventKind::Drag(MouseButton::Left), col + 5));
        let copied = shell.handle_event(&at(MouseEventKind::Up(MouseButton::Left), col + 5));
        assert_eq!(
            copied,
            ShellAction::View(ViewAction::Copy("target".into())),
            "the copy reads the row under the pointer"
        );

        let screen = frame_at_ctx(100, 30, |ctx| shell.frame(ctx));
        let inverted: Vec<usize> = (0..screen.grid.height())
            .filter(|&y| {
                y > 0
                    && screen
                        .grid
                        .row(y)
                        .iter()
                        .any(|cell| cell.style.contains("\x1b[7m"))
            })
            .collect();
        assert_eq!(
            inverted,
            vec![row as usize],
            "the highlight is on the row under the pointer"
        );
    }

    #[test]
    fn the_plugin_buffer_reads_only_the_visible_lines_of_10k() {
        let fetched = Rc::new(Cell::new(0));
        let mut shell = FullscreenShell::new(vec![], Some(fake_buffer(10_000, fetched.clone())));
        let rows = frame(&mut shell);
        assert_eq!(fetched.get(), 29, "one fetch per visible row");
        assert!(
            rows[29].contains("log line 09999"),
            "it follows the end: {rows:#?}"
        );
        assert!(rows[0].contains("log 9972/10000"), "{:?}", rows[0]);

        shell.handle_event(&key(KeyCode::Home));
        let rows = frame(&mut shell);
        assert!(rows[1].contains("log line 00000"));
        assert!(!shell.buffer.as_ref().unwrap().scroll().follows());
    }

    #[test]
    fn a_growing_buffer_follows_its_end_until_the_reader_scrolls_up() {
        let len = Rc::new(Cell::new(100usize));
        let source = len.clone();
        let mut shell = FullscreenShell::new(
            vec![],
            Some(PluginBuffer::new(
                "tail",
                move || source.get(),
                |i| format!("row {i}"),
            )),
        );
        frame(&mut shell);
        len.set(150);
        let rows = frame(&mut shell);
        assert!(rows[29].contains("row 149"));

        shell.handle_event(&key(KeyCode::PageUp));
        let before = frame(&mut shell);
        len.set(300);
        assert_eq!(frame(&mut shell)[1], before[1], "a reader stays put");
    }
}
