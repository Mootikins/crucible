//! Component isolation tests for correct rendering (spacing, ANSI, layout)
//!
//! These tests verify that individual components render correctly in isolation,
//! checking both structural output (plain text) and styled output (ANSI codes).

use crate::tui::oil::ansi::{strip_ansi, visible_width};
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::ChatMode;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{
    popup_item, popup_item_with_desc, InputArea, PopupOverlay, StatusBar, INPUT_MAX_CONTENT_LINES,
};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::node::{col, row, spacer, styled, text, PopupItemNode};
use crate::tui::oil::render::{render_to_plain_text, render_to_string, render_with_cursor};
use crate::tui::oil::style::{Color, Style};
use insta::assert_snapshot;

fn render_plain(component: &impl Component, width: usize) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let node = component.view(&ctx);
    render_to_plain_text(&node, width)
}

fn render_ansi(component: &impl Component, width: usize) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let node = component.view(&ctx);
    render_to_string(&node, width)
}

fn has_ansi_codes(s: &str) -> bool {
    s.contains("\x1b[")
}

fn assert_fits_width(output: &str, max_width: usize) {
    for (i, line) in output.lines().enumerate() {
        let width = visible_width(line);
        assert!(
            width <= max_width,
            "Line {} exceeds width {}: got {} chars: {:?}",
            i,
            max_width,
            width,
            strip_ansi(line)
        );
    }
}

mod status_bar_tests {
    use super::*;

    #[test]
    fn renders_mode_label_at_start() {
        let bar = StatusBar::new().mode(ChatMode::Normal);
        let plain = render_plain(&bar, 80);

        assert!(
            plain.starts_with(" NORMAL "),
            "Mode label should be at start with padding: {:?}",
            plain
        );
    }

    #[test]
    fn mode_labels_have_consistent_padding() {
        let normal = StatusBar::new().mode(ChatMode::Normal);
        let plan = StatusBar::new().mode(ChatMode::Plan);
        let auto = StatusBar::new().mode(ChatMode::Auto);

        let normal_plain = render_plain(&normal, 80);
        let plan_plain = render_plain(&plan, 80);
        let auto_plain = render_plain(&auto, 80);

        // All mode labels should have space padding
        assert!(normal_plain.contains(" NORMAL "));
        assert!(plan_plain.contains(" PLAN "));
        assert!(auto_plain.contains(" AUTO "));
    }

    #[test]
    fn ansi_output_has_color_codes() {
        let bar = StatusBar::new().mode(ChatMode::Normal).model("gpt-4o");
        let ansi = render_ansi(&bar, 80);

        assert!(
            has_ansi_codes(&ansi),
            "StatusBar should have ANSI color codes"
        );
    }

    #[test]
    fn different_modes_have_different_colors() {
        let normal = StatusBar::new().mode(ChatMode::Normal);
        let plan = StatusBar::new().mode(ChatMode::Plan);

        let normal_ansi = render_ansi(&normal, 80);
        let plan_ansi = render_ansi(&plan, 80);

        // The raw ANSI should differ (different background colors)
        assert_ne!(
            normal_ansi, plan_ansi,
            "Different modes should have different ANSI codes"
        );
    }

    #[test]
    fn model_name_appears_after_mode() {
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .model("claude-3-opus");
        let plain = render_plain(&bar, 80);

        let mode_pos = plain.find("NORMAL").expect("mode should exist");
        let model_pos = plain.find("claude-3-opus").expect("model should exist");

        assert!(model_pos > mode_pos, "Model should appear after mode label");
    }

    #[test]
    fn context_percentage_formatted_correctly() {
        let bar = StatusBar::new().context(32000, 128000);
        let plain = render_plain(&bar, 80);

        assert!(
            plain.contains("25% ctx"),
            "Context should show percentage: {:?}",
            plain
        );
    }

    #[test]
    fn context_token_count_when_no_total() {
        let bar = StatusBar::new().context(15000, 0);
        let plain = render_plain(&bar, 80);

        assert!(
            plain.contains("15k tok"),
            "Should show token count when no total: {:?}",
            plain
        );
    }

