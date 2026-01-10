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

use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{Theme, ThemeSet};

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
        let is_dark = theme
            .settings
            .background
            .map(|bg| {
                // Consider dark if luminance is below 0.5
                let luminance =
                    0.299 * f64::from(bg.r) + 0.587 * f64::from(bg.g) + 0.114 * f64::from(bg.b);
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
            .unwrap_or(if self.is_dark {
                Color::White
            } else {
                Color::Black
            })
    }

    /// Get the default background color from the theme.
    pub fn background(&self) -> Color {
        self.theme
            .settings
            .background
            .map(syntect_to_ratatui_color)
            .unwrap_or(if self.is_dark {
                Color::Black
            } else {
                Color::White
            })
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

    /// Compute the style for a markdown element.
    ///
    /// Uses ANSI indexed colors (0-15) so styles inherit from terminal theme.
    /// This ensures consistent appearance across different terminal color schemes.
    fn compute_style(&self, element: MarkdownElement) -> Style {
        use MarkdownElement::*;

        // ANSI color indices (use terminal's palette):
        // 0=black, 1=red, 2=green, 3=yellow, 4=blue, 5=magenta, 6=cyan, 7=white
        // 8-15 = bright versions
        match element {
            Text => Style::default(), // Use terminal default

            Bold => Style::default().add_modifier(Modifier::BOLD),

            Italic => Style::default().add_modifier(Modifier::ITALIC),

            BoldItalic => Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::ITALIC),

            InlineCode => {
                let bg = Color::Indexed(if self.is_dark { 8 } else { 7 }); // Bright black or white
                Style::default().bg(bg)
            }

            Heading1 => Style::default()
                .fg(Color::Indexed(4)) // Blue
                .add_modifier(Modifier::BOLD),

            Heading2 => Style::default()
                .fg(Color::Indexed(6)) // Cyan
                .add_modifier(Modifier::BOLD),

            Heading3 => Style::default()
                .fg(Color::Indexed(2)) // Green
                .add_modifier(Modifier::BOLD),

            Heading4 | Heading5 | Heading6 => Style::default()
                .fg(Color::Indexed(3)) // Yellow
                .add_modifier(Modifier::BOLD),

            Link => Style::default()
                .fg(Color::Indexed(12)) // Bright blue
                .add_modifier(Modifier::UNDERLINED),

            Blockquote => Style::default()
                .fg(Color::Indexed(8)) // Bright black (gray)
                .add_modifier(Modifier::DIM),

            ListMarker => Style::default().fg(Color::Indexed(6)), // Cyan

            TableBorder => Style::default()
                .fg(Color::Indexed(8)) // Bright black (gray)
                .add_modifier(Modifier::DIM),

            Strikethrough => Style::default()
                .add_modifier(Modifier::CROSSED_OUT)
                .add_modifier(Modifier::DIM),

            HorizontalRule => Style::default()
                .fg(Color::Indexed(8)) // Bright black (gray)
                .add_modifier(Modifier::DIM),
        }
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

        // Text uses terminal default (no explicit fg) for terminal theme inheritance
        assert!(style.fg.is_none());
        assert!(style.add_modifier.is_empty());
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
            // All elements except Text should have fg or modifiers
            // Text uses terminal default for theme inheritance
            if element != MarkdownElement::Text {
                assert!(
                    style.fg.is_some() || style.bg.is_some() || !style.add_modifier.is_empty(),
                    "{element:?} should have fg, bg, or modifiers"
                );
            }
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
