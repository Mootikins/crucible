//! Markdown rendering with syntax highlighting
//!
//! Uses termimad for markdown structure and syntect for code block highlighting.
//! Auto-detects terminal theme (dark/light).

use std::env;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use termimad::MadSkin;

/// Markdown renderer with syntax highlighting support
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    dark_theme: Theme,
    light_theme: Theme,
    skin_dark: MadSkin,
    skin_light: MadSkin,
    is_dark: bool,
}

impl MarkdownRenderer {
    /// Create a new renderer with auto-detected theme
    pub fn new() -> Self {
        let is_dark = Self::detect_terminal_background();
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        let dark_theme = theme_set.themes["base16-ocean.dark"].clone();
        let light_theme = theme_set.themes["base16-ocean.light"].clone();

        let skin_dark = Self::create_dark_skin();
        let skin_light = Self::create_light_skin();

        Self {
            syntax_set,
            dark_theme,
            light_theme,
            skin_dark,
            skin_light,
            is_dark,
        }
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

    /// Render markdown to ANSI-styled string
    pub fn render(&self, markdown: &str) -> String {
        let skin = if self.is_dark {
            &self.skin_dark
        } else {
            &self.skin_light
        };

        // For now, use termimad directly
        // TODO: Extract code blocks and highlight with syntect
        skin.term_text(markdown).to_string()
    }

    /// Render a code block with syntax highlighting
    #[allow(dead_code)]
    fn render_code_block(&self, code: &str, lang: &str) -> String {
        use syntect::easy::HighlightLines;
        use syntect::util::as_24_bit_terminal_escaped;

        let theme = if self.is_dark {
            &self.dark_theme
        } else {
            &self.light_theme
        };

        let syntax = self.syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut h = HighlightLines::new(syntax, theme);
        let mut result = String::new();

        for line in code.lines() {
            match h.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let escaped = as_24_bit_terminal_escaped(&ranges, false);
                    result.push_str(&escaped);
                    result.push('\n');
                }
                Err(_) => {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        // Reset colors at end
        result.push_str("\x1b[0m");
        result
    }

    fn create_dark_skin() -> MadSkin {
        use termimad::crossterm::style::Color;

        let mut skin = MadSkin::default();
        skin.bold.set_fg(Color::White);
        skin.italic.set_fg(Color::Cyan);
        skin.inline_code.set_bg(Color::DarkGrey);
        skin.code_block.set_bg(Color::DarkGrey);
        skin
    }

    fn create_light_skin() -> MadSkin {
        use termimad::crossterm::style::Color;

        let mut skin = MadSkin::default();
        skin.bold.set_fg(Color::Black);
        skin.italic.set_fg(Color::DarkBlue);
        skin.inline_code.set_bg(Color::Grey);
        skin.code_block.set_bg(Color::Grey);
        skin
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
        let renderer = MarkdownRenderer::new();
        assert!(!renderer.syntax_set.syntaxes().is_empty());
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
    fn test_code_block_highlighting() {
        let renderer = MarkdownRenderer::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let result = renderer.render_code_block(code, "rust");
        // Should contain ANSI escape codes
        assert!(result.contains("\x1b["));
    }
}
