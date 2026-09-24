use crate::node::Node;
use crate::output::OutputBuffer;
use crate::planning::{FramePlanner, FrameSnapshot};
use crate::render::CursorInfo;
use crate::screen::{PresentStats, ScreenDiff, DISABLE_MOUSE_CAPTURE, ENABLE_MOUSE_CAPTURE};
use crossterm::{
    cursor::{self, Hide, MoveDown, MoveToColumn, MoveUp, SetCursorStyle, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event as CtEvent,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute, terminal,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Stdout, Write};
use std::time::Duration;

/// Where the terminal draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenMode {
    /// The main screen. The terminal owns the scroll and keeps the
    /// transcript in its own scrollback. This is the default.
    #[default]
    Inline,
    /// The alternate screen. The app owns every row, the scroll, selection
    /// and copy. `mouse_capture` starts mouse reporting on entry.
    Fullscreen { mouse_capture: bool },
}

pub struct Terminal<W: Write = Stdout> {
    width: u16,
    height: u16,
    planner: FramePlanner,
    output: OutputBuffer<W>,
    keyboard_enhanced: bool,
    bracketed_paste: bool,
    last_cursor: Option<CursorInfo>,
    cursor_style: SetCursorStyle,
    last_snapshot: Option<FrameSnapshot>,
    mode: ScreenMode,
    /// Rows the alternate screen shows, for the full-screen row diff.
    screen: ScreenDiff,
    mouse_captured: bool,
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
            output: OutputBuffer::new(width as usize, height as usize),
            keyboard_enhanced: false,
            bracketed_paste: false,
            last_cursor: None,
            cursor_style: SetCursorStyle::SteadyBlock,
            last_snapshot: None,
            mode: ScreenMode::Inline,
            screen: ScreenDiff::new(),
            mouse_captured: false,
        }
    }

    pub fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        // The kitty keyboard flags are a stack per screen, so the flags go
        // on after the switch, onto the alternate screen's own stack.
        if let ScreenMode::Fullscreen { mouse_capture } = self.mode {
            execute!(self.output.writer(), EnterAlternateScreen)?;
            self.set_mouse_capture(mouse_capture)?;
        }
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
        if execute!(w, EnableBracketedPaste).is_ok() {
            self.bracketed_paste = true;
            tracing::debug!("bracketed paste enabled");
        }

        let w = self.output.writer();
        execute!(w, Hide)?;
        let w = self.output.writer();
        let _ = execute!(w, self.cursor_style);
        Ok(())
    }

    pub fn exit(&mut self) -> io::Result<()> {
        let kb_enhanced = self.keyboard_enhanced;

        let fullscreen = self.is_fullscreen();
        if !fullscreen {
            // Move cursor to bottom of viewport so content above is preserved
            self.cleanup_viewport()?;
        }

        let w = self.output.writer();
        let _ = execute!(w, SetCursorStyle::DefaultUserShape);
        execute!(w, Show)?;
        if kb_enhanced {
            let _ = execute!(w, PopKeyboardEnhancementFlags);
        }
        if fullscreen {
            self.set_mouse_capture(false)?;
            execute!(self.output.writer(), LeaveAlternateScreen)?;
        }
        if self.bracketed_paste {
            let w = self.output.writer();
            let _ = execute!(w, DisableBracketedPaste);
        }
        terminal::disable_raw_mode()?;
        let w = self.output.writer();
        writeln!(w)?;
        w.flush()
    }

    pub fn handle_resize(&mut self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        let width_changed = width != self.width;
        let height_changed = height != self.height;
        self.width = width;
        self.height = height;
        self.output.set_size(width as usize, height as usize);
        self.planner.set_size(width, height);

        // The alternate screen has no scrollback to purge: the app reflows
        // its own transcript, and the next frame writes every row.
        if self.is_fullscreen() {
            self.screen.invalidate();
            return Ok(());
        }

        // A width change alters every wrap, so the transcript must be printed
        // again at the new width. No escape sequence can rewrap a row the
        // terminal already owns, so the only way to reflow is to purge the
        // scrollback and print the transcript again.
        //
        // A height-only change needs the same rebuild to keep the visible tail
        // aligned, except on a shell whose software keyboard resizes the
        // terminal. There a rebuild would replay the whole transcript on every
        // keyboard toggle.
        let purge = width_changed || (height_changed && !mobile_keyboard_shell());
        if purge {
            self.output.purge_and_reset()?;
        } else {
            self.output.clear()?;
        }
        self.output.force_redraw();
        Ok(())
    }

    /// Take a size change that has no resize event yet.
    ///
    /// In a burst of resizes the size changes before its event arrives. A
    /// full-screen frame built at the old width writes rows that the
    /// terminal wraps, and the last row scrolls the whole screen. Call this
    /// before each full-screen frame; it returns whether the size changed.
    pub fn sync_size(&mut self) -> io::Result<bool> {
        if terminal::size()? == (self.width, self.height) {
            return Ok(false);
        }
        self.handle_resize()?;
        Ok(true)
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
            output: OutputBuffer::with_writer(Vec::new(), width as usize, height as usize),
            keyboard_enhanced: false,
            bracketed_paste: false,
            last_cursor: None,
            cursor_style: SetCursorStyle::SteadyBlock,
            last_snapshot: None,
            mode: ScreenMode::Inline,
            screen: ScreenDiff::new(),
            mouse_captured: false,
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
        if self.is_fullscreen() {
            self.screen.invalidate();
            return;
        }
        // Mirror handle_resize. clear() returns io::Result but a Vec<u8>
        // writer cannot fail; in any case, a swallowed error during test-
        // only resize would only show as a missing escape sequence, which
        // assertions will surface.
        let _ = self.output.clear();
        self.output.force_redraw();
    }
}

