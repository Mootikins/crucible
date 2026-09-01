use crate::ansi::visual_rows;
use crate::overlay::{composite_overlays, Overlay};
use crate::planning::RenderedOverlay;
use crossterm::{cursor, execute, terminal};
use std::io::{self, Stdout, Write};

pub(crate) const BEGIN_SYNCHRONIZED_UPDATE: &str = "\x1b[?2026h";
pub(crate) const END_SYNCHRONIZED_UPDATE: &str = "\x1b[?2026l";

/// Snapshot of the last frame written, used for incremental diffing on the
/// next frame. `lines`, `line_visual_rows` and `total_visual_rows` always
/// describe the same frame — collapsing them into one option makes
/// inconsistent state unrepresentable.
///
/// `lines` holds the whole transcript, not the visible tail. The terminal owns
/// every row that scrolled above the screen, so a row above the visible window
/// can no longer be addressed. `repaint_start` enforces that bound.
#[derive(Default)]
struct PreviousFrame {
    lines: Vec<String>,
    line_visual_rows: Vec<usize>,
    total_visual_rows: usize,
}

impl PreviousFrame {
    /// Visual rows occupied by `lines[index..]`.
    fn rows_from(&self, index: usize) -> usize {
        self.line_visual_rows[index.min(self.line_visual_rows.len())..]
            .iter()
            .sum()
    }
}

pub struct OutputBuffer<W: Write = Stdout> {
    stdout: W,
    previous: Option<PreviousFrame>,
    terminal_width: usize,
    terminal_height: usize,
    force_next_redraw: bool,
    /// Most transcript rows kept for a reprint after a resize.
    ///
    /// A resize purges the scrollback and prints the transcript again, so this
    /// bounds that reprint. Rows beyond the cap are dropped from the model:
    /// they already went to the terminal, they sit above the repaintable
    /// window, and reprinting more rows than the terminal retains is wasted
    /// work. Values follow the documented scrollback depth of each terminal.
    max_transcript_rows: usize,
    /// Rows the frame occupies even when its content is shorter.
    ///
    /// A bottom-anchored overlay draws over the rows above the prompt. If the
    /// frame is shorter than the overlay, compositing has to grow it, and the
    /// prompt jumps down the screen each time the overlay opens. The caller
    /// reserves the tallest overlay here, so the frame keeps one height and
    /// the overlay draws over rows that are already there.
    min_frame_rows: usize,
}

impl Default for OutputBuffer<Stdout> {
    fn default() -> Self {
        let (width, height) = terminal::size()
            .map(|(w, h)| (w as usize, h as usize))
            .unwrap_or((80, 24));
        Self::new(width, height)
    }
}

impl OutputBuffer<Stdout> {
    pub fn new(width: usize, height: usize) -> Self {
        Self::with_writer(io::stdout(), width, height)
    }
}

impl<W: Write> OutputBuffer<W> {
    pub fn with_writer(writer: W, width: usize, height: usize) -> Self {
        Self {
            stdout: writer,
            previous: None,
            terminal_width: width,
            terminal_height: height,
            force_next_redraw: false,
            max_transcript_rows: detect_max_transcript_rows(),
            min_frame_rows: 0,
        }
    }

    /// Override the reprint cap. Tests use this to reach the bound cheaply.
    pub fn set_max_transcript_rows(&mut self, rows: usize) {
        self.max_transcript_rows = rows;
    }

    /// Reserve `rows` for the frame. See [`Self::min_frame_rows`].
    pub fn set_min_frame_rows(&mut self, rows: usize) {
        self.min_frame_rows = rows;
    }

    /// Get a mutable reference to the underlying writer.
    pub fn writer(&mut self) -> &mut W {
        &mut self.stdout
    }

