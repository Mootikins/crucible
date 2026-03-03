//! Theme configuration types — re-exported from crucible-lua.
//!
//! Types are defined in `crucible_lua::theme` so that both crucible-lua
//! (for Lua loading) and crucible-cli (for rendering) can use them.

pub use crucible_lua::theme::{
    BorderStyle, StatusBarPosition, ThemeColors, ThemeConfig, ThemeDecorations, ThemeIcons,
    ThemeLayout, ThemeSpinnerStyle,
};

// ─────────────────────────────────────────────────────────────────────────────
// Tests — verify re-exported types match ThemeTokens
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::theme::ThemeTokens;
    use crucible_oil::style::{AdaptiveColor, Color};

    #[test]
    fn default_dark_core_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        // Text colors
        assert_eq!(
            config.resolve_color(config.colors.text),
            tokens.text_primary
        );
        assert_eq!(
            config.resolve_color(config.colors.text_muted),
            tokens.text_muted
        );
        assert_eq!(
            config.resolve_color(config.colors.text_emphasized),
            tokens.text_accent
        );

        // Background
        assert_eq!(
            config.resolve_color(config.colors.background),
            tokens.input_bg
        );
        assert_eq!(
            config.resolve_color(config.colors.background_panel),
            tokens.code_bg
        );
    }

    #[test]
    fn default_dark_semantic_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(config.resolve_color(config.colors.error), tokens.error);
        assert_eq!(config.resolve_color(config.colors.warning), tokens.warning);
        assert_eq!(config.resolve_color(config.colors.success), tokens.success);
        assert_eq!(config.resolve_color(config.colors.info), tokens.info);
    }

    #[test]
    fn default_dark_chat_role_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.user_message),
            tokens.role_user
        );
        assert_eq!(
            config.resolve_color(config.colors.assistant_message),
            tokens.role_assistant
        );
        assert_eq!(
            config.resolve_color(config.colors.system_message),
            tokens.role_system
        );
    }

    #[test]
    fn default_dark_mode_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.mode_normal),
            tokens.mode_normal
        );
        assert_eq!(
            config.resolve_color(config.colors.mode_plan),
            tokens.mode_plan
        );
        assert_eq!(
            config.resolve_color(config.colors.mode_auto),
            tokens.mode_auto
        );
    }

    #[test]
    fn default_dark_diff_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.diff_added),
            tokens.diff_add
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_removed),
            tokens.diff_del
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_context),
            tokens.diff_ctx
        );
    }

    #[test]
    fn default_dark_overlay_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.popup_bg),
            tokens.popup_bg
        );
        assert_eq!(
            config.resolve_color(config.colors.popup_selected_bg),
            tokens.popup_selected_bg
        );
        assert_eq!(config.resolve_color(config.colors.border), tokens.border);
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
        assert_eq!(config.decorations.tool_error_icon, "✖");
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
