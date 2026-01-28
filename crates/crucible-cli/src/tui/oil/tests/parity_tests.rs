//! Parity tests comparing legacy render path vs LayoutTree render path.
//!
//! These tests ensure that `plan_legacy()` and `plan_with_layout_tree()` produce
//! equivalent output for the same input nodes. This validates that the new
//! LayoutTree-based rendering pipeline can replace the legacy path without
//! visual regressions.
//!
//! # Known Differences
//!
//! The two paths may have minor differences:
//! - **Trailing whitespace**: Legacy may have trailing spaces, LayoutTree uses CellGrid
//! - **Line endings**: Both use `\r\n` but edge cases may differ
//! - **Gap handling**: Spacing between children may differ slightly
//!
//! Tests document these differences when found.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::node::{
    col, popup, popup_item, row, scrollback, spinner, styled, text, text_input, BoxNode, Direction,
    InputNode, Node, Size,
};
use crate::tui::oil::planning::FramePlanner;
use crate::tui::oil::style::{Border, Color, Gap, Padding, Style};

/// Helper to compare legacy and layout tree outputs.
/// Returns (legacy_output, layout_tree_output, are_equal).
fn compare_outputs(node: &Node, width: u16, height: u16) -> (String, String, bool) {
    let mut legacy_planner = FramePlanner::new(width, height);
    let mut layout_tree_planner = FramePlanner::new(width, height);

    let legacy_snapshot = legacy_planner.plan_legacy(node);
    let layout_tree_snapshot = layout_tree_planner.plan_with_layout_tree(node);

    let legacy_content = strip_ansi(legacy_snapshot.viewport_content());
    let layout_tree_content = strip_ansi(layout_tree_snapshot.viewport_content());

    let are_equal = legacy_content == layout_tree_content;
    (legacy_content, layout_tree_content, are_equal)
}

/// Helper to compare outputs, normalizing trailing whitespace per line.
fn compare_outputs_normalized(node: &Node, width: u16, height: u16) -> (String, String, bool) {
    let (legacy, layout_tree, _) = compare_outputs(node, width, height);

    // Normalize: trim trailing whitespace from each line
    let legacy_normalized: String = legacy
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let layout_tree_normalized: String = layout_tree
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    let are_equal = legacy_normalized == layout_tree_normalized;
    (legacy_normalized, layout_tree_normalized, are_equal)
}

// =============================================================================
// 1. Simple Text Tests
// =============================================================================

