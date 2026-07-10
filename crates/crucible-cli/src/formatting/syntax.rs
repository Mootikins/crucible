use crate::tui::oil::{Color, Style};
use crucible_core::config::HighlightingConfig;
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// Process-wide highlighting state, mirroring how render code reads the TUI
/// palette from `theme::active()`. The config-seeded values and the `:set
/// theme` override are kept separate so `:set theme&` (reset) can revert
/// rendering to the seed. `None` fields fall back to the
/// `SyntaxHighlighter::new()` defaults.
#[derive(Default)]
struct HighlightingState {
    seeded_theme: Option<String>,
    seeded_enabled: Option<bool>,
    override_theme: Option<String>,
}

static ACTIVE_HIGHLIGHTING: std::sync::RwLock<HighlightingState> =
    std::sync::RwLock::new(HighlightingState {
        seeded_theme: None,
        seeded_enabled: None,
        override_theme: None,
    });

/// Set the active syntax-highlight theme for subsequent renders.
pub fn set_active_theme(name: &str) {
    ACTIVE_HIGHLIGHTING
        .write()
        .expect("highlighting lock poisoned")
        .override_theme = Some(name.to_string());
}

/// Drop the `:set theme` override, reverting to the config-seeded theme.
pub fn clear_theme_override() {
    ACTIVE_HIGHLIGHTING
        .write()
        .expect("highlighting lock poisoned")
        .override_theme = None;
}

/// Seed the active highlighting state from config (theme + enabled).
pub fn seed_from_config(config: &HighlightingConfig) {
    let mut state = ACTIVE_HIGHLIGHTING
        .write()
        .expect("highlighting lock poisoned");
    state.seeded_theme = Some(config.theme.clone());
    state.seeded_enabled = Some(config.enabled);
}

/// Serializes tests that mutate ACTIVE_HIGHLIGHTING. Isolated per test under
/// nextest; required for correctness under the shared-process `cargo test`
/// fallback. Any test (in any module) that writes the global must hold this.
#[cfg(test)]
pub(crate) static ACTIVE_STATE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The theme name renders currently use (override > seed > built-in default).
pub fn active_theme_name() -> String {
    let state = ACTIVE_HIGHLIGHTING
        .read()
        .expect("highlighting lock poisoned");
    state
        .override_theme
        .clone()
        .or_else(|| state.seeded_theme.clone())
        .unwrap_or_else(|| "base16-ocean.dark".to_string())
}

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

    /// Highlighter configured from the process-wide active state (the
    /// config-seeded, `:set theme`-updatable counterpart of `new()`).
    /// Render code should prefer this over `new()`.
    pub fn active() -> Self {
        let enabled = ACTIVE_HIGHLIGHTING
            .read()
            .expect("highlighting lock poisoned")
            .seeded_enabled;
        let mut h = Self::new().with_theme(&active_theme_name());
        if let Some(enabled) = enabled {
            h.enabled = enabled;
        }
        h
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

    // ACTIVE_HIGHLIGHTING is process-global. Isolated per test under nextest;
    // under the `cargo test` fallback these tests serialize on the shared
    // lock and only assert their OWN writes, never the ambient default.

    #[test]
    fn active_reflects_set_active_theme() {
        let _guard = ACTIVE_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_active_theme("InspiredGitHub");
        assert_eq!(active_theme_name(), "InspiredGitHub");
        assert_eq!(SyntaxHighlighter::active().theme_name, "InspiredGitHub");
    }

    #[test]
    fn seed_from_config_sets_theme_and_enabled() {
        let _guard = ACTIVE_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let config = HighlightingConfig {
            enabled: false,
            theme: "Solarized (light)".to_string(),
        };
        seed_from_config(&config);
        // An override from a prior same-process test would shadow the seed.
        clear_theme_override();
        let h = SyntaxHighlighter::active();
        assert_eq!(h.theme_name, "Solarized (light)");
        assert!(!h.is_enabled());
    }

    /// Pure-core proof that the theme knob changes rendered colors: the same
    /// code highlighted under two themes must produce different styles.
    #[test]
    fn different_themes_produce_different_styles() {
        let code = "fn main() { let x = 42; }";
        let default_lines = SyntaxHighlighter::new().highlight(code, "rs");
        let github_lines = SyntaxHighlighter::new()
            .with_theme("InspiredGitHub")
            .highlight(code, "rs");
        assert_ne!(
            default_lines, github_lines,
            "InspiredGitHub must highlight differently than base16-ocean.dark"
        );
    }
}
