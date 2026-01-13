use crate::tui::ink::node::{BoxNode, Direction, Node, PopupNode, StaticNode};
use crate::tui::ink::output::OutputBuffer;
use crate::tui::ink::render::{render_popup_standalone, render_to_string};
use crate::tui::ink::runtime::GraduationState;
use crossterm::{
    cursor::{Hide, MoveTo, MoveToColumn, Show},
    event::{
        self, Event as CtEvent, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Stdout, Write};
use std::time::Duration;

pub struct Terminal {
    stdout: Stdout,
    width: u16,
    height: u16,
    graduation: GraduationState,
    use_alternate_screen: bool,
    output: OutputBuffer,
    keyboard_enhanced: bool,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        Ok(Self {
            stdout: io::stdout(),
            width,
            height,
            graduation: GraduationState::new(),
            use_alternate_screen: false,
            output: OutputBuffer::new(width as usize, height as usize),
            keyboard_enhanced: false,
        })
    }

    pub fn with_alternate_screen(mut self, use_alt: bool) -> Self {
        self.use_alternate_screen = use_alt;
        self
    }

    pub fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;

        if execute!(
            self.stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok()
        {
            self.keyboard_enhanced = true;
            tracing::debug!("kitty keyboard enhancement enabled");
        }

        if self.use_alternate_screen {
            execute!(self.stdout, EnterAlternateScreen, Hide)?;
        } else {
            execute!(self.stdout, Hide)?;
        }
        Ok(())
    }

    pub fn exit(&mut self) -> io::Result<()> {
        if self.output.height() > 0 {
            execute!(self.stdout, MoveToColumn(0))?;
        }
        execute!(self.stdout, Show)?;
        if self.use_alternate_screen {
            execute!(self.stdout, LeaveAlternateScreen)?;
        }
        if self.keyboard_enhanced {
            let _ = execute!(self.stdout, PopKeyboardEnhancementFlags);
        }
        terminal::disable_raw_mode()?;
        writeln!(self.stdout)?;
        self.stdout.flush()
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn handle_resize(&mut self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        self.width = width;
        self.height = height;
        self.output.set_size(width as usize, height as usize);
        Ok(())
    }

    pub fn poll_event(&self, timeout: Duration) -> io::Result<Option<CtEvent>> {
        if event::poll(timeout)? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }

    pub fn render(&mut self, tree: &Node) -> io::Result<()> {
        let popup = self.find_popup(tree);

        let graduated = self.graduation.graduate(tree, self.width as usize)?;

        if !graduated.is_empty() {
            tracing::debug!(
                count = graduated.len(),
                keys = ?graduated.iter().map(|g| &g.key).collect::<Vec<_>>(),
                "graduating"
            );
            self.output.clear()?;

            for item in &graduated {
                write!(self.stdout, "{}", item.content)?;
                if item.newline {
                    write!(self.stdout, "\r\n")?;
                }
            }
            self.stdout.flush()?;

            self.output.force_redraw();
        }

        let dynamic = self.filter_graduated(tree);
        let content = render_to_string(&dynamic, self.width as usize);

        self.output.render(&content)?;

        if let Some(popup_node) = popup {
            self.render_popup_overlay(popup_node)?;
        }

        Ok(())
    }

    fn find_popup<'a>(&self, node: &'a Node) -> Option<&'a PopupNode> {
        match node {
            Node::Popup(p) => Some(p),
            Node::Box(b) => b.children.iter().find_map(|c| self.find_popup(c)),
            Node::Static(s) => s.children.iter().find_map(|c| self.find_popup(c)),
            Node::Fragment(children) => children.iter().find_map(|c| self.find_popup(c)),
            _ => None,
        }
    }

    fn render_popup_overlay(&mut self, popup: &PopupNode) -> io::Result<()> {
        if popup.items.is_empty() {
            return Ok(());
        }

        let popup_content = render_popup_standalone(popup, self.width as usize);
        let popup_lines: Vec<&str> = popup_content.split("\r\n").collect();
        let popup_height = popup_lines.len();

        let input_bar_lines = 3usize;
        let lines_up_from_cursor = input_bar_lines + popup_height;

        use crossterm::cursor::{MoveDown, MoveUp, RestorePosition, SavePosition};

        execute!(self.stdout, SavePosition)?;

        if lines_up_from_cursor > 0 {
            execute!(
                self.stdout,
                MoveUp(lines_up_from_cursor as u16),
                MoveToColumn(1)
            )?;
        }

        for (i, line) in popup_lines.iter().enumerate() {
            if i > 0 {
                execute!(self.stdout, MoveDown(1), MoveToColumn(1))?;
            }
            write!(self.stdout, "{}", line)?;
        }

        execute!(self.stdout, RestorePosition)?;
        self.stdout.flush()?;

        Ok(())
    }

    fn filter_graduated(&self, node: &Node) -> Node {
        match node {
            Node::Static(s) if self.graduation.is_graduated(&s.key) => Node::Empty,

            Node::Popup(_) => Node::Empty,

            Node::Static(s) => Node::Static(StaticNode {
                key: s.key.clone(),
                children: s
                    .children
                    .iter()
                    .map(|c| self.filter_graduated(c))
                    .collect(),
                newline: s.newline,
            }),

            Node::Box(b) => Node::Box(BoxNode {
                children: b
                    .children
                    .iter()
                    .map(|c| self.filter_graduated(c))
                    .collect(),
                direction: b.direction,
                size: b.size,
                padding: b.padding,
                border: b.border,
                style: b.style,
            }),

            Node::Fragment(children) => {
                Node::Fragment(children.iter().map(|c| self.filter_graduated(c)).collect())
            }

            other => other.clone(),
        }
    }

    pub fn show_cursor_at(&mut self, x: u16, y: u16) -> io::Result<()> {
        execute!(self.stdout, MoveTo(x, y), Show)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.exit();
    }
}
