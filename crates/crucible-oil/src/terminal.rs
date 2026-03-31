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

pub struct Terminal<W: Write = Stdout> {
    width: u16,
    height: u16,
    planner: FramePlanner,
    use_alternate_screen: bool,
    output: OutputBuffer<W>,
    keyboard_enhanced: bool,
    last_cursor: Option<CursorInfo>,
    cursor_style: SetCursorStyle,
    last_snapshot: Option<FrameSnapshot>,
    /// Transient: whether the next graduation needs a leading blank line.
    pub(crate) pending_leading_blank: bool,
}

// --- Real terminal (Stdout) only ---

impl Terminal<Stdout> {
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        Ok(Self::with_size(width, height))
    }

    pub fn with_size(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            planner: FramePlanner::new(width, height),
            use_alternate_screen: false,
            output: OutputBuffer::new(width as usize, height as usize),
            keyboard_enhanced: false,
            last_cursor: None,
            cursor_style: SetCursorStyle::SteadyBlock,
            last_snapshot: None,
            pending_leading_blank: false,
        }
    }

    pub fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let w = self.output.writer();

        if execute!(
            w,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok()
        {
            self.keyboard_enhanced = true;
            tracing::debug!("kitty keyboard enhancement enabled");
        }

        let w = self.output.writer();
        if self.use_alternate_screen {
            execute!(w, EnterAlternateScreen, Hide)?;
        } else {
            execute!(w, Hide)?;
        }
        let w = self.output.writer();
        let _ = execute!(w, self.cursor_style);
        Ok(())
    }

    pub fn exit(&mut self) -> io::Result<()> {
        let use_alt = self.use_alternate_screen;
        let kb_enhanced = self.keyboard_enhanced;

        // Move cursor to bottom of viewport so content above is preserved
        self.cleanup_viewport()?;

        let w = self.output.writer();
        let _ = execute!(w, SetCursorStyle::DefaultUserShape);
        execute!(w, Show)?;
        if use_alt {
            execute!(w, LeaveAlternateScreen)?;
        }
        if kb_enhanced {
            let _ = execute!(w, PopKeyboardEnhancementFlags);
        }
        terminal::disable_raw_mode()?;
        let w = self.output.writer();
        writeln!(w)?;
        w.flush()
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
}

// --- Headless (Vec<u8>) for testing ---

impl Terminal<Vec<u8>> {
    /// Create a headless terminal that writes to an in-memory buffer.
    /// No raw mode, no alternate screen, no keyboard enhancement.
    pub fn headless(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            planner: FramePlanner::new(width, height),
            use_alternate_screen: false,
            output: OutputBuffer::with_writer(Vec::new(), width as usize, height as usize),
            keyboard_enhanced: false,
            last_cursor: None,
            cursor_style: SetCursorStyle::SteadyBlock,
            last_snapshot: None,
            pending_leading_blank: false,
        }
    }

    /// Get the raw bytes written by the terminal (escape sequences + content).
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(self.output.writer())
    }

    /// Set the terminal dimensions (for resize testing).
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.output.set_size(width as usize, height as usize);
        self.planner.set_size(width, height);
    }
}

// --- Generic: works with any writer ---

impl<W: Write> Terminal<W> {
    pub fn with_alternate_screen(mut self, use_alt: bool) -> Self {
        self.use_alternate_screen = use_alt;
        self
    }

    pub fn cursor_style(mut self, style: SetCursorStyle) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn render(&mut self, tree: &Node, stdout_delta: &str) -> io::Result<()> {
        let snapshot = self
            .planner
            .plan_with_stdout(tree, stdout_delta.to_string());
        self.apply(&snapshot)?;
        self.last_snapshot = Some(snapshot);
        Ok(())
    }

    /// Get the last rendered FrameSnapshot (for test inspection).
    pub fn snapshot(&self) -> Option<&FrameSnapshot> {
        self.last_snapshot.as_ref()
    }

    fn apply(&mut self, snapshot: &FrameSnapshot) -> io::Result<()> {
        execute!(self.output.writer(), Hide)?;

        if !snapshot.stdout_delta.is_empty() {
            let cursor_offset = self
                .last_cursor
                .as_ref()
                .map(|c| c.row_from_end)
                .unwrap_or(0);

            // Clear viewport, write graduation content to scrollback
            self.output.clear(cursor_offset)?;
            // Cross-frame blank line (e.g., ToolGroup → AssistantResponse)
            if self.pending_leading_blank {
                write!(self.output.writer(), "\r\n")?;
                self.pending_leading_blank = false;
            }
            write!(self.output.writer(), "{}", snapshot.stdout_delta)?;
            // Graduation content needs a trailing \r\n so the viewport render
            // starts on a new line and doesn't overwrite the last graduation line.
            write!(self.output.writer(), "\r\n")?;
            self.output.writer().flush()?;

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
                self.output.writer(),
                MoveUp(move_up),
                MoveToColumn(cursor_info.col),
                Show
            )?;
        } else if move_down > 0 {
            execute!(
                self.output.writer(),
                MoveDown(move_down),
                MoveToColumn(cursor_info.col),
                Show
            )?;
        } else {
            execute!(self.output.writer(), MoveToColumn(cursor_info.col), Show)?;
        }

