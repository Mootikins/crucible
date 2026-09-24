//! Full-screen output: a row diff over a fixed grid on the alternate screen.
//!
//! The inline mode (`output.rs`) lets the terminal own the scroll, so it can
//! address only the rows still on screen. The full-screen mode owns every row,
//! so a frame can rewrite one row by its address. [`ScreenDiff`] keeps the rows
//! of the last frame and writes only the rows that differ, inside one
//! synchronized update. It never clears the whole screen: a written row ends
//! with an erase to the end of the line, so an old tail cannot survive.

use crate::cell_grid::CellGrid;
use crate::output::{BEGIN_SYNCHRONIZED_UPDATE, END_SYNCHRONIZED_UPDATE};
use std::fmt::Write as _;
use std::io::{self, Write};

/// Mouse reporting for the full-screen mode: button presses (1000), motion
/// while a button is down (1002), in SGR encoding (1006).
///
/// Crossterm's `EnableMouseCapture` also turns on 1003, which reports every
/// pointer move. Selection needs only drags, and 1003 floods the event loop.
pub const ENABLE_MOUSE_CAPTURE: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";
/// The reverse of [`ENABLE_MOUSE_CAPTURE`].
pub const DISABLE_MOUSE_CAPTURE: &str = "\x1b[?1006l\x1b[?1002l\x1b[?1000l";

/// What one call to [`ScreenDiff::present`] wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresentStats {
    /// Rows written. Zero means the frame wrote nothing at all.
    pub rows_written: usize,
    /// Bytes written, escape sequences included.
    pub bytes: usize,
}

/// The rows the terminal shows, for the next row diff.
#[derive(Debug, Default)]
pub struct ScreenDiff {
    rows: Vec<String>,
    cursor: Option<(u16, u16)>,
    /// False until a frame is written, and after [`Self::invalidate`]. An
    /// invalid diff writes every row.
    valid: bool,
}

impl ScreenDiff {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget what the terminal shows. The next frame writes every row.
    ///
    /// Call this after a resize, and after anything else wrote to the screen.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Write the rows of `grid` that differ from the last frame.
    ///
    /// `cursor` is `(column, row)` for a visible cursor, or `None` to keep it
    /// hidden. A frame with no changed row and the same cursor writes nothing.
    pub fn present(
        &mut self,
        out: &mut impl Write,
        grid: &CellGrid,
        cursor: Option<(u16, u16)>,
    ) -> io::Result<PresentStats> {
        let rows: Vec<String> = (0..grid.height()).map(|y| grid.row_ansi(y)).collect();
        let changed: Vec<usize> = (0..rows.len())
            .filter(|&y| !self.valid || self.rows.get(y) != Some(&rows[y]))
            .collect();
        if self.valid && changed.is_empty() && cursor == self.cursor {
            return Ok(PresentStats::default());
        }

        let mut buf = String::with_capacity(changed.len() * (grid.width() + 16) + 32);
        buf.push_str(BEGIN_SYNCHRONIZED_UPDATE);
        // Hide the cursor while rows change, so it never shows mid-frame.
        buf.push_str("\x1b[?25l");
        for &y in &changed {
            // Writing into a String cannot fail.
            let _ = write!(buf, "\x1b[{};1H", y + 1);
            buf.push_str(&rows[y]);
        }
        if let Some((x, y)) = cursor {
            let _ = write!(buf, "\x1b[{};{}H\x1b[?25h", y + 1, x + 1);
        }
        buf.push_str(END_SYNCHRONIZED_UPDATE);
        out.write_all(buf.as_bytes())?;
        out.flush()?;

        self.rows = rows;
        self.cursor = cursor;
        self.valid = true;
        Ok(PresentStats {
            rows_written: changed.len(),
            bytes: buf.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(lines: &[&str], width: usize) -> CellGrid {
        let mut grid = CellGrid::new(width, lines.len());
        for (y, line) in lines.iter().enumerate() {
            grid.blit_line(line, 0, y);
        }
        grid
    }

    fn screen(bytes: &[u8], width: u16, height: u16) -> vt100::Parser {
        let mut parser = vt100::Parser::new(height, width, 0);
        parser.process(bytes);
        parser
    }

    #[test]
    fn the_first_frame_writes_every_row() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        let stats = diff
            .present(&mut out, &grid(&["a", "b", "c"], 10), None)
            .unwrap();
        assert_eq!(stats.rows_written, 3);
        assert_eq!(stats.bytes, out.len());
    }

    #[test]
    fn a_frame_rewrites_only_the_changed_row() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        diff.present(&mut out, &grid(&["one", "two", "three"], 10), None)
            .unwrap();
        out.clear();

        let stats = diff
            .present(&mut out, &grid(&["one", "TWO", "three"], 10), None)
            .unwrap();

        assert_eq!(stats.rows_written, 1);
        let written = String::from_utf8(out).unwrap();
        assert!(written.contains("\x1b[2;1HTWO"), "{written:?}");
        assert!(!written.contains("one") && !written.contains("three"));
    }

    #[test]
    fn an_unchanged_frame_writes_nothing() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        let frame = grid(&["one", "two"], 10);
        diff.present(&mut out, &frame, Some((1, 1))).unwrap();
        out.clear();

        let stats = diff.present(&mut out, &frame, Some((1, 1))).unwrap();

        assert_eq!(stats, PresentStats::default());
        assert!(out.is_empty(), "no synchronized update for no change");
    }