    #[test]
    fn notification_badge_appears_on_right() {
        use crate::tui::oil::components::NotificationToastKind;
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .toast("Processing", NotificationToastKind::Info);
        let plain = render_plain(&bar, 80);

        let mode_pos = plain.find("NORMAL").expect("mode should exist");
        let badge_pos = plain.find("INFO").expect("notification badge should exist");

        assert!(
            badge_pos > mode_pos,
            "Notification badge should appear after mode (on right side)"
        );
    }

    #[test]
    fn fits_width_80() {
        use crate::tui::oil::components::NotificationToastKind;
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .model("claude-3-opus-very-long-name")
            .context(64000, 128000)
            .status("Streaming...")
            .counts(vec![
                (NotificationToastKind::Warning, 3),
                (NotificationToastKind::Error, 1),
            ]);
        let plain = render_plain(&bar, 80);

        assert_fits_width(&plain, 80);
    }

    #[test]
    fn snapshot_normal_mode() {
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .model("gpt-4o-mini")
            .context(10000, 128000);
        assert_snapshot!("status_bar_normal", render_plain(&bar, 80));
    }

    #[test]
    fn snapshot_plan_mode_with_status() {
        let bar = StatusBar::new()
            .mode(ChatMode::Plan)
            .model("claude-3-opus")
            .context(50000, 200000)
            .status("Thinking...");
        assert_snapshot!("status_bar_plan", render_plain(&bar, 80));
    }
}

mod input_area_tests {
    use super::*;

    #[test]
    fn renders_prompt_for_normal_mode() {
        let input = InputArea::new("hello world", 11, 80);
        let plain = render_plain(&input, 80);

        assert!(
            plain.contains(" > "),
            "Normal mode should show '>' prompt: {:?}",
            plain
        );
    }

    #[test]
    fn renders_prompt_for_command_mode() {
        let input = InputArea::new(":set model gpt-4", 16, 80);
        let plain = render_plain(&input, 80);

        assert!(
            plain.contains(" : "),
            "Command mode should show ':' prompt: {:?}",
            plain
        );
        // Content should NOT include the leading ':'
        assert!(plain.contains("set model"));
    }

    #[test]
    fn renders_prompt_for_shell_mode() {
        let input = InputArea::new("!ls -la", 7, 80);
        let plain = render_plain(&input, 80);

        assert!(
            plain.contains(" ! "),
            "Shell mode should show '!' prompt: {:?}",
            plain
        );
        // Content should NOT include the leading '!'
        assert!(plain.contains("ls -la"));
    }

    #[test]
    fn has_top_and_bottom_edges() {
        let input = InputArea::new("test", 4, 40);
        let plain = render_plain(&input, 40);
        let lines: Vec<&str> = plain.lines().collect();

        // Should have at least 3 lines: top edge, content, bottom edge
        assert!(
            lines.len() >= 3,
            "Input should have top edge, content, bottom edge: {:?}",
            lines
        );

        // Top edge should be solid blocks
        assert!(
            lines[0].contains('▄'),
            "Top edge should have ▄ characters: {:?}",
            lines[0]
        );

        // Bottom edge should be solid blocks
        let last = lines.last().unwrap();
        assert!(
            last.contains('▀'),
            "Bottom edge should have ▀ characters: {:?}",
            last
        );
    }

    #[test]
    fn different_modes_have_different_colors() {
        let normal = InputArea::new("hello", 5, 80);
        let command = InputArea::new(":help", 5, 80);
        let shell = InputArea::new("!pwd", 4, 80);

        let normal_ansi = render_ansi(&normal, 80);
        let command_ansi = render_ansi(&command, 80);
        let shell_ansi = render_ansi(&shell, 80);

        // All should have ANSI codes
        assert!(has_ansi_codes(&normal_ansi));
        assert!(has_ansi_codes(&command_ansi));
        assert!(has_ansi_codes(&shell_ansi));

        // And they should differ
        assert_ne!(normal_ansi, command_ansi);
        assert_ne!(command_ansi, shell_ansi);
    }

