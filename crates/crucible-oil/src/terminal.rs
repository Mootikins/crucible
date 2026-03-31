use crate::node::Node;
use crate::output::OutputBuffer;
use crate::planning::{FramePlanner, FrameSnapshot};
use crate::render::CursorInfo;
#[allow(unused_imports)] // WIP: self not yet used in cursor and event modules
use crossterm::{
    cursor::{self, Hide, MoveDown, MoveTo, MoveToColumn, MoveUp, SetCursorStyle, Show},
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
    cursor_style: SetCursorStyle,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        Ok(Self::with_size(width, height))
    }

    pub fn with_size(width: u16, height: u16) -> Self {
        Self {
            stdout: io::stdout(),
            width,
            height,
            planner: FramePlanner::new(width, height),
            use_alternate_screen: false,
            output: OutputBuffer::new(width as usize, height as usize),
            keyboard_enhanced: false,
            last_cursor: None,
            cursor_style: SetCursorStyle::SteadyBlock,
        }
    }

    pub fn with_alternate_screen(mut self, use_alt: bool) -> Self {
        self.use_alternate_screen = use_alt;
        self
    }

    pub fn cursor_style(mut self, style: SetCursorStyle) -> Self {
        self.cursor_style = style;
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
        let _ = execute!(self.stdout, self.cursor_style);
        Ok(())
    }

    pub fn exit(&mut self) -> io::Result<()> {
        if self.output.height() > 0 {
            execute!(self.stdout, MoveToColumn(0))?;
        }
        let _ = execute!(self.stdout, SetCursorStyle::DefaultUserShape);
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

    pub fn render(&mut self, tree: &Node, stdout_delta: &str) -> io::Result<()> {
        let snapshot = self
            .planner
            .plan_with_stdout(tree, stdout_delta.to_string());
        self.apply(&snapshot)
    }

    fn apply(&mut self, snapshot: &FrameSnapshot) -> io::Result<()> {
        execute!(self.stdout, Hide)?;

        if !snapshot.stdout_delta.is_empty() {
            let delta_lines = snapshot.stdout_delta.lines().count();
            let delta_ends_newline = snapshot.stdout_delta.ends_with('\n')
                || snapshot.stdout_delta.ends_with("\r\n");
            let cursor_offset = self
                .last_cursor
                .as_ref()
                .map(|c| c.row_from_end)
                .unwrap_or(0);
            let delta_first_stripped = crate::ansi::strip_ansi(
                snapshot.stdout_delta.lines().next().unwrap_or(""),
            );
            let delta_first_10: String = delta_first_stripped.chars().take(40).collect();
            tracing::debug!(
                delta_lines,
                delta_ends_newline,
                delta_first_10,
                cursor_offset,
                prev_visual_rows = self.output.height(),
                delta_bytes = snapshot.stdout_delta.len(),
                delta_starts_with_newline = snapshot.stdout_delta.starts_with("\r\n"),
                "[graduation] Writing stdout_delta"
            );
            // Dump first 80 chars of raw delta for debugging
            let raw_start: String = snapshot.stdout_delta.chars().take(80).map(|c| {
                match c {
                    '\r' => '⏎',
                    '\n' => '↵',
                    '\x1b' => '␛',
                    c if c.is_control() => '·',
                    c => c,
                }
            }).collect();
            tracing::debug!(raw_start, "[graduation] Raw delta start");

            self.output.clear(cursor_offset)?;

            write!(self.stdout, "{}", snapshot.stdout_delta)?;
            self.stdout.flush()?;

            self.output.force_redraw();
            self.last_cursor = None;
        }

        let did_render = self.output.render_with_overlays(
            &snapshot.plan.viewport.content,
            self.last_cursor
                .as_ref()
                .map(|c| c.row_from_end)
                .unwrap_or(0),
            &snapshot.plan.overlays,
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
        if self.output.height() == 0 {
            return Ok(());
        }

        let (move_up, move_down) = if did_render {
            (cursor_info.row_from_end, 0)
        } else if let Some(last) = &self.last_cursor {
            (
                cursor_info.row_from_end.saturating_sub(last.row_from_end),
                last.row_from_end.saturating_sub(cursor_info.row_from_end),
            )
        } else {
            (0, 0)
        };

        if move_up > 0 {
            execute!(
                self.stdout,
                MoveUp(move_up),
                MoveToColumn(cursor_info.col),
                Show
            )?;
        } else if move_down > 0 {
            execute!(
                self.stdout,
                MoveDown(move_down),
                MoveToColumn(cursor_info.col),
                Show
            )?;
        } else {
            execute!(self.stdout, MoveToColumn(cursor_info.col), Show)?;
        }

        self.stdout.flush()
    }

    pub fn force_full_redraw(&mut self) -> io::Result<()> {
        self.output.force_redraw();
        Ok(())
    }

    pub fn render_fullscreen(&mut self, tree: &Node) -> io::Result<()> {
        let layout = crate::layout::build_layout_tree(tree, self.width, self.height);
        let (content, _cursor) = crate::layout::render_layout_tree(&layout);
        self.output.render_fullscreen(&content)?;
        Ok(())
    }

    pub fn show_cursor_at(&mut self, x: u16, y: u16) -> io::Result<()> {
        execute!(self.stdout, MoveTo(x, y), Show)
    }
}

impl crate::runtime::FrameRenderer for Terminal {
    fn render_frame(&mut self, tree: &Node, stdout_delta: &str) {
        let _ = self.render(tree, stdout_delta);
    }

    fn force_full_redraw(&mut self) {
        let _ = self.force_full_redraw();
    }

    fn size(&self) -> (u16, u16) {
        self.size()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.exit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::cursor::SetCursorStyle;

    #[test]
    fn terminal_default_cursor_style_is_steady_block() {
        let term = Terminal::with_size(80, 24);
        assert_eq!(term.cursor_style, SetCursorStyle::SteadyBlock);
    }

    #[test]
    fn terminal_has_cursor_style_builder() {
        let term = Terminal::with_size(80, 24).cursor_style(SetCursorStyle::BlinkingBar);
        assert_eq!(term.cursor_style, SetCursorStyle::BlinkingBar);
    }
}