    pub fn set_size(&mut self, width: usize, height: usize) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    /// Render the whole transcript, repainting only what the terminal can
    /// still address.
    ///
    /// `content` is the full transcript, not a visible tail. Rows that
    /// scrolled above the screen belong to the terminal and cannot be
    /// rewritten, so a change above the visible window is skipped instead of
    /// painted at the wrong place.
    pub fn render_with_overlays(
        &mut self,
        content: &str,
        overlays: &[RenderedOverlay],
    ) -> io::Result<bool> {
        let mut all_lines: Vec<String> = collapse_blank_lines(content);
        // The reserve goes on before the overlays, so an overlay finds the
        // rows it needs and composites over them instead of growing the frame.
        self.pad_to_min_frame_rows(&mut all_lines);
        // Overlays anchor to the screen, not to the transcript, so composite
        // them onto the tail the terminal actually shows.
        self.composite_visible_overlays(&mut all_lines, overlays);

        let mut line_visual_rows: Vec<usize> = all_lines
            .iter()
            .map(|line| visual_rows(line, self.terminal_width))
            .collect();
        let mut total_visual_rows: usize = line_visual_rows.iter().sum();

        // Never drop a row the screen still shows.
        let cap = self.max_transcript_rows.max(self.terminal_height);
        let mut dropped = 0usize;
        while total_visual_rows > cap && dropped + 1 < line_visual_rows.len() {
            total_visual_rows -= line_visual_rows[dropped];
            dropped += 1;
        }
        if dropped > 0 {
            all_lines.drain(..dropped);
            line_visual_rows.drain(..dropped);
        }

        let next = PreviousFrame {
            lines: all_lines,
            line_visual_rows,
            total_visual_rows,
        };

        let force = std::mem::replace(&mut self.force_next_redraw, false);
        let Some(prev) = self.previous.take().filter(|_| !force) else {
            self.paint_from(&next, 0)?;
            self.previous = Some(next);
            return Ok(true);
        };

        if prev.lines == next.lines {
            self.previous = Some(prev);
            return Ok(false);
        }

        let common = prev.lines.len().min(next.lines.len());
        let mut first_diff = (0..common)
            .find(|&i| prev.lines[i] != next.lines[i])
            .unwrap_or(common);

        // Bound the repaint to rows still on screen. A larger move would reach
        // the top of the screen and paint over the wrong rows.
        let floor = Self::first_addressable_line(&prev, self.terminal_height);
        if first_diff < floor {
            tracing::debug!(
                first_diff,
                floor,
                "change sits above the visible window; the terminal keeps those rows"
            );
            first_diff = floor;
        }

        let previous_rows = prev.rows_from(first_diff);
        let rows_up = previous_rows.saturating_sub(1);
        if rows_up > 0 {
            execute!(
                self.stdout,
                cursor::MoveUp(rows_up as u16),
                cursor::MoveToColumn(0)
            )?;
        } else {
            execute!(self.stdout, cursor::MoveToColumn(0))?;
        }

        let shrank = next.rows_from(first_diff) < previous_rows;
        self.paint_from(&next, first_diff)?;
        if shrank {
            execute!(
                self.stdout,
                terminal::Clear(terminal::ClearType::FromCursorDown)
            )?;
            self.stdout.flush()?;
        }

        self.previous = Some(next);
        Ok(true)
    }

    /// Write `frame.lines[start..]`, clearing each row before it is written.
    ///
    /// The caller puts the cursor on the first row of `start`. On return the
    /// cursor sits on the last row of the last line.
    fn paint_from(&mut self, frame: &PreviousFrame, start: usize) -> io::Result<()> {
        let last = frame.lines.len().saturating_sub(1);
        for (i, line) in frame.lines.iter().enumerate().skip(start) {
            let rows = frame.line_visual_rows[i].max(1);
            execute!(self.stdout, cursor::MoveToColumn(0))?;
            // A wrapped line owns several rows, and Clear(CurrentLine) reaches
            // only the cursor's own row.
            for row in 0..rows {
                execute!(
                    self.stdout,
                    terminal::Clear(terminal::ClearType::CurrentLine)
                )?;
                if row < rows - 1 {
                    execute!(self.stdout, cursor::MoveDown(1))?;
                }
            }
            if rows > 1 {
                execute!(
                    self.stdout,
                    cursor::MoveUp((rows - 1) as u16),
                    cursor::MoveToColumn(0)
                )?;
            }
            write!(self.stdout, "{}", line)?;
            if i < last {
                write!(self.stdout, "\r\n")?;
            }
        }
        self.stdout.flush()
    }

    /// Index of the first line whose rows are still on screen.
    fn first_addressable_line(frame: &PreviousFrame, terminal_height: usize) -> usize {
        let mut rows = 0usize;
        for (index, line_rows) in frame.line_visual_rows.iter().enumerate().rev() {
            rows += line_rows;
            if rows >= terminal_height {
                return index;
            }
        }
        0
    }

    /// Prepend blank rows until the frame reaches [`Self::min_frame_rows`].
    ///
    /// The reserve never exceeds the screen: rows beyond it would scroll the
    /// transcript away to hold space the overlay cannot use.
    fn pad_to_min_frame_rows(&self, lines: &mut Vec<String>) {
        let min = self.min_frame_rows.min(self.terminal_height);
        let rows: usize = lines
            .iter()
            .map(|line| visual_rows(line, self.terminal_width))
            .sum();
        if rows >= min {
            return;
        }
        let mut padded = vec![String::new(); min - rows];
        padded.append(lines);
        *lines = padded;
    }

