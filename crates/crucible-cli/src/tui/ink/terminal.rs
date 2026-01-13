use crate::tui::ink::layout::calculate_layout;
use crate::tui::ink::node::{BoxNode, Direction, Node, StaticNode};
use crate::tui::ink::render::render_to_string;
use crate::tui::ink::runtime::GraduationState;
use crossterm::{
    cursor::{Hide, MoveTo, MoveToColumn, MoveUp, Show},
    event::{
        self, Event as CtEvent, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Stdout, Write};
use std::time::Duration;

pub struct Terminal {
    stdout: Stdout,
    width: u16,
    height: u16,
    graduation: GraduationState,
    use_alternate_screen: bool,
    dynamic_lines: u16,
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
            dynamic_lines: 0,
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
        if self.dynamic_lines > 0 {
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
        self.clear_dynamic_area()?;

        let graduated = self.graduation.graduate(tree, self.width as usize)?;
        for item in &graduated {
            write!(self.stdout, "{}", item.content)?;
            if item.newline {
                write!(self.stdout, "\r\n")?;
            }
        }

        let dynamic = self.filter_graduated(tree);
        let content = render_to_string(&dynamic, self.width as usize);

        if content.is_empty() {
            self.stdout.flush()?;
            self.dynamic_lines = 0;
            return Ok(());
        }

        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len() as u16;

        for (i, line) in lines.iter().enumerate() {
            write!(self.stdout, "{}", line)?;
            if i < lines.len() - 1 {
                write!(self.stdout, "\r\n")?;
            }
        }

        self.stdout.flush()?;
        self.dynamic_lines = line_count.saturating_sub(1);

        Ok(())
    }

    fn clear_dynamic_area(&mut self) -> io::Result<()> {
        if self.dynamic_lines > 0 {
            execute!(
                self.stdout,
                MoveUp(self.dynamic_lines),
                MoveToColumn(0),
                Clear(ClearType::FromCursorDown)
            )?;
            self.dynamic_lines = 0;
        }
        Ok(())
    }

    fn filter_graduated(&self, node: &Node) -> Node {
        match node {
            Node::Static(s) if self.graduation.is_graduated(&s.key) => Node::Empty,

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
