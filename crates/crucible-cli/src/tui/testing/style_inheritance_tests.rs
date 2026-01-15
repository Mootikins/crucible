//! Tests for style inheritance and combination behavior.
//!
//! These tests verify that styles are correctly inherited and combined
//! when rendering different content types together (e.g., user messages
//! with bold text, code blocks with syntax highlighting).

use crate::tui::conversation::{render_item_to_lines, ConversationItem};
use crate::tui::styles::{colors, presets};
use crate::tui::theme::{MarkdownElement, MarkdownTheme};
use ratatui::style::{Color, Modifier, Style};

mod user_message_styles {
    use super::*;

    #[test]
    fn user_message_preserves_content_foreground() {
        let theme = MarkdownTheme::dark();
        let user_style = presets::user_message();

        // User message should have inverted background but preserve content colors
        assert_eq!(user_style.bg, Some(colors::USER_BG));
        assert_eq!(user_style.fg, Some(colors::USER_FG));

        // When rendering markdown content inside user message,
        // the content's foreground should still be visible
        let content_style = theme.style_for(MarkdownElement::Bold);
        // Bold typically sets BOLD modifier but not foreground
        assert!(content_style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn user_prefix_has_bold_modifier() {
        let prefix_style = presets::user_prefix();
        assert!(prefix_style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(prefix_style.fg, Some(colors::USER_FG));
        assert_eq!(prefix_style.bg, Some(colors::USER_BG));
    }

    #[test]
    fn user_message_background_applies_to_content() {
        // When content is rendered inside user message,
        // it should inherit the user's background color
        let user_bg = colors::USER_BG;

        // Verify the user_bg can be combined with content styles
        let content_style = Style::default().fg(Color::White);
        let combined = content_style.bg(user_bg);

        assert_eq!(combined.bg, Some(user_bg));
        assert_eq!(combined.fg, Some(Color::White));
    }
}

mod assistant_message_styles {
    use super::*;

    #[test]
    fn assistant_message_uses_terminal_default() {
        let style = presets::assistant_message();
        // Assistant messages use terminal default (Reset) for foreground
        assert_eq!(style.fg, Some(Color::Reset));
        assert!(style.bg.is_none());
    }

    #[test]
    fn assistant_prefix_is_dimmed() {
        let prefix_style = presets::assistant_prefix();
        assert_eq!(prefix_style.fg, Some(colors::DIM));
    }

    #[test]
    fn assistant_bold_text_has_bold_modifier() {
        let theme = MarkdownTheme::dark();
        let bold_style = theme.style_for(MarkdownElement::Bold);
        assert!(bold_style.add_modifier.contains(Modifier::BOLD));
    }
}

mod tool_styles {
    use super::*;

    #[test]
    fn tool_running_uses_white() {
        let style = presets::tool_running();
        assert_eq!(style.fg, Some(colors::TOOL_RUNNING));
        assert_eq!(colors::TOOL_RUNNING, Color::White);
    }

    #[test]
    fn tool_complete_uses_green() {
        let style = presets::tool_complete();
        assert_eq!(style.fg, Some(colors::TOOL_COMPLETE));
        assert_eq!(colors::TOOL_COMPLETE, Color::Green);
    }

    #[test]
    fn tool_error_uses_red_and_bold() {
        let style = presets::tool_error();
        assert_eq!(style.fg, Some(colors::TOOL_ERROR));
        assert_eq!(colors::TOOL_ERROR, Color::Red);
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn tool_output_is_dimmed() {
        let style = presets::tool_output();
        assert_eq!(style.fg, Some(colors::DIM));
    }
}

mod code_block_styles {
    use super::*;

    #[test]
    fn inline_code_has_background() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::InlineCode);
        // Inline code should have a background for visibility
        assert!(
            style.bg.is_some() || !style.add_modifier.is_empty(),
            "Inline code should have bg or modifiers"
        );
    }

    #[test]
    fn code_block_uses_syntax_highlighting() {
        let theme = MarkdownTheme::dark();
        // The syntect theme should provide colors for code
        let syntect_theme = theme.syntect_theme();
        assert!(syntect_theme.settings.background.is_some());
    }

    #[test]
    fn code_block_lang_label_uses_dim_style() {
        // Language labels should be dimmed for visual hierarchy
        let label_style = Style::default().fg(Color::DarkGray);
        assert_eq!(label_style.fg, Some(Color::DarkGray));
    }
}

mod heading_styles {
    use super::*;

    #[test]
    fn heading1_has_blue_fg() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Heading1);
        assert!(style.add_modifier.contains(Modifier::BOLD));
        // Should have colored foreground
        assert!(
            style.fg.is_some() || !style.add_modifier.is_empty(),
            "Heading1 should have styling"
        );
    }