    /// Composite overlays onto the tail the terminal shows, so scrolled rows
    /// stay untouched.
    fn composite_visible_overlays(&self, lines: &mut Vec<String>, overlays: &[RenderedOverlay]) {
        if overlays.is_empty() {
            return;
        }
        let overlay_refs: Vec<Overlay> = overlays
            .iter()
            .map(|o| Overlay {
                lines: o.lines.clone(),
                anchor: o.anchor,
            })
            .collect();
        let split = lines.len().saturating_sub(self.terminal_height);
        let tail = lines.split_off(split);
        lines.extend(composite_overlays(
            &tail,
            &overlay_refs,
            self.terminal_width,
        ));
    }

    /// Clear the viewport from terminal.
    ///
    /// The caller must ensure the cursor is at the bottom of the viewport
    /// before calling this. This simplifies the math: we only need
    /// `previous_visual_rows` to compute how far up to move.
    pub fn clear(&mut self) -> io::Result<()> {
        // Never move above the screen top: rows beyond it belong to the terminal.
        let prev_visual_rows = self
            .previous
            .as_ref()
            .map(|p| p.total_visual_rows.min(self.terminal_height))
            .unwrap_or(0);
        tracing::debug!(
            previous_visual_rows = prev_visual_rows,
            terminal_height = self.terminal_height,
            "[clear] clearing viewport"
        );
        if prev_visual_rows > 0 {
            let move_up_amount = (prev_visual_rows as u16).saturating_sub(1);
            if move_up_amount > 0 {
                execute!(
                    self.stdout,
                    cursor::MoveUp(move_up_amount),
                    cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::FromCursorDown),
                )?;
            } else {
                execute!(
                    self.stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::FromCursorDown),
                )?;
            }
            self.previous = None;
        }
        Ok(())
    }

    /// Clear the screen and purge the scrollback, then force a full repaint.
    ///
    /// This is the resize path. The terminal owns every row it already
    /// printed and no escape sequence can rewrap them, so a reflow has to
    /// destroy the buffer and print the transcript again at the new width.
    /// Shell output from before the TUI started is destroyed with it.
    pub fn purge_and_reset(&mut self) -> io::Result<()> {
        // Reset the scroll region and style, home the cursor, clear the
        // screen, then purge the scrollback (ED 3).
        write!(self.stdout, "\x1b[r\x1b[0m\x1b[H\x1b[2J\x1b[3J\x1b[H")?;
        self.stdout.flush()?;
        self.previous = None;
        self.force_next_redraw = true;
        Ok(())
    }

    pub fn height(&self) -> usize {
        self.previous
            .as_ref()
            .map(|p| p.total_visual_rows)
            .unwrap_or(0)
    }

    pub fn force_redraw(&mut self) {
        self.previous = None;
        self.force_next_redraw = true;
    }

    pub fn render_fullscreen(&mut self, content: &str) -> io::Result<()> {
        write!(self.stdout, "{}", BEGIN_SYNCHRONIZED_UPDATE)?;

        execute!(
            self.stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        )?;

        write!(self.stdout, "{}", content)?;

        write!(self.stdout, "{}", END_SYNCHRONIZED_UPDATE)?;
        self.stdout.flush()?;

        self.previous = None;

        Ok(())
    }
}

/// Reprint cap for the terminal this process is attached to.
///
/// Each value mirrors that terminal's documented scrollback default, so a
/// reprint never emits more rows than the terminal would keep. The values come
/// from Codex's `resize_reflow_cap.rs`, which solves the same problem.
fn detect_max_transcript_rows() -> usize {
    const VSCODE: usize = 1_000;
    const WEZTERM: usize = 3_500;
    const WINDOWS_TERMINAL: usize = 9_001;
    const ALACRITTY: usize = 10_000;
    const FALLBACK: usize = 1_000;

    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if term_program.eq_ignore_ascii_case("vscode") {
        return VSCODE;
    }
    if term_program.eq_ignore_ascii_case("wezterm") || std::env::var_os("WEZTERM_PANE").is_some() {
        return WEZTERM;
    }
    if std::env::var_os("WT_SESSION").is_some() {
        return WINDOWS_TERMINAL;
    }
    if std::env::var_os("ALACRITTY_WINDOW_ID").is_some()
        || std::env::var("TERM")
            .unwrap_or_default()
            .contains("alacritty")
    {
        return ALACRITTY;
    }
    FALLBACK
}