    #[test]
    fn content_wraps_at_width() {
        let long_text = "a".repeat(150);
        let input = InputArea::new(&long_text, 150, 80);
        let plain = render_plain(&input, 80);

        // Should have multiple content lines (plus edges)
        let lines: Vec<&str> = plain.lines().collect();
        assert!(
            lines.len() > 3,
            "Long content should wrap to multiple lines"
        );
    }

    #[test]
    fn respects_max_content_lines() {
        let very_long = "x".repeat(1000);
        let input = InputArea::new(&very_long, 1000, 80);
        let plain = render_plain(&input, 80);
        let lines: Vec<&str> = plain.lines().collect();

        // Total lines should be bounded: edges (2) + max content lines
        let max_total = 2 + INPUT_MAX_CONTENT_LINES;
        assert!(
            lines.len() <= max_total,
            "Should not exceed max lines ({}): got {}",
            max_total,
            lines.len()
        );
    }

    #[test]
    fn fits_width() {
        let input = InputArea::new("Test input with some content", 28, 60);
        let plain = render_plain(&input, 60);

        assert_fits_width(&plain, 60);
    }

    #[test]
    fn cursor_tracking_works() {
        let input = InputArea::new("hello", 3, 80).set_focused(true);
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let node = input.view(&ctx);

        let result = render_with_cursor(&node, 80);
        assert!(
            result.cursor.visible,
            "Cursor should be visible when focused"
        );
    }

    #[test]
    fn snapshot_empty_input() {
        let input = InputArea::new("", 0, 80);
        assert_snapshot!("input_area_empty", render_plain(&input, 80));
    }

    #[test]
    fn snapshot_command_mode() {
        let input = InputArea::new(":set thinking on", 16, 80);
        assert_snapshot!("input_area_command", render_plain(&input, 80));
    }

    #[test]
    fn snapshot_shell_mode() {
        let input = InputArea::new("!cargo test", 11, 80);
        assert_snapshot!("input_area_shell", render_plain(&input, 80));
    }
}

mod popup_overlay_tests {
    use super::*;

    fn sample_items() -> Vec<PopupItemNode> {
        vec![
            popup_item("Option A"),
            popup_item("Option B"),
            popup_item("Option C"),
        ]
    }

    fn items_with_descriptions() -> Vec<PopupItemNode> {
        vec![
            popup_item_with_desc("model", "Switch AI model"),
            popup_item_with_desc("theme", "Change color theme"),
            popup_item_with_desc("verbose", "Toggle verbose output"),
        ]
    }

    #[test]
    fn hidden_popup_returns_empty() {
        let popup = PopupOverlay::new(sample_items()).visible(false);
        let plain = render_plain(&popup, 80);

        assert!(
            plain.is_empty(),
            "Hidden popup should render nothing: {:?}",
            plain
        );
    }

    #[test]
    fn empty_items_returns_empty() {
        let popup = PopupOverlay::new(vec![]);
        let plain = render_plain(&popup, 80);

        assert!(
            plain.is_empty(),
            "Empty popup should render nothing: {:?}",
            plain
        );
    }

    #[test]
    fn shows_all_items() {
        let popup = PopupOverlay::new(sample_items());
        let plain = render_plain(&popup, 80);

        assert!(plain.contains("Option A"));
        assert!(plain.contains("Option B"));
        assert!(plain.contains("Option C"));
    }

    #[test]
    fn selected_item_has_indicator() {
        let popup = PopupOverlay::new(sample_items()).selected(1);
        let plain = render_plain(&popup, 80);

        // Find lines and check indicator
        for line in plain.lines() {
            if line.contains("Option B") {
                assert!(
                    line.contains("▸"),
                    "Selected item should have ▸ indicator: {:?}",
                    line
                );
            }
        }
    }

    #[test]
    fn unselected_items_no_indicator() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        let plain = render_plain(&popup, 80);

