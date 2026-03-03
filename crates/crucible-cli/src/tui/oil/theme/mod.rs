//! Theme system for the TUI.
//!
//! Provides [`ThemeConfig`] — full theme configuration with adaptive colors,
//! icons, layout, loaded from Lua.
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
        let t = ThemeConfig::default_dark();
        assert_ne!(t.colors.error, t.colors.success);
        assert_ne!(t.colors.user_message, t.colors.assistant_message);
        assert_ne!(t.colors.background, t.colors.command_bg);
    }

    #[test]
    fn semantic_colors_resolve_correctly() {
        let t = ThemeConfig::default_dark();

        assert_eq!(t.resolve_color(t.colors.user_message), Color::Green);
        assert_eq!(t.resolve_color(t.colors.error), Color::Rgb(247, 118, 142));
    }

    #[test]
    fn mode_colors_are_set() {
        let t = ThemeConfig::default_dark();
        // Modes should have distinct colors
        assert_ne!(t.colors.mode_normal, t.colors.mode_plan);
        assert_ne!(t.colors.mode_plan, t.colors.mode_auto);
    }

    #[test]
    fn muted_and_dim_are_different() {
        let t = ThemeConfig::default_dark();
        assert_ne!(t.colors.text_muted, t.colors.text_dim);
    }
}