fn collapse_blank_lines(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut prev_blank = false;

    for line in content.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue;
        }
        result.push(line.to_string());
        prev_blank = is_blank;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_buffer_creation() {
        let buffer = OutputBuffer::new(80, 24);
        assert_eq!(buffer.height(), 0);
        assert!(buffer.previous.is_none());
    }

    #[test]
    fn transcript_is_capped_to_the_reprint_bound() {
        // A resize reprints the retained rows, so the model must not grow past
        // what the terminal would keep.
        let mut buffer = OutputBuffer::with_writer(Vec::new(), 80, 10);
        buffer.set_max_transcript_rows(20);

        let content = (0..100)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        buffer.render_with_overlays(&content, &[]).unwrap();

        let retained = buffer.previous.as_ref().expect("a frame was recorded");
        assert!(
            retained.total_visual_rows <= 20,
            "retained {} rows, cap is 20",
            retained.total_visual_rows
        );
        assert_eq!(
            retained.lines.last().map(String::as_str),
            Some("line99"),
            "the cap must drop the oldest rows, not the newest"
        );
    }

    #[test]
    fn the_cap_never_drops_a_row_the_screen_shows() {
        // A cap below the screen height would blank rows the user can see.
        let mut buffer = OutputBuffer::with_writer(Vec::new(), 80, 24);
        buffer.set_max_transcript_rows(2);

        let content = (0..50)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        buffer.render_with_overlays(&content, &[]).unwrap();

        let retained = buffer.previous.as_ref().expect("a frame was recorded");
        assert!(
            retained.total_visual_rows >= 24,
            "kept {} rows, screen holds 24",
            retained.total_visual_rows
        );
    }

    #[test]
    fn a_frame_shorter_than_the_reserve_is_padded_up_to_it() {
        let mut buffer = OutputBuffer::with_writer(Vec::new(), 80, 24);
        buffer.set_min_frame_rows(10);

        buffer.render_with_overlays("a\nb\nc", &[]).unwrap();

        let frame = buffer.previous.as_ref().expect("a frame was recorded");
        assert_eq!(frame.total_visual_rows, 10, "the reserve was not filled");
        assert_eq!(
            frame
                .lines
                .iter()
                .rev()
                .take(3)
                .rev()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"],
            "the content must stay at the bottom of the reserve"
        );
        assert!(
            frame.lines[..7].iter().all(|line| line.trim().is_empty()),
            "the reserve is blank rows: {:?}",
            frame.lines
        );
    }

    /// The reason the reserve exists: a bottom-anchored overlay draws over the
    /// rows above the prompt instead of pushing the prompt down the screen.
    #[test]
    fn an_overlay_taller_than_the_content_does_not_grow_a_reserved_frame() {
        let overlay = [RenderedOverlay {
            lines: (0..6).map(|i| format!("item{i}")).collect(),
            anchor: crate::overlay::OverlayAnchor::FromBottom(2),
        }];

        let mut bare = OutputBuffer::with_writer(Vec::new(), 80, 24);
        bare.render_with_overlays("a\nb\nc", &overlay).unwrap();
        let grown = bare.previous.as_ref().expect("a frame").total_visual_rows;

        let mut reserved = OutputBuffer::with_writer(Vec::new(), 80, 24);
        reserved.set_min_frame_rows(10);
        reserved.render_with_overlays("a\nb\nc", &[]).unwrap();
        let closed = reserved
            .previous
            .as_ref()
            .expect("a frame")
            .total_visual_rows;
        reserved.render_with_overlays("a\nb\nc", &overlay).unwrap();
        let open = reserved.previous.as_ref().expect("a frame");

        assert!(grown > 3, "without a reserve the overlay grows the frame");
        assert_eq!(
            open.total_visual_rows, closed,
            "the frame must keep one height whether the overlay is open or not"
        );
        assert!(
            open.lines.iter().any(|line| line.contains("item5")),
            "the overlay must still draw: {:?}",
            open.lines
        );
    }

    #[test]
    fn the_reserve_never_exceeds_the_screen() {
        let mut buffer = OutputBuffer::with_writer(Vec::new(), 80, 6);
        buffer.set_min_frame_rows(40);

        buffer.render_with_overlays("a", &[]).unwrap();

        let frame = buffer.previous.as_ref().expect("a frame was recorded");
        assert_eq!(
            frame.total_visual_rows, 6,
            "reserving more rows than the screen holds would scroll the \
             transcript away for space the overlay cannot use"
        );
    }

    #[test]
    fn collapse_blank_lines_preserves_single_blanks() {
        let content = "line1\n\nline2\n\nline3";
        let result = collapse_blank_lines(content);
        assert_eq!(result, vec!["line1", "", "line2", "", "line3"]);
    }

    #[test]
    fn collapse_blank_lines_collapses_consecutive() {
        let content = "line1\n\n\n\nline2";
        let result = collapse_blank_lines(content);
        assert_eq!(result, vec!["line1", "", "line2"]);
    }

    #[test]
    fn collapse_blank_lines_no_leading_blank() {
        let content = "\n\nline1\nline2";
        let result = collapse_blank_lines(content);
        assert_eq!(result, vec!["", "line1", "line2"]);
    }
}