// --- Generic: works with any writer ---

impl<W: Write> Terminal<W> {
    #[cfg(test)]
    pub fn cursor_style(mut self, style: SetCursorStyle) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Choose the screen before [`Terminal::enter`]. The default is
    /// [`ScreenMode::Inline`].
    pub fn with_mode(mut self, mode: ScreenMode) -> Self {
        self.set_mode(mode);
        self
    }

    /// Choose the screen before [`Terminal::enter`].
    pub fn set_mode(&mut self, mode: ScreenMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> ScreenMode {
        self.mode
    }

    pub fn is_fullscreen(&self) -> bool {
        matches!(self.mode, ScreenMode::Fullscreen { .. })
    }

    /// Turn mouse reporting on or off. With reporting off, the terminal's
    /// own selection works again, but the app sees no wheel and no drag.
    pub fn set_mouse_capture(&mut self, on: bool) -> io::Result<()> {
        if on == self.mouse_captured {
            return Ok(());
        }
        let sequence = if on {
            ENABLE_MOUSE_CAPTURE
        } else {
            DISABLE_MOUSE_CAPTURE
        };
        let w = self.output.writer();
        w.write_all(sequence.as_bytes())?;
        w.flush()?;
        self.mouse_captured = on;
        Ok(())
    }

    /// Write `bytes` to the terminal as they are, between frames. The copy
    /// path sends OSC 52 this way.
    pub fn write_raw(&mut self, bytes: &str) -> io::Result<()> {
        let w = self.output.writer();
        w.write_all(bytes.as_bytes())?;
        w.flush()
    }

    pub fn mouse_captured(&self) -> bool {
        self.mouse_captured
    }

    /// Write one full-screen frame as a row diff against the last one.
    pub fn present(
        &mut self,
        grid: &crate::cell_grid::CellGrid,
        cursor: Option<(u16, u16)>,
    ) -> io::Result<PresentStats> {
        self.screen.present(self.output.writer(), grid, cursor)
    }

    /// Write `lines` to the main screen, where they go into the terminal's
    /// own scrollback, then come back to the alternate screen.
    ///
    /// This is the "dump to scrollback" of the full-screen mode. The next
    /// frame writes every row, because the alternate screen may not keep its
    /// content across the switch.
    pub fn print_to_main_screen(&mut self, lines: &[String]) -> io::Result<()> {
        let fullscreen = self.is_fullscreen();
        let w = self.output.writer();
        if fullscreen {
            execute!(w, LeaveAlternateScreen)?;
        }
        for line in lines {
            write!(w, "{line}\x1b[0m\r\n")?;
        }
        if fullscreen {
            execute!(w, EnterAlternateScreen)?;
        }
        w.flush()?;
        self.screen.invalidate();
        Ok(())
    }

    /// Reserve `rows` for every frame, so a bottom-anchored overlay draws
    /// over rows that are already on screen instead of growing the frame.
    pub fn set_min_viewport_rows(&mut self, rows: u16) {
        self.output.set_min_frame_rows(rows as usize);
    }

    pub fn render(&mut self, tree: &Node, stdout_delta: &str) -> io::Result<()> {
        // Legacy API: accepts a pre-rendered stdout string. Used by tests.
        let mut snapshot = self.planner.plan_frame(tree, None);
        snapshot.stdout_delta = stdout_delta.to_string();
        self.apply(&snapshot)?;
        self.last_snapshot = Some(snapshot);
        Ok(())
    }

    /// Get the last rendered FrameSnapshot (for test inspection).
    pub fn snapshot(&self) -> Option<&FrameSnapshot> {
        self.last_snapshot.as_ref()
    }

    fn apply(&mut self, snapshot: &FrameSnapshot) -> io::Result<()> {
        use crate::output::{BEGIN_SYNCHRONIZED_UPDATE, END_SYNCHRONIZED_UPDATE};

        execute!(self.output.writer(), Hide)?;

        // Begin synchronized update — wraps everything (clear, graduation,
        // viewport render) in a single atomic block. This prevents the terminal
        // from rendering intermediate states where old spinner content is visible.
        write!(self.output.writer(), "{}", BEGIN_SYNCHRONIZED_UPDATE)?;

        // Normalize cursor to viewport bottom before any clearing/rendering.
        // This eliminates cursor_offset_from_end as a variable — clear() and
        // render_with_overlays() always know the cursor is at the bottom.
        if let Some(cursor) = &self.last_cursor {
            if cursor.row_from_end > 0 {
                execute!(self.output.writer(), MoveDown(cursor.row_from_end))?;
            }
        }

        if !snapshot.stdout_delta.is_empty() {
            tracing::debug!(
                graduation_len = snapshot.stdout_delta.len(),
                viewport_rows = self.output.height(),
                has_spinner = snapshot
                    .stdout_delta
                    .chars()
                    .any(|c| "◐◓◑◒⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".contains(c)),
                "[apply] graduation write"
            );
            // Clear viewport, write graduation content to scrollback.
            // Cursor is at viewport bottom, so clear() only needs previous_visual_rows.
            self.output.clear()?;
            write!(self.output.writer(), "{}", snapshot.stdout_delta)?;
            // Line terminator after graduation content — ensures the viewport
            // render starts on a fresh line. This is NOT spacing; inter-batch
            // gaps are encoded as margin-top in the graduated node tree.
            write!(self.output.writer(), "\r\n")?;
            self.output.writer().flush()?;

            self.output.force_redraw();
            self.last_cursor = None;
        }

        self.output
            .render_with_overlays(&snapshot.plan.viewport.content, &snapshot.plan.overlays)?;

        // End synchronized update — terminal can now process all writes atomically.
        write!(self.output.writer(), "{}", END_SYNCHRONIZED_UPDATE)?;
        self.output.writer().flush()?;

        if snapshot.plan.viewport.cursor.visible {
            self.last_cursor = Some(snapshot.plan.viewport.cursor);
            self.position_cursor(&snapshot.plan.viewport.cursor)?;
        } else {
            self.last_cursor = None;
        }

        Ok(())
    }

    fn position_cursor(&mut self, cursor_info: &CursorInfo) -> io::Result<()> {
        if self.output.height() == 0 {
            return Ok(());
        }

        // Cursor is always at viewport bottom (invariant from apply()).
        // Move up by row_from_end to reach the input position.
        let move_up = cursor_info.row_from_end;

        if move_up > 0 {
            execute!(
                self.output.writer(),
                MoveUp(move_up),
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
        self.screen.invalidate();
        Ok(())
    }

    /// Switch to the alternate screen again after something else left it,
    /// such as the shell modal. The next frame writes every row.
    pub fn reenter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(self.output.writer(), EnterAlternateScreen, Hide)?;
        self.screen.invalidate();
        Ok(())
    }

    pub fn render_fullscreen(&mut self, tree: &Node) -> io::Result<()> {
        let result = crate::render::render_tree(tree, self.width, self.height);
        self.output.render_fullscreen(&result.content)?;
        Ok(())
    }
}

impl<W: Write> crate::runtime::FrameRenderer for Terminal<W> {
    fn set_min_viewport_rows(&mut self, rows: u16) {
        Terminal::set_min_viewport_rows(self, rows);
    }

    fn render_frame(&mut self, tree: &Node, graduation: Option<&crate::planning::Graduation>) {
        let snapshot = self.planner.plan_frame(tree, graduation.cloned());
        let _ = self.apply(&snapshot);
        self.last_snapshot = Some(snapshot);
    }

    fn force_full_redraw(&mut self) {
        let _ = Terminal::force_full_redraw(self);
    }

    fn size(&self) -> (u16, u16) {
        Terminal::size(self)
    }
}

/// Whether this shell resizes the terminal when a software keyboard appears.
///
/// A rebuild on every keyboard toggle is unusable, so these shells keep the
/// differential repaint for a height-only change.
///
/// Termux announces itself with `TERMUX_VERSION`. iSH sets no marker variable
/// at all — it only hardcodes `TERM=xterm-256color` — so it is identified by
/// its kernel string instead: the release is hardcoded to `4.20.69-ish` and
/// `/proc/version` repeats it next to the literal build tag `SUPER AWESOME`.
/// iSH does resize and does raise `SIGWINCH` when the iOS keyboard appears.
fn mobile_keyboard_shell() -> bool {
    static DETECTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DETECTED.get_or_init(|| {
        // An explicit override wins in both directions.
        match std::env::var("CRUCIBLE_TUI_MOBILE").as_deref() {
            Ok("1") | Ok("true") | Ok("yes") => return true,
            Ok("0") | Ok("false") | Ok("no") => return false,
            _ => {}
        }
        if std::env::var_os("TERMUX_VERSION").is_some() {
            return true;
        }
        std::fs::read_to_string("/proc/version")
            .map(|version| is_ish_kernel(&version))
            .unwrap_or(false)
    })
}

/// Whether a `/proc/version` string came from iSH.
///
/// Both markers are hardcoded in the iSH kernel, so an exact match is safe.
fn is_ish_kernel(proc_version: &str) -> bool {
    proc_version.contains("-ish ") || proc_version.contains("SUPER AWESOME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ish_kernel_string_is_recognised() {
        // The literal string iSH reports; both markers are hardcoded upstream.
        assert!(is_ish_kernel(
            "Linux version 4.20.69-ish SUPER AWESOME Feb 14 2026 12:00:00"
        ));
    }

    #[test]
    fn ordinary_kernel_string_is_not_ish() {
        assert!(!is_ish_kernel(
            "Linux version 7.1.4-102.fc43.x86_64 (mockbuild@) (gcc 15.0.1)"
        ));
        assert!(!is_ish_kernel(
            "Linux version 6.6.0-generic #1 SMP PREEMPT_DYNAMIC"
        ));
    }
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
    fn set_size_scrubs_old_viewport_and_invalidates_snapshot() {
        use crate::node::{col, text};

        // Render once at width 80 to populate the previous-frame snapshot.
        let mut term = Terminal::headless(80, 24);
        let tree = col([text("Hello World")]);
        term.render(&tree, "").unwrap();
        let _ = term.take_bytes();
        assert!(term.output.height() > 0, "snapshot recorded");

        // Resize to width 40. set_size must (a) emit a clearing escape
        // sequence so the old content doesn't survive under the new wrap,
        // and (b) drop the stale-width snapshot so the next diff doesn't
        // compare new wrapping against old.
        term.set_size(40, 24);
        let bytes = term.take_bytes();
        let output = String::from_utf8_lossy(&bytes);
        assert!(
            output.contains("\x1b[J"),
            "set_size must emit ClearFromCursorDown to scrub old viewport (got: {:?})",
            output
        );
        assert_eq!(
            term.output.height(),
            0,
            "set_size drops the previous-frame snapshot at the old width"
        );
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

    fn fullscreen_headless(width: u16, height: u16) -> Terminal<Vec<u8>> {
        Terminal::headless(width, height).with_mode(ScreenMode::Fullscreen {
            mouse_capture: true,
        })
    }

    #[test]
    fn a_fullscreen_resize_neither_purges_nor_clears() {
        // The alternate screen has no scrollback, and the next frame writes
        // every row, so a clear would only add a blank flash.
        let mut term = fullscreen_headless(20, 4);
        let mut grid = crate::cell_grid::CellGrid::new(20, 4);
        grid.blit_line("row", 0, 0);
        term.present(&grid, None).unwrap();
        let _ = term.take_bytes();

        term.set_size(10, 4);
        assert!(term.take_bytes().is_empty(), "a resize writes nothing");

        let grid = crate::cell_grid::CellGrid::new(10, 4);
        let stats = term.present(&grid, None).unwrap();
        assert_eq!(stats.rows_written, 4, "the frame after a resize writes every row");
    }

    #[test]
    fn print_to_main_screen_leaves_and_reenters_the_alternate_screen() {
        let mut term = fullscreen_headless(20, 4);
        let mut parser = vt100::Parser::new(4, 20, 100);
        parser.process(b"\x1b[?1049h");
        let mut grid = crate::cell_grid::CellGrid::new(20, 4);
        grid.blit_line("frame", 0, 0);
        term.present(&grid, None).unwrap();

        term.print_to_main_screen(&["kept one".into(), "kept two".into()])
            .unwrap();
        parser.process(&term.take_bytes());

        assert!(parser.screen().alternate_screen(), "back on the alternate screen");
        parser.process(b"\x1b[?1049l");
        let main = parser.screen().contents();
        assert!(main.contains("kept one") && main.contains("kept two"), "{main:?}");

        let stats = term.present(&grid, None).unwrap();
        assert_eq!(stats.rows_written, 4, "the next frame repaints the whole screen");
    }

    #[test]
    fn mouse_capture_toggles_once_per_change() {
        let mut term = fullscreen_headless(20, 4);
        term.set_mouse_capture(true).unwrap();
        term.set_mouse_capture(true).unwrap();
        assert_eq!(
            String::from_utf8(term.take_bytes()).unwrap(),
            crate::screen::ENABLE_MOUSE_CAPTURE
        );
        term.set_mouse_capture(false).unwrap();
        assert_eq!(
            String::from_utf8(term.take_bytes()).unwrap(),
            crate::screen::DISABLE_MOUSE_CAPTURE
        );
        assert!(!term.mouse_captured());
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
