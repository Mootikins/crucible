//! Syntect-based theme system for markdown rendering.
//!
//! Provides a unified theme system that maps markdown elements to ratatui styles
//! using syntect's TextMate/Sublime theme format as the source of truth.
//!
//! # Usage
//!
//! ```no_run
//! use crucible_cli::tui::theme::{MarkdownTheme, MarkdownElement};
//!
//! // Use auto-detected theme based on terminal background
//! let theme = MarkdownTheme::auto();
//!
//! // Get style for a markdown element
//! let style = theme.style_for(MarkdownElement::Heading1);
//!
//! // Access underlying syntect theme for code highlighting
//! let syntect_theme = theme.syntect_theme();
//! ```

use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{FontStyle, Highlighter, Theme, ThemeSet};
use syntect::parsing::Scope;

/// Markdown elements that can be styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkdownElement {
    /// Normal text (uses theme foreground)
    Text,
    /// Bold text (`**bold**`)
    Bold,
    /// Italic text (`*italic*`)
    Italic,
    /// Bold italic text (`***bold italic***`)
    BoldItalic,
    /// Inline code (`` `code` ``)
    InlineCode,
    /// Level 1 heading (`# Heading`)
    Heading1,
    /// Level 2 heading (`## Heading`)
    Heading2,
    /// Level 3 heading (`### Heading`)
    Heading3,
    /// Level 4 heading (`#### Heading`)
    Heading4,
    /// Level 5 heading (`##### Heading`)
    Heading5,
    /// Level 6 heading (`###### Heading`)
    Heading6,
    /// Link text (`[text](url)`)
    Link,
    /// Blockquote (`> quote`)
    Blockquote,
    /// List bullet/marker (`-`, `*`, `1.`)
    ListMarker,
    /// Table border characters
    TableBorder,
    /// Strikethrough text (`~~strikethrough~~`)
    Strikethrough,
    /// Horizontal rule (`---`)
    HorizontalRule,
}

impl MarkdownElement {
    /// All markdown element variants.
    ///
    /// Useful for iteration, caching, and testing.
    pub const ALL: [Self; 17] = [
        Self::Text,
        Self::Bold,
        Self::Italic,
        Self::BoldItalic,
        Self::InlineCode,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Heading4,
        Self::Heading5,
        Self::Heading6,
        Self::Link,
        Self::Blockquote,
        Self::ListMarker,
        Self::TableBorder,
        Self::Strikethrough,
        Self::HorizontalRule,
    ];
}

/// Scope mapping configuration for a markdown element.
///
/// Defines which syntect scopes to try (in order) and what modifiers to apply.
struct ScopeMapping {
    /// Scopes to try, in order of preference
    scopes: &'static [&'static str],
    /// Additional modifiers to apply on top of the theme style
    modifiers: Modifier,
}

