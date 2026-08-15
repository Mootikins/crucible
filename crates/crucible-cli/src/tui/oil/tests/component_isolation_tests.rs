//! Component isolation tests for correct rendering (spacing, ANSI, layout)
//!
//! These tests verify that individual components render correctly in isolation,
//! checking both structural output (plain text) and styled output (ANSI codes).

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{
    popup_item, popup_item_with_desc, InputComponent, PopupOverlay, StatusBar,
};
use crucible_oil::ansi::{strip_ansi, visible_width};
use crucible_oil::focus::FocusContext;
use crucible_oil::node::{col, row, spacer, styled, text, PopupItemNode};
use crucible_oil::render::{render_to_plain_text, render_to_string};
use crucible_oil::style::{Color, Style};
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

/// Extract RGB values from ANSI truecolor foreground escape code \x1b[38;2;R;G;Bm
fn extract_ansi_fg_color(s: &str) -> Option<(u8, u8, u8)> {
    // Look for pattern: \x1b[38;2;R;G;Bm
    for part in s.split("\x1b[") {
        if let Some(rest) = part.strip_prefix("38;2;") {
            let end = rest.find('m')?;
            let color_str = &rest[..end];
            let parts: Vec<&str> = color_str.split(';').collect();
            if parts.len() >= 3 {
                let r: u8 = parts[0].parse().ok()?;
                let g: u8 = parts[1].parse().ok()?;
                let b: u8 = parts[2].parse().ok()?;
                return Some((r, g, b));
            }
        }
    }
    None
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

#[cfg(test)]
mod extract_ansi_fg_color_tests {
    use super::*;

    #[test]
    fn extracts_rgb_from_foreground_escape() {
        let result = extract_ansi_fg_color("\x1b[38;2;60;64;72mtext");
        assert_eq!(result, Some((60, 64, 72)));
    }

    #[test]
    fn extracts_different_rgb_values() {
        let result = extract_ansi_fg_color("\x1b[38;2;255;128;0mtext");
        assert_eq!(result, Some((255, 128, 0)));
    }

    #[test]
    fn returns_none_for_plain_text() {
        let result = extract_ansi_fg_color("plain text");
        assert_eq!(result, None);
    }

    #[test]
    fn returns_none_for_background_color() {
        let result = extract_ansi_fg_color("\x1b[48;2;60;64;72mtext");
        assert_eq!(result, None);
    }
}

mod status_bar_tests {
    use super::*;

    fn render_bar(bar: &StatusBar, width: usize) -> String {
        render_to_plain_text(&bar.emergency_view(), width)
    }

    fn render_bar_ansi(bar: &StatusBar, width: usize) -> String {
        render_to_string(&bar.emergency_view(), width)
    }

    /// The footer bar as production renders it: everything at
    /// `footer.below_input`, stacked. Tests install a single bar, so this is one
    /// node — but it goes through the same path the app does.
    fn configured_bar_node(bar: &StatusBar) -> crucible_oil::node::Node {
        use crucible_lua::statusline_items::Region;
        // The input is elided: these tests render the status rows alone.
        crucible_oil::node::col(
            bar.render_region(Region::Prompt, false, || crucible_oil::node::Node::Empty),
        )
    }

    fn render_configured_bar(bar: &StatusBar, width: usize) -> String {
        let node = configured_bar_node(bar);
        render_to_plain_text(&node, width)
    }

    fn render_configured_bar_ansi(bar: &StatusBar, width: usize) -> String {
        let node = configured_bar_node(bar);
        render_to_string(&node, width)
    }

    #[test]
    fn renders_mode_label_at_start() {
        let bar = StatusBar::new().mode("normal");
        let plain = render_bar(&bar, 80);

        assert!(
            plain.starts_with(" NORMAL "),
            "Mode label should be at start with padding: {:?}",
            plain
        );
    }

    #[test]
    fn mode_labels_have_consistent_padding() {
        let normal = StatusBar::new().mode("normal");
        let plan = StatusBar::new().mode("plan");
        let auto = StatusBar::new().mode("auto");

        assert!(render_bar(&normal, 80).contains(" NORMAL "));
        assert!(render_bar(&plan, 80).contains(" PLAN "));
        assert!(render_bar(&auto, 80).contains(" AUTO "));
    }

    #[test]
    fn ansi_output_has_color_codes() {
        let bar = StatusBar::new().mode("normal").model("gpt-4o");
        let ansi = render_bar_ansi(&bar, 80);

        assert!(
            has_ansi_codes(&ansi),
            "StatusBar should have ANSI color codes"
        );
    }

    /// Each mode badge paints the palette slot its colour names.
    ///
    /// This used to read `normal.contains("42") || normal.contains("48;5;10")`
    /// — slot 2 or slot 10, green or *bright* green — and the frame emitted
    /// slot 10 for years because `Color::Green` mapped to crossterm's `Green`,
    /// which is the bright one. An either/or over the right answer and the
    /// wrong one cannot fail, so it never did. Exact slots only, `m`-anchored
    /// so `48;5;2` cannot be satisfied by `48;5;20`.
    #[test]
    fn mode_badge_colors_include_bg_fg_and_bold() {
        let normal = render_bar_ansi(&StatusBar::new().mode("normal"), 80);
        let plan = render_bar_ansi(&StatusBar::new().mode("plan"), 80);
        let auto = render_bar_ansi(&StatusBar::new().mode("auto"), 80);

        for (mode, ansi, bg_slot, colour) in [
            ("NORMAL", &normal, 2, "green"),
            ("PLAN", &plan, 4, "blue"),
            ("AUTO", &auto, 3, "yellow"),
        ] {
            assert!(
                ansi.contains(&format!("48;5;{bg_slot}m")),
                "{mode} badge should paint {colour} (palette {bg_slot}): {ansi:?}"
            );
            assert!(
                ansi.contains("38;5;0m"),
                "{mode} badge should use black text: {ansi:?}"
            );
            assert!(
                ansi.contains("\u{1b}[1m"),
                "{mode} badge should be bold: {ansi:?}"
            );
        }
    }

    #[test]
    fn different_modes_have_different_colors() {
        // Compare the STYLE, not the rendered frame: the frames already differ
        // in plain text (" NORMAL " vs " PLAN "), so a frame comparison passes
        // even if every mode resolves to the same colour. `mode_style` now has
        // a `_ =>` catch-all, so deleting the "plan" arm no longer fails to
        // compile either.
        use crate::tui::oil::chat_app::mode_style;

        assert_ne!(
            mode_style("normal").bg,
            mode_style("plan").bg,
            "normal and plan must be visually distinguishable"
        );
        assert_ne!(mode_style("plan").bg, mode_style("auto").bg);
    }

    #[test]
    fn model_name_appears_after_mode() {
        let bar = StatusBar::new().mode("normal").model("claude-3-opus");
        let plain = render_bar(&bar, 80);

        let mode_pos = plain.find("NORMAL").expect("mode should exist");
        let model_pos = plain.find("claude-3-opus").expect("model should exist");

        assert!(model_pos > mode_pos, "Model should appear after mode label");
    }

    #[test]
    fn context_percentage_formatted_correctly() {
        let bar = StatusBar::new().context(32000, 128000);
        let plain = render_bar(&bar, 80);

        assert!(
            plain.contains("25% ctx"),
            "Context should show percentage: {:?}",
            plain
        );
    }

    #[test]
    fn context_token_count_when_no_total() {
        let bar = StatusBar::new().context(15000, 0);
        let plain = render_bar(&bar, 80);

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
            .mode("normal")
            .toast("Processing", NotificationToastKind::Info);
        let node = configured_bar_node(&bar);
        let plain = render_to_plain_text(&node, 80);

        let mode_pos = plain.find("NORMAL").expect("mode should exist");
        let badge_pos = plain.find("INFO").expect("notification badge should exist");

        assert!(
            badge_pos > mode_pos,
            "Notification badge should appear after mode (on right side)"
        );
    }

    /// Shared StatusBar config for the ctrl+c notification snapshot tests:
    /// mode + model + active "Ctrl+C again to quit" warning toast. The narrow
    /// (width 40) and wide (width 120) snapshots both render this bar.
    /// Widths 50 and 80 are intentionally not snapshotted here — that is a
    /// DECLARED coverage-type change (see
    /// `.omo/evidence/task-14-test-suite-cleanup.md`).
    fn ctrlc_notification_bar() -> StatusBar {
        use crate::tui::oil::components::NotificationToastKind;
        StatusBar::new()
            .mode("normal")
            .model("glm-4.7-flash-iq4")
            .toast("Ctrl+C again to quit", NotificationToastKind::Warning)
    }

    #[test]
    fn snapshot_statusline_ctrlc_notification_right_aligned_120() {
        let bar = ctrlc_notification_bar();

        let plain = render_configured_bar(&bar, 120);
        let toast_start = plain
            .find("Ctrl+C again to quit")
            .expect("toast text should render");
        assert!(
            toast_start >= 88,
            "toast should be right-aligned with large spacer at width 120: {plain:?}"
        );

        assert_snapshot!(
            "statusline_ctrlc_notification_right_aligned_120",
            render_configured_bar_ansi(&bar, 120)
        );
    }

    /// US-205: the count-badge state (no toast, accumulated notification
    /// counts) also degrades gracefully — every badge AND its count stay
    /// intact at narrow widths; only the model span elides.
    #[test]
    fn snapshot_statusline_count_badges_narrow_width_40() {
        use crate::tui::oil::components::NotificationToastKind;

        let bar = StatusBar::new()
            .mode("normal")
            .model("glm-4.7-flash-iq4")
            .counts(vec![
                (NotificationToastKind::Warning, 2),
                (NotificationToastKind::Error, 1),
            ]);

        let ansi = render_configured_bar_ansi(&bar, 40);
        assert_fits_width(&ansi, 40);

        let plain = render_configured_bar(&bar, 40);
        assert!(
            plain.contains(" WARN ") && plain.contains(" ERROR "),
            "count badges must survive narrow widths intact: {plain:?}"
        );
        assert!(
            plain.contains(" 2 ") && plain.contains(" 1 "),
            "badge COUNTS are the payload — they must not shrink away: {plain:?}"
        );

        assert_snapshot!("statusline_count_badges_narrow_width_40", ansi);
    }

    /// US-205: at extreme narrow widths the badges (mode, WARN) stay intact
    /// and shrinkable spans (model, toast) absorb the overflow with ellipses —
    /// nothing overlaps and nothing lands past the right edge.
    #[test]
    fn snapshot_statusline_ctrlc_notification_narrow_width_40() {
        let bar = ctrlc_notification_bar();

        let ansi = render_configured_bar_ansi(&bar, 40);
        assert_fits_width(&ansi, 40);

        let plain = render_configured_bar(&bar, 40);
        assert!(
            plain.contains(" NORMAL ") && plain.contains(" WARN "),
            "badges must survive extreme narrow widths intact: {plain:?}"
        );

        assert_snapshot!("statusline_ctrlc_notification_narrow_width_40", ansi);
    }

    #[test]
    fn snapshot_statusline_idle_context_fallback_right_aligned() {
        let bar = StatusBar::new()
            .mode("normal")
            .model("glm-4.7-flash-iq4")
            .context(4096, 32768);

        let plain = render_configured_bar(&bar, 80);
        let ctx_start = plain
            .find("13% ctx")
            .expect("context fallback should render");
        assert!(
            ctx_start >= 70,
            "context fallback should stay right-aligned when no toast is active: {plain:?}"
        );

        assert_snapshot!(
            "statusline_idle_context_fallback_right_aligned",
            render_configured_bar_ansi(&bar, 80)
        );
    }

    #[test]
    fn fits_width_80() {
        use crate::tui::oil::components::NotificationToastKind;
        let bar = StatusBar::new()
            .mode("normal")
            .model("claude-3-opus-very-long-name")
            .context(64000, 128000)
            .status("Streaming...")
            .counts(vec![
                (NotificationToastKind::Warning, 3),
                (NotificationToastKind::Error, 1),
            ]);
        let node = configured_bar_node(&bar);
        let plain = render_to_plain_text(&node, 80);

        assert_fits_width(&plain, 80);
    }

    #[test]
    fn snapshot_normal_mode() {
        let bar = StatusBar::new()
            .mode("normal")
            .model("gpt-4o-mini")
            .context(10000, 128000);
        assert_snapshot!("status_bar_normal", render_bar(&bar, 80));
    }

    #[test]
    fn snapshot_plan_mode_with_status() {
        let bar = StatusBar::new()
            .mode("plan")
            .model("claude-3-opus")
            .context(50000, 200000)
            .status("Thinking...");
        assert_snapshot!("status_bar_plan", render_bar(&bar, 80));
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

        // Find the lines containing actual item text (not padding)
        let item_lines: Vec<&str> = ansi.lines().filter(|l| l.contains("Option")).collect();
        assert!(
            item_lines.len() >= 2,
            "Should have at least 2 item lines, got: {}",
            item_lines.len()
        );
        // The selected item (Option B, index 1) should differ from unselected (Option A, index 0)
        assert_ne!(
            item_lines[0], item_lines[1],
            "Selected line should differ from others"
        );
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

        let item_lines: Vec<_> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(item_lines.len(), 3, "Should have 3 item lines");
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

    #[test]
    fn popup_bg_matches_theme() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        let ansi = render_ansi(&popup, 80);

        // ThemeConfig popup_bg is Rgb(40, 44, 52)
        // ANSI escape for background: \x1b[48;2;40;44;52m
        assert!(
            ansi.contains("\x1b[48;2;40;44;52m"),
            "Popup should use theme popup_bg color Rgb(40,44,52). Got: {:?}",
            ansi
        );
    }

    #[test]
    fn popup_selected_bg_matches_theme() {
        let popup = PopupOverlay::new(sample_items()).selected(1);
        let ansi = render_ansi(&popup, 80);

        // Selected items should have a different background (theme-derived)
        // At minimum, should contain ANSI background color codes
        let lines: Vec<&str> = ansi.lines().collect();
        let mut found_selected_bg = false;

        for line in lines {
            // Selected line should have ANSI background code
            if line.contains("Option B") && line.contains("\x1b[48;2;") {
                found_selected_bg = true;
                break;
            }
        }

        assert!(
            found_selected_bg,
            "Selected item should have theme-derived background ANSI code"
        );
    }

    #[test]
    fn popup_all_lines_have_bg() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        let ansi = render_ansi(&popup, 80);

        // All non-empty rendered lines should have background color
        let lines: Vec<&str> = ansi.lines().collect();
        assert!(!lines.is_empty(), "Popup should render lines");

        for (i, line) in lines.iter().enumerate() {
            if !line.is_empty() {
                assert!(
                    line.contains("\x1b[48;2;"),
                    "Line {} should have background color ANSI code. Got: {:?}",
                    i,
                    line
                );
            }
        }
    }

    #[test]
    fn popup_bg_not_hardcoded_45_50_60() {
        let popup = PopupOverlay::new(sample_items()).selected(0);
        let ansi = render_ansi(&popup, 80);

        // The old hardcoded wrong value was Rgb(45, 50, 60)
        // ANSI escape: \x1b[48;2;45;50;60m
        // This test verifies we're NOT using that hardcoded value
        assert!(
            !ansi.contains("\x1b[48;2;45;50;60m"),
            "Popup should NOT use hardcoded Rgb(45,50,60). Should use theme popup_bg instead."
        );
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
        let status = StatusBar::new().mode("normal").model("test-model");
        let input = InputComponent::new("Hello", 5, 80);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);

        let combined = col([status.emergency_view(), input.view(&ctx)]);
        let plain = render_to_plain_text(&combined, 80);

        // Both components should be present
        assert!(plain.contains("NORMAL"));
        assert!(plain.contains("test-model"));
        assert!(plain.contains("Hello"));
    }
}

