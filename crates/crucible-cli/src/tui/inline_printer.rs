//! Inline printer for writing formatted content to stdout
//!
//! Prints conversation content directly to terminal scrollback with ANSI styling.
//! Used by InlineRunner for the hybrid viewport/scrollback architecture.

use crate::formatting::render_markdown;
use crate::tui::content_block::StreamBlock;
use crate::tui::conversation::ToolStatus;
use crossterm::style::{Color, Stylize};
use std::io::{self, Write};

// Crossterm color constants (matching ratatui styles in styles.rs)
mod ct_colors {
    use crossterm::style::Color;

    /// User message background
    pub const USER_BG: Color = Color::Rgb { r: 60, g: 60, b: 80 };

    /// Assistant prefix color (dim gray)
    pub const ASSISTANT_PREFIX: Color = Color::DarkGrey;

    /// Tool running indicator
    pub const TOOL_RUNNING: Color = Color::White;

    /// Tool complete indicator
    pub const TOOL_COMPLETE: Color = Color::Green;

    /// Tool error indicator
    pub const TOOL_ERROR: Color = Color::Red;
}

/// Unicode indicators for various states
mod ct_indicators {
    /// Spinner frames for running tools
    pub const SPINNER_FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

    /// Tool complete indicator
    pub const TOOL_COMPLETE: &str = "●";

    /// Tool error indicator
    pub const TOOL_ERROR: &str = "✗";

    /// Assistant message prefix
    pub const ASSISTANT_PREFIX: &str = "●";
}

/// Default margin subtracted from terminal width for textwidth
const TEXTWIDTH_MARGIN: usize = 4;

/// Printer for emitting formatted content to terminal scrollback
///
/// Handles word wrapping at textwidth and ANSI styling for markdown content.
/// All output goes directly to stdout and becomes part of native terminal scrollback.
pub struct InlinePrinter {
    /// Width for word wrapping (terminal width - margin)
    textwidth: usize,
}

impl Default for InlinePrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl InlinePrinter {
    /// Create a new printer with default textwidth (80 - margin)
    pub fn new() -> Self {
        Self {
            textwidth: 80 - TEXTWIDTH_MARGIN,
        }
    }

    /// Update textwidth based on terminal width
    pub fn update_width(&mut self, terminal_width: u16) {
        self.textwidth = (terminal_width as usize).saturating_sub(TEXTWIDTH_MARGIN);
        // Minimum textwidth of 40 columns
        if self.textwidth < 40 {
            self.textwidth = 40;
        }
    }

    /// Get current textwidth
    pub fn textwidth(&self) -> usize {
        self.textwidth
    }

    /// Print a user message to scrollback
    ///
    /// Format: ` > message content`
    /// Multi-line content is indented with `   ` (3 spaces) for continuation.
    pub fn print_user_message(&self, content: &str) -> io::Result<()> {
        let mut stdout = io::stdout().lock();

        // Blank line before user message
        writeln!(stdout)?;

        // Wrap and print with prefix
        let wrapped = self.wrap_text(content, self.textwidth.saturating_sub(3));
        for (i, line) in wrapped.iter().enumerate() {
            if i == 0 {
                // First line with " > " prefix (inverted style for visibility)
                let prefix = " > ".on(ct_colors::USER_BG);
                let styled_line = line.clone().on(ct_colors::USER_BG);
                writeln!(stdout, "{}{}", prefix, styled_line)?;
            } else {
                // Continuation lines with 3-space indent
                let styled_line = line.clone().on(ct_colors::USER_BG);
                writeln!(stdout, "   {}", styled_line)?;
            }
        }

        stdout.flush()
    }

    /// Print a completed assistant message to scrollback
    ///
    /// Renders markdown with ANSI styling. Format: ` · content`
    pub fn print_assistant_message(&self, blocks: &[StreamBlock]) -> io::Result<()> {
        let markdown = blocks_to_markdown(blocks);
        let rendered = render_markdown(&markdown);
        self.print_prefixed_lines(&rendered, ct_indicators::ASSISTANT_PREFIX, ct_colors::ASSISTANT_PREFIX)
    }

    /// Print a tool call result to scrollback
    ///
    /// Format: ` ✓ tool_name → result` (or ✗ for errors)
    pub fn print_tool_result(&self, name: &str, status: &ToolStatus) -> io::Result<()> {
        let mut stdout = io::stdout().lock();

        let (indicator, style, suffix) = match status {
            ToolStatus::Running => (
                ct_indicators::SPINNER_FRAMES[0],
                ct_colors::TOOL_RUNNING,
                String::new(),
            ),
            ToolStatus::Complete { summary } => (
                ct_indicators::TOOL_COMPLETE,
                ct_colors::TOOL_COMPLETE,
                summary
                    .as_ref()
                    .map(|s| format!(" → {}", s))
                    .unwrap_or_default(),
            ),
            ToolStatus::Error { message } => (
                ct_indicators::TOOL_ERROR,
                ct_colors::TOOL_ERROR,
                format!(" → {}", message),
            ),
        };

        writeln!(
            stdout,
            "{}",
            format!(" {} {}{}", indicator, name, suffix).with(style)
        )?;

        stdout.flush()
    }