impl MarkdownElement {
    /// Get the scope mapping for this element.
    fn scope_mapping(self) -> ScopeMapping {
        match self {
            Self::Text => ScopeMapping {
                scopes: &[], // Uses theme foreground directly
                modifiers: Modifier::empty(),
            },
            Self::Bold => ScopeMapping {
                scopes: &["markup.bold"],
                modifiers: Modifier::BOLD,
            },
            Self::Italic => ScopeMapping {
                scopes: &["markup.italic"],
                modifiers: Modifier::ITALIC,
            },
            Self::BoldItalic => ScopeMapping {
                scopes: &["markup.bold"],
                modifiers: Modifier::BOLD | Modifier::ITALIC,
            },
            Self::InlineCode => ScopeMapping {
                scopes: &["markup.raw.inline", "markup.raw", "string"],
                modifiers: Modifier::empty(),
            },
            Self::Heading1 => ScopeMapping {
                scopes: &["markup.heading.1", "entity.name.section", "markup.heading"],
                modifiers: Modifier::BOLD,
            },
            Self::Heading2 => ScopeMapping {
                scopes: &["markup.heading.2", "markup.heading", "entity.name.section"],
                modifiers: Modifier::BOLD,
            },
            Self::Heading3 => ScopeMapping {
                scopes: &["markup.heading.3", "markup.heading", "entity.name.section"],
                modifiers: Modifier::BOLD,
            },
            Self::Heading4 => ScopeMapping {
                scopes: &["markup.heading.4", "markup.heading", "entity.name.section"],
                modifiers: Modifier::BOLD,
            },
            Self::Heading5 => ScopeMapping {
                scopes: &["markup.heading.5", "markup.heading", "entity.name.section"],
                modifiers: Modifier::BOLD,
            },
            Self::Heading6 => ScopeMapping {
                scopes: &["markup.heading.6", "markup.heading", "entity.name.section"],
                modifiers: Modifier::BOLD,
            },
            Self::Link => ScopeMapping {
                scopes: &["markup.underline.link", "string.other.link", "string"],
                modifiers: Modifier::UNDERLINED,
            },
            Self::Blockquote => ScopeMapping {
                scopes: &["markup.quote", "comment"],
                modifiers: Modifier::DIM,
            },
            Self::ListMarker => ScopeMapping {
                scopes: &["punctuation.definition.list_item", "punctuation.definition.list", "keyword"],
                modifiers: Modifier::empty(),
            },
            Self::TableBorder => ScopeMapping {
                scopes: &["punctuation", "comment"],
                modifiers: Modifier::DIM,
            },
            Self::Strikethrough => ScopeMapping {
                scopes: &["markup.strikethrough"],
                modifiers: Modifier::CROSSED_OUT,
            },
            Self::HorizontalRule => ScopeMapping {
                scopes: &["punctuation", "comment"],
                modifiers: Modifier::DIM,
            },
        }
    }
}

/// A theme for rendering markdown content in the terminal.
///
/// Wraps a syntect `Theme` and provides ratatui `Style` values for markdown elements.
/// Uses TextMate/Sublime scope selectors to determine colors from the theme.
#[derive(Debug, Clone)]
pub struct MarkdownTheme {
    /// The underlying syntect theme
    theme: Theme,
    /// Cached styles for each markdown element
    style_cache: HashMap<MarkdownElement, Style>,
    /// Whether this is a dark theme
    is_dark: bool,
}

impl MarkdownTheme {
    /// Create a new theme from a syntect `Theme`.
    ///
    /// # Arguments
    ///
    /// * `theme` - The syntect theme to use
    /// * `is_dark` - Whether this is a dark theme (affects fallback colors)
    pub fn new(theme: Theme, is_dark: bool) -> Self {
        let mut md_theme = Self {
            theme,
            style_cache: HashMap::new(),
            is_dark,
        };
        md_theme.build_style_cache();
        md_theme
    }

    /// Create a dark theme using the default "base16-ocean.dark" theme.
    pub fn dark() -> Self {
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.dark"].clone();
        Self::new(theme, true)
    }

    /// Create a light theme using the default "base16-ocean.light" theme.
    pub fn light() -> Self {
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.light"].clone();
        Self::new(theme, false)
    }

