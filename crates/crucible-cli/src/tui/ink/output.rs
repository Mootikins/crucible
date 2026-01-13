use crossterm::{cursor, execute, terminal};
use std::io::{self, Stdout, Write};

pub struct OutputBuffer {
    stdout: Stdout,
    previous_lines: Vec<String>,
    previous_height: usize,
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputBuffer {
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            previous_lines: Vec::new(),
            previous_height: 0,
        }
    }

    pub fn render(&mut self, content: &str) -> io::Result<()> {
        let next_lines: Vec<String> = content.lines().map(String::from).collect();
        let next_height = next_lines.len();

        if next_lines == self.previous_lines {
            return Ok(());
        }

        if self.previous_height == 0 {
            for (i, line) in next_lines.iter().enumerate() {
                write!(self.stdout, "{}", line)?;
                if i < next_height - 1 {
                    write!(self.stdout, "\r\n")?;
                }
            }
            self.stdout.flush()?;
            self.previous_lines = next_lines;
            self.previous_height = next_height;
            return Ok(());
        }

        execute!(
            self.stdout,
            cursor::MoveUp(self.previous_height.saturating_sub(1) as u16),
            cursor::MoveToColumn(0),
        )?;

        let max_lines = next_height.max(self.previous_height);

        for i in 0..max_lines {
            let next_line = next_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            let prev_line = self.previous_lines.get(i).map(|s| s.as_str()).unwrap_or("");

            if next_line != prev_line || i >= self.previous_height {
                execute!(self.stdout, cursor::MoveToColumn(0))?;
                write!(self.stdout, "{}", next_line)?;
                execute!(
                    self.stdout,
                    terminal::Clear(terminal::ClearType::UntilNewLine)
                )?;
            }

            if i < max_lines - 1 {
                write!(self.stdout, "\r\n")?;
            }
        }

        if next_height < self.previous_height {
            execute!(
                self.stdout,
                terminal::Clear(terminal::ClearType::FromCursorDown)
            )?;
        }

        self.stdout.flush()?;
        self.previous_lines = next_lines;
        self.previous_height = next_height;

        Ok(())
    }

    pub fn clear(&mut self) -> io::Result<()> {
        if self.previous_height > 0 {
            execute!(
                self.stdout,
                cursor::MoveUp(self.previous_height.saturating_sub(1) as u16),
                cursor::MoveToColumn(0),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
            self.previous_lines.clear();
            self.previous_height = 0;
        }
        Ok(())
    }

    pub fn height(&self) -> usize {
        self.previous_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_buffer_creation() {
        let buffer = OutputBuffer::new();
        assert_eq!(buffer.height(), 0);
        assert!(buffer.previous_lines.is_empty());
    }
}