        for line in plain.lines() {
            if line.contains("Option B") || line.contains("Option C") {
                assert!(
                    !line.contains("▸"),
                    "Unselected items should not have ▸: {:?}",
                    line
                );
            }
        }
    }

    #[test]
    fn has_ansi_background_colors() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        let ansi = render_ansi(&popup, 80);

        assert!(
            has_ansi_codes(&ansi),
            "Popup should have ANSI codes for background"
        );
    }

    #[test]
    fn selected_has_different_background() {
        let popup = PopupOverlay::new(sample_items()).selected(1);
        let ansi = render_ansi(&popup, 80);

        let lines: Vec<&str> = ansi.lines().filter(|l| !l.is_empty()).collect();
        if lines.len() >= 2 {
            assert_ne!(
                lines[0], lines[1],
                "Selected line should differ from others"
            );
        }
    }

    #[test]
    fn descriptions_rendered_when_space() {
        let popup = PopupOverlay::new(items_with_descriptions());
        let plain = render_plain(&popup, 80);

        assert!(
            plain.contains("Switch AI model"),
            "Description should appear: {:?}",
            plain
        );
    }

    #[test]
    fn truncates_long_labels() {
        let items = vec![popup_item(
            "This is a very long option label that should be truncated",
        )];
        let popup = PopupOverlay::new(items);
        let plain = render_plain(&popup, 40);

        // Should contain ellipsis
        assert!(
            plain.contains("…"),
            "Long labels should be truncated with ellipsis"
        );
    }

    #[test]
    fn respects_max_visible() {
        let items: Vec<_> = (0..20).map(|i| popup_item(format!("Item {}", i))).collect();
        let popup = PopupOverlay::new(items).max_visible(5);
        let plain = render_plain(&popup, 80);

        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines.len(), 5, "Should show exactly max_visible lines");
    }

    #[test]
    fn fits_width() {
        let popup = PopupOverlay::new(items_with_descriptions()).max_visible(5);
        let plain = render_plain(&popup, 60);

        assert_fits_width(&plain, 60);
    }

    #[test]
    fn line_count_equals_max_visible() {
        let popup = PopupOverlay::new(sample_items()).max_visible(10);
        let plain = render_plain(&popup, 80);

        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(
            lines.len(),
            10,
            "Popup always renders max_visible lines (with padding)"
        );

        let non_empty: Vec<_> = lines.iter().filter(|l| !l.is_empty()).collect();
        assert_eq!(non_empty.len(), 3, "Should have 3 item lines");
    }

    #[test]
    fn snapshot_basic_popup() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        assert_snapshot!("popup_basic", render_plain(&popup, 80));
    }

    #[test]
    fn snapshot_popup_with_descriptions() {
        let popup = PopupOverlay::new(items_with_descriptions()).selected(1);
        assert_snapshot!("popup_with_descriptions", render_plain(&popup, 80));
    }

    #[test]
    fn snapshot_popup_selection_moved() {
        let popup = PopupOverlay::new(sample_items()).selected(2);
        assert_snapshot!("popup_selection_last", render_plain(&popup, 80));
    }
}

mod layout_tests {
    use super::*;

    #[test]
    fn row_with_spacer_expands() {
        // A row with content + spacer + content should fill width
        let node = row([text("Left"), spacer(), text("Right")]);
        let plain = render_to_plain_text(&node, 40);

        // Should have spaces between Left and Right
        assert!(plain.contains("Left"));
        assert!(plain.contains("Right"));

        let width = visible_width(&plain);
        // The spacer should expand to fill available space
        assert!(
            width >= 10,
            "Row should expand with spacer: width={}",
            width
        );
    }

    #[test]
    fn column_stacks_vertically() {
        let node = col([text("Line 1"), text("Line 2"), text("Line 3")]);
        let plain = render_to_plain_text(&node, 80);

        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
    }

    #[test]
    fn styled_text_has_ansi() {
        let node = styled("Colored text", Style::new().fg(Color::Red).bold());
        let ansi = render_to_string(&node, 80);

        assert!(has_ansi_codes(&ansi), "Styled text should have ANSI codes");
        assert!(
            strip_ansi(&ansi).contains("Colored text"),
            "Should contain plain text after stripping"
        );
    }

    #[test]
    fn nested_components_render_correctly() {
        // Create a column containing status bar and input area
        let status = StatusBar::new().mode(ChatMode::Normal).model("test-model");
        let input = InputArea::new("Hello", 5, 80);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);

