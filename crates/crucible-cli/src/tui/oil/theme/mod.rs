//! Theme system for the TUI.
//!
//! Provides two complementary systems:
//! - [`ThemeTokens`] — runtime color tokens and style presets (current system)
//! - [`ThemeConfig`] — full theme configuration with adaptive colors, icons, layout
//!
//! Use [`active()`] to access the global `ThemeConfig` (initialized lazily with
//! `ThemeConfig::default_dark()`). Use [`set()`] at startup to override.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::oil::theme;
//!
//! let config = theme::active();
//! let color = config.resolve_color(config.colors.error);
//! ```

pub mod tokens;
pub use tokens::ThemeTokens;

pub mod config;
pub use config::*;

pub mod global;
pub use global::{active, is_initialized, set, set_if_unset};

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::style::Color;

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
