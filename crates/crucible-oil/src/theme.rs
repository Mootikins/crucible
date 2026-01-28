//! Semantic color theme for Oil rendering
//!
//! Provides a centralized theme system for consistent colors across the UI.
//! Themes define semantic color roles (e.g., "error", "success") that can be
//! easily swapped for dark/light mode or custom color schemes.

use crate::style::Color;

/// A complete color theme for Oil rendering
///
/// Defines semantic colors for text, UI elements, and message types.
/// Use `Theme::default()` for the standard theme or create custom themes
/// by constructing directly or using builder methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_dim: Color,

    // UI element colors
    pub border: Color,
    pub background: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,

    // Semantic colors
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,

    // Role-specific message colors
    pub user_message: Color,
    pub assistant_message: Color,
    pub system_message: Color,
}

impl Theme {
    /// Create a new theme with all colors specified
    pub fn new(
        text_primary: Color,
        text_secondary: Color,
        text_dim: Color,
        border: Color,
        background: Color,
        selection_bg: Color,
        selection_fg: Color,
        success: Color,
        error: Color,
        warning: Color,
        info: Color,
        user_message: Color,
        assistant_message: Color,
        system_message: Color,
    ) -> Self {
        Self {
            text_primary,
            text_secondary,
            text_dim,
            border,
            background,
            selection_bg,
            selection_fg,
            success,
            error,
            warning,
            info,
            user_message,
            assistant_message,
            system_message,
        }
    }

    /// Create a dark theme (default)
    ///
    /// Uses colors optimized for dark terminal backgrounds.
    pub fn dark() -> Self {
        Self {
            // Text colors
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_dim: Color::DarkGray,

            // UI element colors
            border: Color::Gray,
            background: Color::Black,
            selection_bg: Color::Rgb(60, 70, 90),
            selection_fg: Color::White,

            // Semantic colors
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Cyan,

            // Role-specific message colors
            user_message: Color::Blue,
            assistant_message: Color::Green,
            system_message: Color::Gray,
        }
    }

    /// Create a light theme
    ///
    /// Uses colors optimized for light terminal backgrounds.
    pub fn light() -> Self {
        Self {
            // Text colors
            text_primary: Color::Black,
            text_secondary: Color::DarkGray,
            text_dim: Color::Gray,

            // UI element colors
            border: Color::DarkGray,
            background: Color::White,
            selection_bg: Color::Rgb(200, 210, 230),
            selection_fg: Color::Black,

            // Semantic colors
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,

            // Role-specific message colors
            user_message: Color::Blue,
            assistant_message: Color::Green,
            system_message: Color::DarkGray,
        }
    }

    /// Create a high-contrast theme
    ///
    /// Uses bold colors for maximum visibility.
    pub fn high_contrast() -> Self {
        Self {
            // Text colors
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_dim: Color::DarkGray,

            // UI element colors
            border: Color::White,
            background: Color::Black,
            selection_bg: Color::White,
            selection_fg: Color::Black,

            // Semantic colors
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Cyan,

            // Role-specific message colors
            user_message: Color::Cyan,
            assistant_message: Color::Green,
            system_message: Color::White,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert_eq!(theme, Theme::dark());
    }

    #[test]
    fn test_theme_dark() {
        let theme = Theme::dark();
        assert_eq!(theme.text_primary, Color::White);
        assert_eq!(theme.background, Color::Black);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
    }

    #[test]
    fn test_theme_light() {
        let theme = Theme::light();
        assert_eq!(theme.text_primary, Color::Black);
        assert_eq!(theme.background, Color::White);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
    }

    #[test]
    fn test_theme_high_contrast() {
        let theme = Theme::high_contrast();
        assert_eq!(theme.text_primary, Color::White);
        assert_eq!(theme.background, Color::Black);
        assert_eq!(theme.selection_bg, Color::White);
        assert_eq!(theme.selection_fg, Color::Black);
    }

    #[test]
    fn test_theme_new() {
        let theme = Theme::new(
            Color::White,
            Color::Gray,
            Color::DarkGray,
            Color::Gray,
            Color::Black,
            Color::Blue,
            Color::White,
            Color::Green,
            Color::Red,
            Color::Yellow,
            Color::Cyan,
            Color::Blue,
            Color::Green,
            Color::Gray,
        );

        assert_eq!(theme.text_primary, Color::White);
        assert_eq!(theme.text_secondary, Color::Gray);
        assert_eq!(theme.text_dim, Color::DarkGray);
        assert_eq!(theme.border, Color::Gray);
        assert_eq!(theme.background, Color::Black);
        assert_eq!(theme.selection_bg, Color::Blue);
        assert_eq!(theme.selection_fg, Color::White);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
        assert_eq!(theme.warning, Color::Yellow);
        assert_eq!(theme.info, Color::Cyan);
        assert_eq!(theme.user_message, Color::Blue);
        assert_eq!(theme.assistant_message, Color::Green);
        assert_eq!(theme.system_message, Color::Gray);
    }

    #[test]
    fn test_theme_clone() {
        let theme1 = Theme::dark();
        let theme2 = theme1;
        assert_eq!(theme1, theme2);
    }

    #[test]
    fn test_theme_semantic_colors() {
        let theme = Theme::default();
        // Verify all semantic colors are set
        assert_ne!(theme.success, Color::Reset);
        assert_ne!(theme.error, Color::Reset);
        assert_ne!(theme.warning, Color::Reset);
        assert_ne!(theme.info, Color::Reset);
    }

    #[test]
    fn test_theme_message_colors() {
        let theme = Theme::default();
        // Verify all message role colors are set
        assert_ne!(theme.user_message, Color::Reset);
        assert_ne!(theme.assistant_message, Color::Reset);
        assert_ne!(theme.system_message, Color::Reset);
    }
}