mod tool_call_tests {
    use super::*;
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
    fn running_tool_shows_pending_icon() {
        let tool = test_tool("mcp_read", r#"{"path": "test.rs"}"#);
        let node = tool.render_compact_with_frame(0, 80);
        let plain = render_to_plain_text(&node, 80);

        // Pending tools show static ● (no animated spinner — spinners are chrome only)
        assert!(
            plain.contains("\u{25CF}"),
            "Running tool should show pending ● icon: {:?}",
            plain
        );
        assert!(
            plain.contains("Read"),
            "Should show title-cased tool name without mcp_ prefix"
        );
    }

    #[test]
    fn complete_tool_shows_checkmark() {
        let tool = test_tool_complete("mcp_glob", r#"{"pattern": "*.rs"}"#, "file1.rs\nfile2.rs");
        let node = tool.render_compact_with_frame(0, 80);
        let plain = render_to_plain_text(&node, 80);

        assert!(plain.contains("✓"), "Complete tool should show checkmark");
        assert!(
            plain.contains("Glob"),
            "Should show title-cased tool name without mcp_ prefix"
        );
    }

    #[test]
    fn error_tool_shows_x() {
        let mut tool = test_tool("mcp_bash", r#"{"command": "false"}"#);
        tool.set_error("Command failed with exit code 1".to_string());

        let node = tool.render_compact_with_frame(0, 80);
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
        let node = tool.render_compact_with_frame(0, 80);
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
        let node = tool.render_compact_with_frame(0, 80);
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
        let node = tool.render_compact_with_frame(0, 80);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            plain.contains("→ applied"),
            "Should show 'applied': {:?}",
            plain
        );
    }

    #[test]
    fn tool_with_output_path_shows_summary_not_path() {
        let mut tool =
            test_tool_complete("mcp_bash", r#"{"command": "ls"}"#, "file1\nfile2\nfile3");
        tool.set_output_path(PathBuf::from("/tmp/output.txt"));

        let node = tool.render_compact_with_frame(0, 80);
        let plain = render_to_plain_text(&node, 80);

        // Spill paths should never appear in the TUI — only clean summaries
        assert!(
            !plain.contains("/tmp/output.txt"),
            "Should NOT show output path: {:?}",
            plain
        );
    }

    #[test]
    fn pending_icon_is_static_across_frames() {
        // Pending tools show a static ● regardless of spinner frame
        // (animated spinners are chrome only)
        let tool = test_tool("mcp_read", "{}");

        let node0 = tool.render_compact_with_frame(0, 80);
        let node1 = tool.render_compact_with_frame(5, 80);

        let plain0 = render_to_plain_text(&node0, 80);
        let plain1 = render_to_plain_text(&node1, 80);

        assert!(plain0.contains("\u{25CF}"), "Frame 0 should show ●");
        assert_eq!(
            plain0, plain1,
            "Pending icon should be static (no animation)"
        );
    }

    #[test]
    fn strips_mcp_prefix_from_name() {
        let tool = test_tool_complete("mcp_read", "{}", "content");
        let node = tool.render_compact_with_frame(0, 80);
        let plain = render_to_plain_text(&node, 80);

        assert!(
            !plain.contains("mcp_"),
            "Should strip mcp_ prefix: {:?}",
            plain
        );
        assert!(plain.contains("Read"), "Should show title-cased base name");
    }
}