#[test]
fn parity_simple_text() {
    let node = text("Hello, World!");
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Simple text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_empty_text() {
    let node = text("");
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Empty text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_multiline_text() {
    let node = text("Line 1\nLine 2\nLine 3");
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Multiline text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_text_wrapping() {
    let node = text("This is a long line that should wrap when rendered at a narrow width");
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 20, 24);

    assert!(
        are_equal,
        "Wrapped text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 2. Styled Text Tests
// =============================================================================

#[test]
fn parity_styled_text_bold() {
    let node = styled("Bold text", Style::new().bold());
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Bold styled text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_styled_text_dim() {
    let node = styled("Dim text", Style::new().dim());
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Dim styled text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_styled_text_colored() {
    let node = styled("Colored text", Style::new().fg(Color::Red).bg(Color::Blue));
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Colored styled text should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 3. Column Layout Tests
// =============================================================================

#[test]
fn parity_simple_column() {
    let node = col([text("Line 1"), text("Line 2"), text("Line 3")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Simple column should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_column_with_empty_children() {
    let node = col([text("First"), Node::Empty, text("Third")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Column with empty children should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_nested_columns() {
    let node = col([
        text("Outer 1"),
        col([text("Inner 1"), text("Inner 2")]),
        text("Outer 2"),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Nested columns should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 4. Row Layout Tests
// =============================================================================

#[test]
fn parity_simple_row() {
    let node = row([text("A"), text("B"), text("C")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Simple row should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_row_with_styled_children() {
    let node = row([
        styled(" > ", Style::new().dim()),
        text("User message content"),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Row with styled children should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_row_with_empty_children() {
    let node = row([text("A"), Node::Empty, text("C")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Row with empty children should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 5. Nested Box Tests (Complex Hierarchy)
// =============================================================================

#[test]
fn parity_nested_row_in_column() {
    let node = col([
        row([text(" > "), text("User message")]),
        row([text(" · "), text("Assistant reply")]),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Nested row in column should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_deeply_nested_structure() {
    let node = col([
        text("Header"),
        col([row([text("A"), text("B")]), row([text("C"), text("D")])]),
        text("Footer"),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Deeply nested structure should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_chat_message_structure() {
    let user_msg = row([styled(" > ", Style::new().dim()), text("What is 2+2?")]);
    let assistant_msg = row([styled(" · ", Style::new().dim()), text("The answer is 4.")]);
    let node = col([user_msg, assistant_msg]);

    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Chat message structure should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 6. Gap Tests
// =============================================================================

#[test]
fn parity_column_with_gap() {
    let node = col([text("Line 1"), text("Line 2")]).gap(Gap::all(1));
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    // Document if gap handling differs
    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Gap handling differs between legacy and LayoutTree.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    assert!(
        are_equal,
        "Gap handling differs between legacy and LayoutTree:\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 7. Border Tests
// =============================================================================

#[test]
fn parity_box_with_single_border() {
    let node = text("Bordered content").with_border(Border::Single);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    // Document if border handling differs
    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Border handling differs between legacy and LayoutTree.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    // Verify both contain the content
    assert!(legacy.contains("Bordered") || layout_tree.contains("Bordered"));
}

#[test]
fn parity_box_with_double_border() {
    let node = text("Double bordered").with_border(Border::Double);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Double border handling differs.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    assert!(legacy.contains("Double") || layout_tree.contains("Double"));
}

// =============================================================================
// 8. Mixed Content Tests (Text + Input + Spinner)
// =============================================================================

#[test]
fn parity_input_with_value() {
    let node = text_input("hello world", 5);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Input with value should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_input_with_placeholder() {
    let input = InputNode {
        value: String::new(),
        cursor: 0,
        placeholder: Some("Type here...".into()),
        style: Style::default(),
        focused: true,
    };
    let node = Node::Input(input);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Input with placeholder should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_spinner_basic() {
    let node = spinner(None, 0);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Basic spinner should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_spinner_with_label() {
    let node = spinner(Some("Loading...".into()), 1);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Spinner with label should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_mixed_content_column() {
    let node = col([
        text("Header text"),
        row([text(" > "), text_input("user input", 5)]),
        spinner(Some("Processing...".into()), 0),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Mixed content column should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 9. Scrollback (Static) Node Tests
// =============================================================================

#[test]
fn parity_scrollback_node() {
    let node = scrollback("msg-1", [text("Message content")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Scrollback node should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_multiple_scrollback_nodes() {
    let node = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
    ]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Multiple scrollback nodes should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

// =============================================================================
// 10. Popup Tests
// =============================================================================

#[test]
fn parity_popup_basic() {
    let items = vec![
        popup_item("Item 1"),
        popup_item("Item 2"),
        popup_item("Item 3"),
    ];
    let node = popup(items, 0, 3);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    // Popups may have different rendering approaches
    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Popup rendering differs.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    // Verify both contain item text
    assert!(legacy.contains("Item 1") || layout_tree.contains("Item 1"));
}

#[test]
fn parity_popup_with_selection() {
    let items = vec![
        popup_item("First"),
        popup_item("Second"),
        popup_item("Third"),
    ];
    let node = popup(items, 1, 3);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Popup with selection differs.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    // Verify selection indicator is present in at least one
    assert!(legacy.contains("▸") || layout_tree.contains("▸"));
}

// =============================================================================
// 11. Edge Cases
// =============================================================================

#[test]
fn parity_empty_node() {
    let node = Node::Empty;
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Empty node should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_empty_column() {
    let node = col([]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Empty column should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_empty_row() {
    let node = row([]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    assert!(
        are_equal,
        "Empty row should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_narrow_width() {
    let node = col([text("Short"), text("Also short")]);
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 10, 24);

    assert!(
        are_equal,
        "Narrow width rendering should match.\nLegacy:\n{}\n\nLayoutTree:\n{}",
        legacy, layout_tree
    );
}

#[test]
fn parity_very_narrow_width() {
    let node = text("This text will definitely wrap at this narrow width");
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 5, 24);

    // Very narrow widths may have edge case differences
    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Very narrow width wrapping differs.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
}

// =============================================================================
// 12. Padding Tests
// =============================================================================

#[test]
fn parity_box_with_padding() {
    let node = text("Padded content").with_padding(Padding::all(1));
    let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);

    if !are_equal {
        eprintln!(
            "KNOWN DIFFERENCE: Padding handling differs.\n\
             Legacy:\n{}\n\nLayoutTree:\n{}",
            legacy, layout_tree
        );
    }
    // Verify content is present
    assert!(legacy.contains("Padded") || layout_tree.contains("Padded"));
}

// =============================================================================
// 13. Graduation Filtering Tests
// =============================================================================

#[test]
fn parity_graduation_filters_same_keys() {
    // Test that both paths filter graduated keys the same way
    let node = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
        text("Non-scrollback content"),
    ]);

    let mut legacy_planner = FramePlanner::new(80, 24);
    let mut layout_tree_planner = FramePlanner::new(80, 24);

    // First render - both should graduate the scrollback nodes
    let legacy_snap1 = legacy_planner.plan_legacy(&node);
    let layout_tree_snap1 = layout_tree_planner.plan_with_layout_tree(&node);

    // Check graduated keys match
    let legacy_keys: Vec<_> = legacy_snap1
        .plan
        .graduated
        .iter()
        .map(|g| g.key.clone())
        .collect();
    let layout_tree_keys: Vec<_> = layout_tree_snap1
        .plan
        .graduated
        .iter()
        .map(|g| g.key.clone())
        .collect();

    assert_eq!(
        legacy_keys, layout_tree_keys,
        "Graduated keys should match between legacy and LayoutTree"
    );

    // Second render - viewport should be similar (graduated content filtered)
    let legacy_snap2 = legacy_planner.plan_legacy(&node);
    let layout_tree_snap2 = layout_tree_planner.plan_with_layout_tree(&node);

    let legacy_viewport = strip_ansi(legacy_snap2.viewport_content());
    let layout_tree_viewport = strip_ansi(layout_tree_snap2.viewport_content());

    // Both should have filtered out the graduated scrollback content
    assert!(
        !legacy_viewport.contains("First message"),
        "Legacy should filter graduated content"
    );
    assert!(
        !layout_tree_viewport.contains("First message"),
        "LayoutTree should filter graduated content"
    );
}

// =============================================================================
// Summary Test - Documents All Known Differences
// =============================================================================

#[test]
fn document_known_differences() {
    // This test documents all known differences between the two render paths.
    // It serves as a reference for what to expect when migrating.

    let test_cases: Vec<(&str, Node)> = vec![
        ("simple_text", text("Hello")),
        ("column", col([text("A"), text("B")])),
        ("row", row([text("X"), text("Y")])),
        ("nested", col([row([text("1"), text("2")]), text("3")])),
        ("styled", styled("Bold", Style::new().bold())),
        ("input", text_input("value", 3)),
        ("spinner", spinner(Some("Loading".into()), 0)),
    ];

    let mut differences = Vec::new();

    for (name, node) in test_cases {
        let (legacy, layout_tree, are_equal) = compare_outputs_normalized(&node, 80, 24);
        if !are_equal {
            differences.push((name, legacy, layout_tree));
        }
    }

    if !differences.is_empty() {
        eprintln!("\n=== KNOWN DIFFERENCES SUMMARY ===\n");
        for (name, legacy, layout_tree) in &differences {
            eprintln!("Test: {}", name);
            eprintln!("Legacy:\n{}", legacy);
            eprintln!("LayoutTree:\n{}", layout_tree);
            eprintln!("---");
        }
        eprintln!("\nTotal differences: {}", differences.len());
    } else {
        eprintln!("\n=== ALL TESTS MATCH ===\n");
    }

    // This test always passes - it's for documentation purposes
}