        let combined = col([status.view(&ctx), input.view(&ctx)]);
        let plain = render_to_plain_text(&combined, 80);

        // Both components should be present
        assert!(plain.contains("NORMAL"));
        assert!(plain.contains("test-model"));
        assert!(plain.contains("Hello"));
    }
}

mod tool_call_tests {
    use super::*;
    use crate::tui::oil::components::render_tool_call_with_frame;
    use crate::tui::oil::viewport_cache::CachedToolCall;
    use std::path::PathBuf;

    fn test_tool(name: &str, args: &str) -> CachedToolCall {
        CachedToolCall::new("tool-1", name, args)
    }

    fn test_tool_complete(name: &str, args: &str, output: &str) -> CachedToolCall {
        let mut tool = CachedToolCall::new("tool-1", name, args);
        tool.append_output(output);
        tool.mark_complete();
        tool
    }

    #[test]
    fn running_tool_shows_spinner() {
        let tool = test_tool("mcp_read", r#"{"path": "test.rs"}"#);
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("⠋"),
            "Running tool should show braille spinner: {:?}",
            plain
        );
        assert!(
            plain.contains("read"),
            "Should show tool name without mcp_ prefix"
        );
    }

    #[test]
    fn complete_tool_shows_checkmark() {
        let tool = test_tool_complete("mcp_glob", r#"{"pattern": "*.rs"}"#, "file1.rs\nfile2.rs");
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(plain.contains("✓"), "Complete tool should show checkmark");
        assert!(
            plain.contains("glob"),
            "Should show tool name without mcp_ prefix"
        );
    }

    #[test]
    fn error_tool_shows_x() {
        let mut tool = test_tool("mcp_bash", r#"{"command": "false"}"#);
        tool.set_error("Command failed with exit code 1".to_string());

        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(plain.contains("✗"), "Error tool should show X: {:?}", plain);
        assert!(
            plain.contains("Command failed"),
            "Should show error message"
        );
    }

    #[test]
    fn short_result_collapses_to_one_line() {
        let tool = test_tool_complete("custom_tool", "{}", "OK");
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("→ OK"),
            "Short result should collapse: {:?}",
            plain
        );
        let lines: Vec<_> = plain.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Should be single line for short result");
    }

    #[test]
    fn known_tool_shows_summary() {
        let tool = test_tool_complete("mcp_glob", r#"{"pattern": "*.rs"}"#, "a.rs\nb.rs\nc.rs");
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("→ 3 files"),
            "Should show file count summary: {:?}",
            plain
        );
    }

    #[test]
    fn edit_success_shows_applied() {
        let tool = test_tool_complete(
            "mcp_edit",
            r#"{"path": "test.rs"}"#,
            "Edit applied successfully",
        );
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("→ applied"),
            "Should show 'applied': {:?}",
            plain
        );
    }

    #[test]
    fn tool_with_output_path_shows_path() {
        let mut tool =
            test_tool_complete("mcp_bash", r#"{"command": "ls"}"#, "file1\nfile2\nfile3");
        tool.set_output_path(PathBuf::from("/tmp/output.txt"));

        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("→ /tmp/output.txt"),
            "Should show output path: {:?}",
            plain
        );
    }

    #[test]
    fn spinner_frame_changes_icon() {
        let tool = test_tool("mcp_read", "{}");

        let node0 = render_tool_call_with_frame(&tool, 0);
        let node1 = render_tool_call_with_frame(&tool, 1);

        let plain0 = render_to_plain_text(&node0, 80);
        let plain1 = render_to_plain_text(&node1, 80);

        assert!(plain0.contains("⠋"), "Frame 0 should show ⠋");
        assert!(plain1.contains("⠙"), "Frame 1 should show ⠙");
    }

    #[test]
    fn strips_mcp_prefix_from_name() {
        let tool = test_tool_complete("mcp_read", "{}", "content");
        let node = render_tool_call_with_frame(&tool, 0);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            !plain.contains("mcp_"),
            "Should strip mcp_ prefix: {:?}",
            plain
        );
        assert!(plain.contains("read"), "Should show base name");
    }
}
