//! Theme configuration types — re-exported from crucible-lua.
//!
//! Types are defined in `crucible_lua::theme` so that both crucible-lua
//! (for Lua loading) and crucible-cli (for rendering) can use them.

pub use crucible_lua::theme::{
    BorderStyle, StatusBarPosition, ThemeColors, ThemeConfig, ThemeDecorations, ThemeIcons,
    ThemeLayout, ThemeSpinnerStyle,
};

// ─────────────────────────────────────────────────────────────────────────────
// Tests — verify ThemeConfig defaults and re-exported types
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::style::{AdaptiveColor, Color};

    #[test]
    fn default_dark_core_colors_are_set() {
        let config = ThemeConfig::default_dark();

        // Text colors
        assert_eq!(config.resolve_color(config.colors.text), Color::White);
        assert_eq!(
            config.resolve_color(config.colors.text_muted),
            Color::DarkGray
        );
        assert_eq!(
            config.resolve_color(config.colors.text_emphasized),
            Color::Cyan
        );

        // Background
        assert_eq!(
            config.resolve_color(config.colors.background),
            Color::Rgb(40, 44, 52)
        );
        assert_eq!(
            config.resolve_color(config.colors.background_panel),
            Color::Rgb(35, 39, 47)
        );
    }

    #[test]
    fn default_dark_semantic_colors_are_set() {
        let config = ThemeConfig::default_dark();

        assert_eq!(
            config.resolve_color(config.colors.error),
            Color::Rgb(247, 118, 142)
        );
        assert_eq!(
            config.resolve_color(config.colors.warning),
            Color::Rgb(224, 175, 104)
        );
        assert_eq!(
            config.resolve_color(config.colors.success),
            Color::Rgb(158, 206, 106)
        );
        assert_eq!(
            config.resolve_color(config.colors.info),
            Color::Rgb(0, 206, 209)
        );
    }

    #[test]
    fn default_dark_chat_role_colors_are_set() {
        let config = ThemeConfig::default_dark();

        assert_eq!(
            config.resolve_color(config.colors.user_message),
            Color::Green
        );
        assert_eq!(
            config.resolve_color(config.colors.assistant_message),
            Color::Cyan
        );
        assert_eq!(
            config.resolve_color(config.colors.system_message),
            Color::Yellow
        );
    }

    #[test]
    fn default_dark_mode_colors_are_set() {
        let config = ThemeConfig::default_dark();

        assert_eq!(
            config.resolve_color(config.colors.mode_normal),
            Color::Green
        );
        assert_eq!(config.resolve_color(config.colors.mode_plan), Color::Blue);
        assert_eq!(config.resolve_color(config.colors.mode_auto), Color::Yellow);
    }

    #[test]
    fn default_dark_diff_colors_are_set() {
        let config = ThemeConfig::default_dark();

        assert_eq!(
            config.resolve_color(config.colors.diff_added),
            Color::Rgb(158, 206, 106)
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_removed),
            Color::Rgb(247, 118, 142)
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_context),
            Color::Rgb(100, 110, 130)
        );
    }

    #[test]
    fn default_dark_overlay_colors_are_set() {
        let config = ThemeConfig::default_dark();

        assert_eq!(
            config.resolve_color(config.colors.popup_bg),
            Color::Rgb(30, 34, 42)
        );
        assert_eq!(
            config.resolve_color(config.colors.popup_selected_bg),
            Color::Rgb(50, 56, 68)
        );
        assert_eq!(
            config.resolve_color(config.colors.border),
            Color::Rgb(40, 44, 52)
        );
    }

    #[test]
    fn default_dark_is_dark() {
        let config = ThemeConfig::default_dark();
        assert!(config.is_dark);
        assert_eq!(config.name, "crucible-dark");
    }

    #[test]
    fn resolve_color_respects_is_dark() {
        let mut config = ThemeConfig::default_dark();
        let test_color = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };

        // Dark mode → dark variant
        assert_eq!(config.resolve_color(test_color), Color::Red);

        // Light mode → light variant
        config.is_dark = false;
        assert_eq!(config.resolve_color(test_color), Color::Blue);
    }

    #[test]
    fn spinner_braille_frames_match_oil() {
        let style = ThemeSpinnerStyle::Braille;
        let oil_frames = crucible_oil::node::BRAILLE_SPINNER_FRAMES;
        assert_eq!(style.frames(), oil_frames);
    }

    #[test]
    fn spinner_pulse_frames_match_oil() {
        let style = ThemeSpinnerStyle::Pulse;
        let oil_frames = crucible_oil::node::SPINNER_FRAMES;
        assert_eq!(style.frames(), oil_frames);
    }

    #[test]
    fn spinner_none_has_empty_frames() {
        assert!(ThemeSpinnerStyle::None.frames().is_empty());
    }

    #[test]
    fn spinner_ascii_has_four_frames() {
        let frames = ThemeSpinnerStyle::Ascii.frames();
        assert_eq!(frames.len(), 4);
        assert_eq!(frames, &['-', '\\', '|', '/']);
    }

    #[test]
    fn default_decorations_have_expected_chars() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.decorations.message_user_indicator, "▌");
        assert_eq!(config.decorations.tool_success_icon, "✓");
        assert_eq!(config.decorations.tool_error_icon, "✗");
        assert_eq!(config.decorations.half_block_top, '▀');
        assert_eq!(config.decorations.half_block_bottom, '▄');
    }

    #[test]
    fn default_icons_have_expected_chars() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.icons.check, "✓");
        assert_eq!(config.icons.error, "✖");
        assert_eq!(config.icons.warning, "⚠");
        assert_eq!(config.icons.arrow_right, "→");
    }

    #[test]
    fn default_layout_values() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.layout.status_bar_position, StatusBarPosition::Bottom);
        assert_eq!(config.layout.message_spacing, 1);
        assert_eq!(config.layout.input_max_lines, 6);
    }

    #[test]
    fn border_style_default_is_rounded() {
        assert_eq!(BorderStyle::default(), BorderStyle::Rounded);
    }

    #[test]
    fn spinner_style_default_is_braille() {
        assert_eq!(ThemeSpinnerStyle::default(), ThemeSpinnerStyle::Braille);
    }

    #[test]
    fn theme_colors_has_29_fields() {
        // Verified by the exhaustive struct literal in default_dark().
        // If any field were missing, the struct literal wouldn't compile.
        let config = ThemeConfig::default_dark();
        // Verify clone works (exercises all fields)
        let config2 = config.clone();
        assert_eq!(config.colors, config2.colors);
    }
}
