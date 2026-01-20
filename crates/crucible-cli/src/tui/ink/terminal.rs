use crate::tui::ink::node::Node;
use crate::tui::ink::output::OutputBuffer;
use crate::tui::ink::planning::{FramePlanner, FrameSnapshot};
use crate::tui::ink::render::{render_to_string, CursorInfo};
use crossterm::{
    cursor::{self, Hide, MoveDown, MoveTo, MoveToColumn, MoveUp, Show},
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
    planner: FramePlanner,
    use_alternate_screen: bool,
    output: OutputBuffer,
    keyboard_enhanced: bool,
    last_cursor: Option<CursorInfo>,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        Ok(Self {
            stdout: io::stdout(),
            width,
            height,
            planner: FramePlanner::new(width, height),
            use_alternate_screen: false,
            output: OutputBuffer::new(width as usize, height as usize),
            keyboard_enhanced: false,
            last_cursor: None,
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
        self.planner.set_size(width, height);
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
        let snapshot = self.planner.plan(tree);
        self.apply(&snapshot)
    }

    fn apply(&mut self, snapshot: &FrameSnapshot) -> io::Result<()> {
        execute!(self.stdout, Hide)?;

        if !snapshot.stdout_delta.is_empty() {
            tracing::debug!(
                count = snapshot.plan.graduated.len(),
                keys = ?snapshot.plan.trace.graduated_keys,
                "graduating"
            );

            self.output.clear()?;
            write!(self.stdout, "{}", snapshot.stdout_delta)?;
            self.stdout.flush()?;

            self.output.force_redraw();
            self.last_cursor = None;
        }

        let did_render = self.output.render_with_cursor_restore(
            &snapshot.plan.viewport.content,
            self.last_cursor
                .as_ref()
                .map(|c| c.row_from_end)
                .unwrap_or(0),
        )?;

        if snapshot.plan.viewport.cursor.visible {
            self.last_cursor = Some(snapshot.plan.viewport.cursor);
            self.position_cursor(&snapshot.plan.viewport.cursor, did_render)?;
        } else {
            self.last_cursor = None;
        }

        Ok(())
    }

    fn position_cursor(&mut self, cursor_info: &CursorInfo, did_render: bool) -> io::Result<()> {
        let viewport_height = self.output.height();
        if viewport_height == 0 {
            return Ok(());
        }

        if did_render {
            let rows_up = cursor_info.row_from_end;
            if rows_up > 0 {
                execute!(
                    self.stdout,
                    MoveUp(rows_up),
                    MoveToColumn(cursor_info.col),
                    Show
                )?;
            } else {
                execute!(self.stdout, MoveToColumn(cursor_info.col), Show)?;
            }
        } else if let Some(last) = &self.last_cursor {
            let row_up = cursor_info.row_from_end.saturating_sub(last.row_from_end);
            let row_down = last.row_from_end.saturating_sub(cursor_info.row_from_end);

            if row_up > 0 {
                execute!(
                    self.stdout,
                    MoveUp(row_up),
                    MoveToColumn(cursor_info.col),
                    Show
                )?;
            } else if row_down > 0 {
                execute!(
                    self.stdout,
                    MoveDown(row_down),
                    MoveToColumn(cursor_info.col),
                    Show
                )?;
            } else {
                execute!(self.stdout, MoveToColumn(cursor_info.col), Show)?;
            }
        } else {
            execute!(self.stdout, MoveToColumn(cursor_info.col), Show)?;
        }

        self.stdout.flush()
    }

    pub fn force_full_redraw(&mut self) -> io::Result<()> {
        self.output.force_redraw();
        self.planner.reset_graduation();
        Ok(())
    }

    pub fn render_fullscreen(&mut self, tree: &Node) -> io::Result<()> {
        let content = render_to_string(tree, self.width as usize);
        self.output.render_fullscreen(&content)?;
        Ok(())
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
