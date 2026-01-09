//! Markdown rendering with syntax highlighting
//!
//! Uses ratatui-native rendering for markdown with syntect for code block highlighting.
//! Auto-detects terminal theme (dark/light).
//!
//! Tables are rendered with full box-drawing borders:
//! - Top border: ┌─┬─┐
//! - Bottom border: └─┴─┘
//! - Row separators between all data rows
//! - 1 space padding in cells

use std::env;
use ratatui::text::Line;

use super::ratatui_markdown::RatatuiMarkdown;
use super::theme::MarkdownTheme;

/// Markdown renderer with syntax highlighting support
///
/// Delegates all rendering to `RatatuiMarkdown`. Auto-detects terminal
/// theme (dark/light) based on environment variables.
pub struct MarkdownRenderer {
    is_dark: bool,
}

impl MarkdownRenderer {
    /// Create a new renderer with auto-detected theme
    pub fn new() -> Self {
        let is_dark = Self::detect_terminal_background();
        Self { is_dark }
    }

    /// Detect if terminal has dark background
    ///
    /// Checks in order:
    /// 1. COLORFGBG env var (format: "fg;bg", bg > 6 = light)
    /// 2. TERM_BACKGROUND env var ("dark" | "light")
    /// 3. Default to dark
    pub fn detect_terminal_background() -> bool {
        // Check COLORFGBG first
        if let Ok(val) = env::var("COLORFGBG") {
            if let Some(bg) = val.split(';').nth(1) {
                if let Ok(bg_num) = bg.parse::<u8>() {
                    return bg_num <= 6; // 0-6 are dark colors
                }
            }
        }

        // Check TERM_BACKGROUND
        if let Ok(val) = env::var("TERM_BACKGROUND") {
            return val.to_lowercase() != "light";
        }

        // Default to dark
        true
    }

    /// Render markdown to plain string (no width constraint)
    pub fn render(&self, markdown: &str) -> String {
        self.render_with_width(markdown, None)
    }

    /// Render markdown with optional width constraint for word wrapping
    ///
    /// When width is provided, text wraps at word boundaries to fit.
    /// This should be used for TUI rendering where we know the column width.
    pub fn render_with_width(&self, markdown: &str, width: Option<usize>) -> String {
        let w = width.unwrap_or(80);
        let lines = self.render_lines(markdown, w);
        lines.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Render markdown to ratatui Lines (new API using ratatui-native renderer)
    ///
    /// Returns a vector of `Line<'static>` that can be passed directly to
    /// a ratatui `Paragraph` widget. This uses the new `RatatuiMarkdown` renderer
    /// internally, which provides better integration with ratatui's styling system.
    ///
    /// # Arguments
    ///
    /// * `markdown` - The markdown text to render
    /// * `width` - Width constraint for word wrapping
    ///
    /// # Example
    ///
    /// ```ignore
    /// let lines = renderer.render_lines("**bold** text", 80);
    /// let paragraph = Paragraph::new(lines);
    /// ```
    pub fn render_lines(&self, markdown: &str, width: usize) -> Vec<Line<'static>> {
        let theme = if self.is_dark {
            MarkdownTheme::dark()
        } else {
            MarkdownTheme::light()
        };
        RatatuiMarkdown::new(theme)
            .with_width(width)
            .render(markdown)
    }

}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let _renderer = MarkdownRenderer::new();
        // Renderer should be created successfully
    }

    #[test]
    fn test_plain_text_passthrough() {
        let renderer = MarkdownRenderer::new();
        let result = renderer.render("Hello world");
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn test_theme_detection_default_dark() {
        // Clear env vars
        env::remove_var("COLORFGBG");
        env::remove_var("TERM_BACKGROUND");
        assert!(MarkdownRenderer::detect_terminal_background());
    }

    #[test]
    fn test_render_lines_returns_lines() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render_lines("**bold** text", 80);
        // Should return at least one line
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_with_width_returns_string() {
        let renderer = MarkdownRenderer::new();
        let result = renderer.render_with_width("Hello world", Some(40));
        assert!(result.contains("Hello world"));
    }
}