    #[test]
    fn a_cursor_move_alone_writes_no_row() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        let frame = grid(&["one"], 10);
        diff.present(&mut out, &frame, Some((0, 0))).unwrap();
        out.clear();

        let stats = diff.present(&mut out, &frame, Some((2, 0))).unwrap();

        assert_eq!(stats.rows_written, 0);
        assert!(String::from_utf8(out)
            .unwrap()
            .contains("\x1b[1;3H\x1b[?25h"));
    }

    #[test]
    fn a_shorter_row_erases_the_old_tail() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        diff.present(&mut out, &grid(&["a long row"], 10), None)
            .unwrap();
        diff.present(&mut out, &grid(&["short"], 10), None).unwrap();

        let parser = screen(&out, 10, 1);
        assert_eq!(parser.screen().contents(), "short");
    }

    #[test]
    fn a_full_row_keeps_its_last_cell() {
        // An erase with the cursor in the last column deletes that cell, so a
        // full row must not end with one.
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        diff.present(&mut out, &grid(&["0123456789"], 10), None)
            .unwrap();

        let parser = screen(&out, 10, 1);
        assert_eq!(parser.screen().contents(), "0123456789");
        assert!(!String::from_utf8(out).unwrap().contains("\x1b[K"));
    }

    #[test]
    fn every_frame_is_one_synchronized_update_without_a_screen_clear() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        diff.present(&mut out, &grid(&["a", "b"], 10), None)
            .unwrap();
        diff.present(&mut out, &grid(&["a", "c"], 10), None)
            .unwrap();

        let written = String::from_utf8(out).unwrap();
        assert_eq!(written.matches(BEGIN_SYNCHRONIZED_UPDATE).count(), 2);
        assert_eq!(written.matches(END_SYNCHRONIZED_UPDATE).count(), 2);
        assert!(written.starts_with(BEGIN_SYNCHRONIZED_UPDATE));
        assert!(written.ends_with(END_SYNCHRONIZED_UPDATE));
        assert!(!written.contains("\x1b[2J"), "no full-screen clear");
    }

    #[test]
    fn invalidate_writes_every_row_again() {
        let mut diff = ScreenDiff::new();
        let mut out = Vec::new();
        let frame = grid(&["a", "b"], 10);
        diff.present(&mut out, &frame, None).unwrap();
        diff.invalidate();

        let stats = diff.present(&mut out, &frame, None).unwrap();
        assert_eq!(stats.rows_written, 2);
    }
}