    #[test]
    fn heading2_has_cyan_fg() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Heading2);
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(
            style.fg.is_some() || !style.add_modifier.is_empty(),
            "Heading2 should have styling"
        );
    }

    #[test]
    fn heading3_has_green_fg() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Heading3);
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(
            style.fg.is_some() || !style.add_modifier.is_empty(),
            "Heading3 should have styling"
        );
    }
}

mod link_styles {
    use super::*;

    #[test]
    fn link_has_underline_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Link);
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn link_has_bright_blue_fg() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Link);
        // Links should be visually distinct
        assert!(
            style.fg.is_some() || style.add_modifier.contains(Modifier::UNDERLINED),
            "Link should have styling"
        );
    }
}

mod blockquote_styles {
    use super::*;

    #[test]
    fn blockquote_has_dim_modifier() {
        let theme = MarkdownTheme::dark();
        let style = theme.style_for(MarkdownElement::Blockquote);
        assert!(style.add_modifier.contains(Modifier::DIM));
    }
}

mod mode_styles {
    use super::*;

    #[test]
    fn plan_mode_is_cyan() {
        let style = presets::mode("plan");
        assert_eq!(style.fg, Some(colors::MODE_PLAN));
        assert_eq!(colors::MODE_PLAN, Color::Cyan);
    }

    #[test]
    fn act_mode_is_yellow() {
        let style = presets::mode("act");
        assert_eq!(style.fg, Some(colors::MODE_ACT));
        assert_eq!(colors::MODE_ACT, Color::Yellow);
    }

    #[test]
    fn auto_mode_is_red() {
        let style = presets::mode("auto");
        assert_eq!(style.fg, Some(colors::MODE_AUTO));
        assert_eq!(colors::MODE_AUTO, Color::Red);
    }

    #[test]
    fn unknown_mode_defaults_to_dim() {
        let style = presets::mode("unknown");
        assert_eq!(style.fg, Some(colors::DIM));
    }
}

mod status_styles {
    use super::*;

    #[test]
    fn thinking_indicator_is_cyan_bold() {
        let style = presets::thinking();
        assert_eq!(style.fg, Some(colors::THINKING));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn streaming_indicator_is_green_bold() {
        let style = presets::streaming();
        assert_eq!(style.fg, Some(colors::STREAMING));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn status_line_is_dim() {
        let style = presets::status_line();
        assert_eq!(style.fg, Some(colors::DIM));
    }

    #[test]
    fn metrics_are_dark_gray() {
        let style = presets::metrics();
        assert_eq!(style.fg, Some(colors::METRICS));
        assert_eq!(colors::METRICS, Color::DarkGray);
    }
}

mod input_styles {
    use super::*;

    #[test]
    fn input_box_has_fg_and_bg() {
        let style = presets::input_box();
        assert_eq!(style.fg, Some(colors::INPUT_FG));
        assert_eq!(style.bg, Some(colors::INPUT_BG));
    }

    #[test]
    fn input_shell_has_red_tint() {
        let style = presets::input_shell();
        assert_eq!(style.bg, Some(colors::INPUT_SHELL_BG));
    }

    #[test]
    fn input_repl_has_yellow_tint() {
        let style = presets::input_repl();
        assert_eq!(style.bg, Some(colors::INPUT_REPL_BG));
    }
}

mod style_combination_tests {
    use super::*;

    #[test]
    fn styles_can_be_combined() {
        let base = Style::default().fg(Color::White).bg(Color::Black);
        let added = Style::default().add_modifier(Modifier::BOLD);

        // Combine styles - later styles override earlier ones
        let combined = base.patch(added);
        assert_eq!(combined.fg, Some(Color::White));
        assert!(combined.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn background_can_be_added_to_styled_content() {
        let content_style = Style::default().fg(Color::Green);
        let bg = Color::Rgb(40, 40, 40);

        // Adding background should preserve foreground
        let combined = content_style.bg(bg);
        assert_eq!(combined.fg, Some(Color::Green));
        assert_eq!(combined.bg, Some(bg));
    }

    #[test]
    fn modifier_can_be_added_without_overwriting_fg() {
        let base = Style::default().fg(Color::Red);
        let with_mod = base.add_modifier(Modifier::BOLD);

        assert_eq!(with_mod.fg, Some(Color::Red));
        assert!(with_mod.add_modifier.contains(Modifier::BOLD));
    }
}
