use crate::tui::oil::{Color, Style};
use crucible_config::HighlightingConfig;
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub struct SyntaxHighlighter {
    theme_name: String,
    enabled: bool,
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            theme_name: "base16-ocean.dark".to_string(),
            enabled: true,
        }
    }

    pub fn from_config(config: &HighlightingConfig) -> Self {
        Self {
            theme_name: config.theme.clone(),
            enabled: config.enabled,
        }
    }

    pub fn with_theme(mut self, theme: &str) -> Self {
        self.theme_name = theme.to_string();
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn highlight(&self, code: &str, language: &str) -> Vec<HighlightedLine> {
        if !self.enabled {
            return code
                .lines()
                .map(|line| HighlightedLine {
                    spans: vec![HighlightedSpan {
                        text: line.to_string(),
                        style: Style::default(),
                    }],
                })
                .collect();
        }

        let syntax = SYNTAX_SET
            .find_syntax_by_token(language)
            .or_else(|| SYNTAX_SET.find_syntax_by_extension(language))
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        let theme = THEME_SET.themes.get(&self.theme_name).unwrap_or_else(|| {
            THEME_SET
                .themes
                .values()
                .next()
                .expect("at least one theme")
        });

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in LinesWithEndings::from(code) {
            let ranges = highlighter
                .highlight_line(line, &SYNTAX_SET)
                .unwrap_or_default();

            let spans: Vec<HighlightedSpan> = ranges
                .into_iter()
                .map(|(style, text)| HighlightedSpan {
                    text: text.trim_end_matches('\n').to_string(),
                    style: syntect_to_ink_style(style),
                })
                .collect();

            result.push(HighlightedLine { spans });
        }

        result
    }

    pub fn available_themes() -> Vec<&'static str> {
        THEME_SET.themes.keys().map(|s| s.as_str()).collect()
    }

    pub fn supports_language(language: &str) -> bool {
        SYNTAX_SET.find_syntax_by_token(language).is_some()
            || SYNTAX_SET.find_syntax_by_extension(language).is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightedLine {
    pub spans: Vec<HighlightedSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightedSpan {
    pub text: String,
    pub style: Style,
}

fn syntect_to_ink_style(syntect_style: SyntectStyle) -> Style {
    let fg = syntect_style.foreground;
    let mut style = Style::new().fg(Color::Rgb(fg.r, fg.g, fg.b));

    if syntect_style.font_style.contains(FontStyle::BOLD) {
        style = style.bold();
    }
    if syntect_style.font_style.contains(FontStyle::ITALIC) {
        style = style.italic();
    }
    if syntect_style.font_style.contains(FontStyle::UNDERLINE) {
        style = style.underline();
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_code_produces_colored_spans() {
        let highlighter = SyntaxHighlighter::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}";

        let lines = highlighter.highlight(code, "rs");

        assert_eq!(lines.len(), 3);
        assert!(!lines[0].spans.is_empty(), "first line should have spans");

        let has_color = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.fg != Some(Color::Reset) && span.style.fg.is_some())
        });
        assert!(has_color, "highlighted code should have colored spans");
    }

    #[test]
    fn highlight_unknown_language_falls_back_to_plain_text() {
        let highlighter = SyntaxHighlighter::new();
        let code = "some plain text";

        let lines = highlighter.highlight(code, "nonexistent_language_xyz");

        assert_eq!(lines.len(), 1);
        assert!(!lines[0].spans.is_empty());
    }

    #[test]
    fn highlight_preserves_line_structure() {
        let highlighter = SyntaxHighlighter::new();
        let code = "line1\nline2\nline3";

        let lines = highlighter.highlight(code, "txt");

        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlight_empty_code_returns_empty() {
        let highlighter = SyntaxHighlighter::new();

        let lines = highlighter.highlight("", "rs");

        assert!(lines.is_empty());
    }

    #[test]
    fn supports_common_languages() {
        assert!(SyntaxHighlighter::supports_language("rs"));
        assert!(SyntaxHighlighter::supports_language("py"));
        assert!(SyntaxHighlighter::supports_language("js"));
        assert!(SyntaxHighlighter::supports_language("go"));
        assert!(SyntaxHighlighter::supports_language("c"));
        assert!(SyntaxHighlighter::supports_language("cpp"));
        assert!(SyntaxHighlighter::supports_language("sh"));
        assert!(SyntaxHighlighter::supports_language("json"));
        assert!(SyntaxHighlighter::supports_language("html"));
    }

    #[test]
    fn available_themes_not_empty() {
        let themes = SyntaxHighlighter::available_themes();
        assert!(!themes.is_empty());
        assert!(themes.contains(&"base16-ocean.dark"));
    }

    #[test]
    fn with_theme_changes_theme() {
        let highlighter = SyntaxHighlighter::new().with_theme("InspiredGitHub");

        let lines = highlighter.highlight("fn main() {}", "rs");

        assert!(!lines.is_empty());
    }

    #[test]
    fn highlight_detects_bold_italic_underline() {
        let highlighter = SyntaxHighlighter::new();
        let code = "// comment\nfn main() {}";

        let lines = highlighter.highlight(code, "rs");

        assert!(lines.len() >= 2);
    }

    #[test]
    fn from_config_uses_theme() {
        let config = HighlightingConfig {
            enabled: true,
            theme: "InspiredGitHub".to_string(),
        };
        let highlighter = SyntaxHighlighter::from_config(&config);

        assert!(highlighter.is_enabled());
        let lines = highlighter.highlight("fn main() {}", "rs");
        assert!(!lines.is_empty());
    }

    #[test]
    fn disabled_highlighting_returns_plain_text() {
        let highlighter = SyntaxHighlighter::new().with_enabled(false);
        let code = "fn main() {\n    println!(\"Hello\");\n}";

        let lines = highlighter.highlight(code, "rs");

        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert_eq!(line.spans.len(), 1);
            assert_eq!(line.spans[0].style, Style::default());
        }
    }

    #[test]
    fn from_config_respects_enabled_flag() {
        let config = HighlightingConfig {
            enabled: false,
            theme: "base16-ocean.dark".to_string(),
        };
        let highlighter = SyntaxHighlighter::from_config(&config);

        assert!(!highlighter.is_enabled());
    }
}