        self.output.writer().flush()
    }

    /// Move cursor to bottom of viewport and clear below.
    /// Used on exit to ensure subsequent println! output doesn't overlap content.
    pub fn cleanup_viewport(&mut self) -> io::Result<()> {
        if self.output.height() > 0 {
            let cursor_row_from_end = self
                .last_cursor
                .as_ref()
                .map(|c| c.row_from_end)
                .unwrap_or(0);
            if cursor_row_from_end > 0 {
                execute!(self.output.writer(), cursor::MoveDown(cursor_row_from_end))?;
            }
            execute!(
                self.output.writer(),
                cursor::MoveToColumn(0),
                terminal::Clear(terminal::ClearType::FromCursorDown)
            )?;
        }
        Ok(())
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
        execute!(self.output.writer(), MoveTo(x, y), Show)
    }
}

impl crate::runtime::FrameRenderer for Terminal<Stdout> {
    fn render_frame(&mut self, tree: &Node, graduation: Option<&crate::planning::Graduation>) {
        if let Some(grad) = graduation {
            self.pending_leading_blank = grad.leading_blank;
            let rendered = grad.render();
            let _ = self.render(tree, &rendered);
        } else {
            let _ = self.render(tree, "");
        }
    }

    fn force_full_redraw(&mut self) {
        let _ = Terminal::force_full_redraw(self);
    }

    fn size(&self) -> (u16, u16) {
        Terminal::size(self)
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

    #[test]
    fn headless_terminal_renders_to_buffer() {
        use crate::node::{col, text};

        let mut term = Terminal::headless(80, 24);
        let tree = col([text("Hello World")]);
        term.render(&tree, "").unwrap();

        let bytes = term.take_bytes();
        assert!(!bytes.is_empty());
        let output = String::from_utf8_lossy(&bytes);
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn headless_terminal_graduation_writes_to_buffer() {
        use crate::node::{col, text};

        let mut term = Terminal::headless(80, 24);
        let tree = col([text("Viewport")]);
        term.render(&tree, "Graduated content").unwrap();

        let bytes = term.take_bytes();
        let output = String::from_utf8_lossy(&bytes);
        assert!(output.contains("Graduated content"));
        assert!(output.contains("Viewport"));
    }

    #[test]
    fn cleanup_viewport_moves_cursor_below_content() {
        use crate::node::{col, text, text_input};

        let mut term = Terminal::headless(80, 24);

        // Render a tree with an input (cursor positioned above bottom)
        let tree = col([text("Line 1"), text("Line 2"), text_input("hello", 3)]);
        term.render(&tree, "").unwrap();

        // Cursor should be positioned at the input, which is above the
        // bottom of the viewport. Verify last_cursor is set.
        assert!(
            term.last_cursor.is_some(),
            "Cursor should be tracked after rendering input"
        );
        let row_from_end = term.last_cursor.as_ref().unwrap().row_from_end;

        // Drain bytes from the render
        let _ = term.take_bytes();

        // Now call cleanup_viewport
        term.cleanup_viewport().unwrap();

        let bytes = term.take_bytes();
        let output = String::from_utf8_lossy(&bytes);

        if row_from_end > 0 {
            // Should contain a MoveDown escape sequence
            // CSI <n> B = \x1b[<n>B
            assert!(
                output.contains("\x1b["),
                "cleanup_viewport should emit cursor movement.\nrow_from_end={}\nOutput bytes: {:?}",
                row_from_end,
                output
            );
        }

        // Should contain Clear(FromCursorDown) = CSI 0 J
        assert!(
            output.contains("\x1b[J") || output.contains("\x1b[0J"),
            "cleanup_viewport should clear below cursor.\nOutput bytes: {:?}",
            output
        );
    }

    #[test]
    fn cleanup_viewport_noop_when_no_content() {
        let mut term = Terminal::headless(80, 24);

        // No render — empty viewport
        term.cleanup_viewport().unwrap();

        let bytes = term.take_bytes();
        assert!(
            bytes.is_empty(),
            "cleanup_viewport should be a no-op with empty viewport"
        );
    }
}