    /// Create a theme with auto-detected dark/light mode based on terminal environment.
    ///
    /// Detection order:
    /// 1. `COLORFGBG` env var (format: "fg;bg", bg > 6 = light)
    /// 2. `TERM_BACKGROUND` env var ("dark" | "light")
    /// 3. Default to dark
    pub fn auto() -> Self {
        if Self::detect_dark_background() {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// Load a theme from a `.tmTheme` or `.sublime-color-scheme` file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the theme file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ThemeLoadError> {
        let theme = ThemeSet::get_theme(path.as_ref())
            .map_err(|e| ThemeLoadError::LoadFailed(e.to_string()))?;

        // Detect if theme is dark based on background color
        let is_dark = theme.settings.background
            .map(|bg| {
                // Consider dark if luminance is below 0.5
                let luminance = 0.299 * f64::from(bg.r)
                    + 0.587 * f64::from(bg.g)
                    + 0.114 * f64::from(bg.b);
                luminance < 128.0
            })
            .unwrap_or(true);

        Ok(Self::new(theme, is_dark))
    }

    /// Get the ratatui `Style` for a markdown element.
    pub fn style_for(&self, element: MarkdownElement) -> Style {
        self.style_cache
            .get(&element)
            .copied()
            .unwrap_or_else(|| self.compute_style(element))
    }

    /// Get a reference to the underlying syntect `Theme`.
    ///
    /// Useful for syntax highlighting code blocks.
    pub fn syntect_theme(&self) -> &Theme {
        &self.theme
    }

    /// Whether this is a dark theme.
    pub fn is_dark(&self) -> bool {
        self.is_dark
    }

    /// Get the default foreground color from the theme.
    pub fn foreground(&self) -> Color {
        self.theme
            .settings
            .foreground
            .map(syntect_to_ratatui_color)
            .unwrap_or(if self.is_dark { Color::White } else { Color::Black })
    }

    /// Get the default background color from the theme.
    pub fn background(&self) -> Color {
        self.theme
            .settings
            .background
            .map(syntect_to_ratatui_color)
            .unwrap_or(if self.is_dark { Color::Black } else { Color::White })
    }

    /// Detect if the terminal has a dark background.
    ///
    /// Checks in order:
    /// 1. `COLORFGBG` env var (format: "fg;bg", bg > 6 = light)
    /// 2. `TERM_BACKGROUND` env var ("dark" | "light")
    /// 3. Default to dark
    pub fn detect_dark_background() -> bool {
        // Check COLORFGBG first (format: "fg;bg")
        if let Ok(val) = env::var("COLORFGBG") {
            if let Some(bg) = val.split(';').nth(1) {
                if let Ok(bg_num) = bg.parse::<u8>() {
                    return bg_num <= 6; // 0-6 are dark colors in standard palette
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

    /// Build the style cache for all markdown elements.
    fn build_style_cache(&mut self) {
        for element in MarkdownElement::ALL {
            let style = self.compute_style(element);
            self.style_cache.insert(element, style);
        }
    }

    /// Compute the style for a markdown element from the theme.
    fn compute_style(&self, element: MarkdownElement) -> Style {
        let mapping = element.scope_mapping();
        let highlighter = Highlighter::new(&self.theme);

        // Start with the base style from theme settings
        let mut style = Style::default().fg(self.foreground());

        // Try each scope in order until we find a match
        for scope_str in mapping.scopes {
            if let Ok(scope) = Scope::from_str(scope_str) {
                let syntect_style = highlighter.style_for_stack(&[scope]);

                // Check if this scope actually matched (has non-default colors)
                let default_style = highlighter.get_default();
                if syntect_style.foreground != default_style.foreground
                    || syntect_style.background != default_style.background
                    || !syntect_style.font_style.is_empty()
                {
                    style = convert_syntect_style(&syntect_style);
                    break;
                }
            }
        }

        // Apply additional modifiers from the mapping
        if !mapping.modifiers.is_empty() {
            style = style.add_modifier(mapping.modifiers);
        }

        style
    }
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self::auto()
    }
}

/// Error type for theme loading operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ThemeLoadError {
    /// Failed to load or parse the theme file.
    #[error("Failed to load theme: {0}")]
    LoadFailed(String),
}

/// Convert a syntect `Color` to a ratatui `Color`.
fn syntect_to_ratatui_color(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// Convert a syntect `Style` to a ratatui `Style`.
fn convert_syntect_style(syntect_style: &syntect::highlighting::Style) -> Style {
    let mut style = Style::default()
        .fg(syntect_to_ratatui_color(syntect_style.foreground));

    // Only set background if it's not fully transparent
    if syntect_style.background.a > 0 {
        style = style.bg(syntect_to_ratatui_color(syntect_style.background));
    }

    // Convert font style flags
    if syntect_style.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if syntect_style.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if syntect_style.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_loads() {
        let theme = MarkdownTheme::dark();
        assert!(theme.is_dark());

        // Should have a non-default foreground color
        let fg = theme.foreground();
        assert!(matches!(fg, Color::Rgb(_, _, _) | Color::White));
    }

    #[test]
    fn test_light_theme_loads() {
        let theme = MarkdownTheme::light();
        assert!(!theme.is_dark());

        // Should have a non-default foreground color
        let fg = theme.foreground();
        assert!(matches!(fg, Color::Rgb(_, _, _) | Color::Black));
    }

    #[test]
    fn test_auto_theme_loads() {
        // Should not panic
        let _theme = MarkdownTheme::auto();
    }

    #[test]
    fn test_style_for_text() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Text);

        // Text should have a foreground color
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_style_for_bold_has_bold_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Bold);

        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "Bold element should have BOLD modifier"
        );
    }

