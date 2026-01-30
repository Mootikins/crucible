//! Semantic color tokens and style presets for the TUI.
//!
//! All colors and styles are accessed via [`ThemeTokens`], which provides
//! runtime theme tokens for the chat interface. Use `ThemeTokens::default_ref()`
//! for a zero-cost static reference to the default dark theme.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::oil::theme::ThemeTokens;
//!
//! let theme = ThemeTokens::default_ref();
//! styled("Error!", theme.error_style());
//! styled("Hello", theme.user_prompt());
//! ```

pub mod tokens;
pub use tokens::ThemeTokens;

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::style::{Color, Style};

    #[test]
    fn color_tokens_are_distinct() {
        let t = ThemeTokens::default_ref();
        assert_ne!(t.error, t.success);
        assert_ne!(t.role_user, t.role_assistant);
        assert_ne!(t.input_bg, t.command_bg);
    }

    #[test]
    fn style_presets_build_correctly() {
        let t = ThemeTokens::default_ref();

        let user = t.user_prompt();
        assert_eq!(user.fg, Some(Color::Green));
        assert!(user.bold);

        let err = t.error_style();
        assert_eq!(err.fg, Some(Color::Rgb(247, 118, 142)));
        assert!(err.bold);
    }

    #[test]
    fn mode_styles_have_contrasting_fg() {
        let t = ThemeTokens::default_ref();
        assert_eq!(t.mode_normal_style().fg, Some(Color::Black));
        assert_eq!(t.mode_plan_style().fg, Some(Color::Black));
        assert_eq!(t.mode_auto_style().fg, Some(Color::Black));
    }

    #[test]
    fn muted_and_dim_are_different() {
        let t = ThemeTokens::default_ref();
        let muted = t.muted();
        let dim = t.dim();

        assert!(!muted.dim);
        assert!(dim.dim);
    }
}
