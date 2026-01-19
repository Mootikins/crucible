use crate::tui::ink::ansi::{strip_ansi, visual_rows};
use crossterm::{cursor, execute, terminal};
use std::io::{self, Stdout, Write};

pub struct OutputBuffer {
    stdout: Stdout,
    previous_lines: Vec<String>,
    previous_visual_rows: usize,
    terminal_width: usize,
    terminal_height: usize,
    force_next_redraw: bool,
}

fn lines_visually_equal(a: &str, b: &str) -> bool {
    strip_ansi(a) == strip_ansi(b)
}

impl Default for OutputBuffer {
    fn default() -> Self {
        let (width, height) = terminal::size()
            .map(|(w, h)| (w as usize, h as usize))
            .unwrap_or((80, 24));
        Self::new(width, height)
    }
}

impl OutputBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            stdout: io::stdout(),
            previous_lines: Vec::new(),
            previous_visual_rows: 0,
            terminal_width: width,
            terminal_height: height,
            force_next_redraw: false,
        }
    }

    pub fn set_size(&mut self, width: usize, height: usize) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    pub fn render(&mut self, content: &str) -> io::Result<bool> {
        self.render_with_cursor_restore(content, 0)
    }

    pub fn render_with_cursor_restore(
        &mut self,
        content: &str,
        cursor_offset_from_end: u16,
    ) -> io::Result<bool> {
        let all_lines: Vec<String> = collapse_blank_lines(content);

        let line_visual_rows: Vec<usize> = all_lines
            .iter()
            .map(|line| visual_rows(line, self.terminal_width))
            .collect();

        let total_visual_rows: usize = line_visual_rows.iter().sum();
        let available_rows = self.terminal_height.saturating_sub(1);

        let (viewport_lines, viewport_visual_rows) = self.clamp_to_viewport(
            &all_lines,
            &line_visual_rows,
            total_visual_rows,
            available_rows,
        );

        let all_equal = !self.force_next_redraw
            && viewport_lines.len() == self.previous_lines.len()
            && viewport_lines
                .iter()
                .zip(self.previous_lines.iter())
                .all(|(a, b)| lines_visually_equal(a, b));

        if all_equal {
            return Ok(false);
        }

        tracing::debug!(
            prev_rows = self.previous_visual_rows,
            next_rows = viewport_visual_rows,
            line_count = viewport_lines.len(),
            width = self.terminal_width,
            height = self.terminal_height,
            force = self.force_next_redraw,
            cursor_offset_from_end,
            "render"
        );

        self.force_next_redraw = false;

        if self.previous_visual_rows > 0 {
            let move_up_amount = (self.previous_visual_rows as u16)
                .saturating_sub(1)
                .saturating_sub(cursor_offset_from_end);
            tracing::trace!(move_up_amount, "cursor move up");
            if move_up_amount > 0 {
                execute!(
                    self.stdout,
                    cursor::MoveUp(move_up_amount),
                    cursor::MoveToColumn(0),
                )?;
            } else {
                execute!(self.stdout, cursor::MoveToColumn(0))?;
            }
        }

        execute!(
            self.stdout,
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;

        for (i, line) in viewport_lines.iter().enumerate() {
            write!(self.stdout, "{}", line)?;
            if i < viewport_lines.len() - 1 {
                write!(self.stdout, "\r\n")?;
            }
        }

        self.stdout.flush()?;
        self.previous_lines = viewport_lines;
        self.previous_visual_rows = viewport_visual_rows;

        Ok(true)
    }

    pub fn clear(&mut self) -> io::Result<()> {
        if self.previous_visual_rows > 0 {
            execute!(
                self.stdout,
                cursor::MoveUp(self.previous_visual_rows.saturating_sub(1) as u16),
                cursor::MoveToColumn(0),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
            self.previous_lines.clear();
            self.previous_visual_rows = 0;
        }
        Ok(())
    }

    pub fn height(&self) -> usize {
        self.previous_visual_rows
    }

    pub fn reset(&mut self) {
        self.previous_lines.clear();
        self.previous_visual_rows = 0;
    }

    pub fn force_redraw(&mut self) {
        self.previous_lines.clear();
        self.previous_visual_rows = 0;
        self.force_next_redraw = true;
    }

    pub fn render_fullscreen(&mut self, content: &str) -> io::Result<()> {
        execute!(
            self.stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        )?;

        write!(self.stdout, "{}", content)?;
        self.stdout.flush()?;

        self.previous_lines.clear();
        self.previous_visual_rows = 0;

        Ok(())
    }

    fn clamp_to_viewport(
        &self,
        all_lines: &[String],
        line_visual_rows: &[usize],
        total_visual_rows: usize,
        available_rows: usize,
    ) -> (Vec<String>, usize) {
        if total_visual_rows <= available_rows {
            return (all_lines.to_vec(), total_visual_rows);
        }

        let mut rows_remaining = available_rows;
        let mut start_idx = all_lines.len();

        for (i, &row_count) in line_visual_rows.iter().enumerate().rev() {
            if rows_remaining >= row_count {
                rows_remaining -= row_count;
                start_idx = i;
            } else {
                break;
            }
        }

        let viewport: Vec<String> = all_lines[start_idx..].to_vec();
        let viewport_rows: usize = line_visual_rows[start_idx..].iter().sum();

        tracing::debug!(
            total_rows = total_visual_rows,
            available = available_rows,
            skipped_lines = start_idx,
            viewport_rows,
            "viewport clamped"
        );

        (viewport, viewport_rows)
    }
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
        assert!(buffer.previous_lines.is_empty());
    }

    #[test]
    fn test_viewport_clamp_content_fits() {
        let buffer = OutputBuffer::new(80, 24);
        let lines: Vec<String> = vec!["line1".into(), "line2".into(), "line3".into()];
        let visual_rows = vec![1, 1, 1];

        let (viewport, rows) = buffer.clamp_to_viewport(&lines, &visual_rows, 3, 22);

        assert_eq!(viewport.len(), 3);
        assert_eq!(rows, 3);
    }

    #[test]
    fn test_viewport_clamp_content_exceeds() {
        let buffer = OutputBuffer::new(80, 10);
        let lines: Vec<String> = (0..20).map(|i| format!("line{}", i)).collect();
        let visual_rows = vec![1; 20];

        let (viewport, rows) = buffer.clamp_to_viewport(&lines, &visual_rows, 20, 8);

        assert_eq!(viewport.len(), 8);
        assert_eq!(rows, 8);
        assert_eq!(viewport[0], "line12");
        assert_eq!(viewport[7], "line19");
    }

    #[test]
    fn test_viewport_clamp_with_wrapped_lines() {
        let buffer = OutputBuffer::new(40, 10);
        let lines: Vec<String> = vec![
            "short".into(),
            "this is a very long line that wraps".into(),
            "another short".into(),
        ];
        let visual_rows = vec![1, 2, 1];

        let (viewport, rows) = buffer.clamp_to_viewport(&lines, &visual_rows, 4, 3);

        assert_eq!(viewport.len(), 2);
        assert_eq!(rows, 3);
        assert_eq!(viewport[0], "this is a very long line that wraps");
        assert_eq!(viewport[1], "another short");
    }
}