    #[test]
    fn test_style_for_italic_has_italic_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Italic);

        assert!(
            style.add_modifier.contains(Modifier::ITALIC),
            "Italic element should have ITALIC modifier"
        );
    }

    #[test]
    fn test_style_for_bold_italic_has_both_modifiers() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::BoldItalic);

        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "BoldItalic should have BOLD modifier"
        );
        assert!(
            style.add_modifier.contains(Modifier::ITALIC),
            "BoldItalic should have ITALIC modifier"
        );
    }

    #[test]
    fn test_style_for_link_has_underline_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Link);

        assert!(
            style.add_modifier.contains(Modifier::UNDERLINED),
            "Link element should have UNDERLINED modifier"
        );
    }

    #[test]
    fn test_style_for_blockquote_has_dim_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Blockquote);

        assert!(
            style.add_modifier.contains(Modifier::DIM),
            "Blockquote element should have DIM modifier"
        );
    }

    #[test]
    fn test_style_for_heading1_has_bold_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Heading1);

        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "Heading1 element should have BOLD modifier"
        );
    }

    #[test]
    fn test_style_for_strikethrough_has_crossed_out_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Strikethrough);

        assert!(
            style.add_modifier.contains(Modifier::CROSSED_OUT),
            "Strikethrough element should have CROSSED_OUT modifier"
        );
    }

    #[test]
    fn test_style_for_table_border_has_dim_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::TableBorder);

        assert!(
            style.add_modifier.contains(Modifier::DIM),
            "TableBorder element should have DIM modifier"
        );
    }

    #[test]
    fn test_syntect_theme_returns_reference() {
        let theme = MarkdownTheme::dark();
        let syntect_theme = theme.syntect_theme();

        // Should be able to use the theme for highlighting
        assert!(syntect_theme.settings.background.is_some());
    }

    #[test]
    fn test_all_elements_have_cached_styles() {
        let theme = MarkdownTheme::dark();

        // All elements should return a style without panicking
        for element in MarkdownElement::ALL {
            let style = theme.style_for(element);
            // All elements should have at least a foreground color set
            assert!(
                style.fg.is_some() || !style.add_modifier.is_empty(),
                "{element:?} should have either fg color or modifiers"
            );
        }
    }

    #[test]
    fn test_syntect_color_conversion() {
        let syntect_color = syntect::highlighting::Color {
            r: 255,
            g: 128,
            b: 64,
            a: 255,
        };
        let ratatui_color = syntect_to_ratatui_color(syntect_color);

        assert_eq!(ratatui_color, Color::Rgb(255, 128, 64));
    }

    #[test]
    fn test_default_is_auto() {
        // Default should not panic and should return a theme
        let theme = MarkdownTheme::default();
        let _style = theme.style_for(MarkdownElement::Text);
    }

    #[test]
    fn test_foreground_and_background_colors() {
        let dark_theme = MarkdownTheme::dark();
        let light_theme = MarkdownTheme::light();

        // Both themes should have foreground and background
        let _dark_fg = dark_theme.foreground();
        let _dark_bg = dark_theme.background();
        let _light_fg = light_theme.foreground();
        let _light_bg = light_theme.background();
    }
}