    /// Print a system/status message
    pub fn print_status(&self, message: &str) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", message.with(Color::DarkGrey))?;
        stdout.flush()
    }

    /// Print lines with a styled prefix on the first line, indent on continuations.
    ///
    /// Used by `print_assistant_message` to avoid duplication.
    fn print_prefixed_lines(&self, content: &str, prefix_char: &str, color: Color) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout)?;

        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                let prefix = format!(" {} ", prefix_char).with(color);
                writeln!(stdout, "{}{}", prefix, line)?;
            } else {
                writeln!(stdout, "   {}", line)?;
            }
        }

        stdout.flush()
    }

    /// Simple word-aware text wrapping
    fn wrap_text(&self, text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![text.to_string()];
        }

        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            if paragraph.is_empty() {
                lines.push(String::new());
                continue;
            }

            let mut current_line = String::new();
            let mut current_width = 0;

            for word in paragraph.split_whitespace() {
                let word_width = word.chars().count();

                if current_width == 0 {
                    // First word on line
                    current_line = word.to_string();
                    current_width = word_width;
                } else if current_width + 1 + word_width <= width {
                    // Word fits on current line
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_width += 1 + word_width;
                } else {
                    // Word doesn't fit, start new line
                    lines.push(current_line);
                    current_line = word.to_string();
                    current_width = word_width;
                }
            }

            if !current_line.is_empty() || paragraph.is_empty() {
                lines.push(current_line);
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }
}

/// Convert StreamBlocks back to markdown string
fn blocks_to_markdown(blocks: &[StreamBlock]) -> String {
    let mut markdown = String::new();

    for block in blocks {
        match block {
            StreamBlock::Prose { text, .. } => {
                markdown.push_str(text);
            }
            StreamBlock::Code { lang, content, .. } => {
                markdown.push_str("```");
                if let Some(lang) = lang {
                    markdown.push_str(lang);
                }
                markdown.push('\n');
                markdown.push_str(content);
                if !content.ends_with('\n') {
                    markdown.push('\n');
                }
                markdown.push_str("```\n");
            }
        }
    }

    markdown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_printer() {
        let printer = InlinePrinter::new();
        assert_eq!(printer.textwidth(), 80 - TEXTWIDTH_MARGIN);
    }

    #[test]
    fn test_update_width() {
        let mut printer = InlinePrinter::new();
        printer.update_width(120);
        assert_eq!(printer.textwidth(), 120 - TEXTWIDTH_MARGIN);
    }

    #[test]
    fn test_update_width_minimum() {
        let mut printer = InlinePrinter::new();
        printer.update_width(30); // Too small
        assert_eq!(printer.textwidth(), 40); // Minimum enforced
    }

    #[test]
    fn test_wrap_text_no_wrap_needed() {
        let printer = InlinePrinter::new();
        let result = printer.wrap_text("Hello world", 80);
        assert_eq!(result, vec!["Hello world"]);
    }

    #[test]
    fn test_wrap_text_wraps_long_line() {
        let printer = InlinePrinter::new();
        let result = printer.wrap_text("Hello world this is a test", 12);
        assert_eq!(result, vec!["Hello world", "this is a", "test"]);
    }

    #[test]
    fn test_wrap_text_preserves_newlines() {
        let printer = InlinePrinter::new();
        let result = printer.wrap_text("Line one\nLine two", 80);
        assert_eq!(result, vec!["Line one", "Line two"]);
    }

    #[test]
    fn test_wrap_text_empty_lines() {
        let printer = InlinePrinter::new();
        let result = printer.wrap_text("Line one\n\nLine three", 80);
        assert_eq!(result, vec!["Line one", "", "Line three"]);
    }

    #[test]
    fn test_blocks_to_markdown_prose() {
        let blocks = vec![StreamBlock::prose("Hello world")];
        let result = blocks_to_markdown(&blocks);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_blocks_to_markdown_code() {
        let blocks = vec![StreamBlock::code(Some("rust".into()), "fn main() {}")];
        let result = blocks_to_markdown(&blocks);
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main()"));
        assert!(result.contains("```"));
    }

    #[test]
    fn test_blocks_to_markdown_mixed() {
        let blocks = vec![
            StreamBlock::prose("Here's code:"),
            StreamBlock::code(Some("rust".into()), "let x = 1;"),
            StreamBlock::prose("That's all."),
        ];
        let result = blocks_to_markdown(&blocks);
        assert!(result.contains("Here's code:"));
        assert!(result.contains("```rust"));
        assert!(result.contains("let x = 1;"));
        assert!(result.contains("That's all."));
    }
}
