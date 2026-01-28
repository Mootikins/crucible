use crate::tui::oil::render::{render_with_cursor, CursorInfo};
use crate::tui::oil::*;
use insta::assert_snapshot;

#[test]
fn text_renders_content() {
    let node = text("Hello world");
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Hello world");
}

#[test]
fn empty_node_renders_nothing() {
    let node = Node::Empty;
    let output = render_to_string(&node, 80);
    assert_eq!(output, "");
}

#[test]
fn column_renders_children_with_newlines() {
    let node = col([text("Line 1"), text("Line 2")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Line 1\r\nLine 2");
}

#[test]
fn row_renders_children_inline() {
    let node = row([text("A"), text("B"), text("C")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "ABC");
}

#[test]
fn nested_layout_renders_correctly() {
    let node = col([
        row([text(" > "), text("User message")]),
        row([text(" · "), text("Assistant reply")]),
    ]);
    let output = render_to_string(&node, 80);
    assert!(output.contains(" > User message"));
    assert!(output.contains(" · Assistant reply"));
    assert!(output.contains('\n'));
}

#[test]
fn text_wraps_at_width() {
    let node = text("Hello world this is a long line that should wrap");
    let output = render_to_string(&node, 20);

    let lines: Vec<&str> = output.lines().collect();
    assert!(lines.len() > 1, "Should wrap to multiple lines");

    for line in &lines {
        assert!(
            line.chars().count() <= 20,
            "Line '{}' exceeds width 20",
            line
        );
    }
}

#[test]
fn text_no_wrap_when_fits() {
    let node = text("Short");
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Short");
    assert!(!output.contains('\n'));
}

#[test]
fn fragment_renders_all_children() {
    let node = fragment([text("A"), text("B"), text("C")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "ABC");
}

#[test]
fn static_node_renders_children() {
    let node = scrollback("key", [text("Content")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Content");
}

#[test]
fn input_renders_value() {
    let node = text_input("hello", 5);
    let output = render_to_string(&node, 80);
    assert!(output.contains("hello"));
}

#[test]
fn input_empty_shows_placeholder() {
    let input = InputNode {
        value: String::new(),
        cursor: 0,
        placeholder: Some("Type here...".into()),
        style: Style::default(),
        focused: true,
    };
    let node = Node::Input(input);
    let output = render_to_string(&node, 80);
    assert!(output.contains("Type here"));
}

#[test]
fn spinner_renders_frame() {
    let node = spinner(None, 0);
    let output = render_to_string(&node, 80);
    assert!(output.contains('◐'));
}

#[test]
fn spinner_with_label() {
    let node = spinner(Some("Loading...".into()), 1);
    let output = render_to_string(&node, 80);
    assert!(output.contains('◓'));
    assert!(output.contains("Loading..."));
}

#[test]
fn empty_children_skipped() {
    let node = col([Node::Empty, text("Visible"), Node::Empty]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Visible");
}

#[test]
fn deeply_nested_structure() {
    let node = col([
        text("Header"),
        col([row([text("A"), text("B")]), row([text("C"), text("D")])]),
        text("Footer"),
    ]);

    let output = render_to_string(&node, 80);
    assert!(output.contains("Header"));
    assert!(output.contains("AB"));
    assert!(output.contains("CD"));
    assert!(output.contains("Footer"));
}

#[test]
fn chat_message_structure() {
    let user_msg = row([styled(" > ", Style::new().dim()), text("What is 2+2?")]);

    let assistant_msg = row([styled(" · ", Style::new().dim()), text("The answer is 4.")]);

    let node = col([user_msg, assistant_msg]);
    let output = render_to_string(&node, 80);

    assert!(output.contains(" > "));
    assert!(output.contains("What is 2+2?"));
    assert!(output.contains(" · "));
    assert!(output.contains("The answer is 4."));
}

#[test]
fn zero_width_no_wrap() {
    let node = text("Hello world");
    let output = render_to_string(&node, 0);
    assert_eq!(output, "Hello world");
}

#[test]
fn multiline_text_preserved() {
    let node = text("Line 1\nLine 2\nLine 3");
    let output = render_to_string(&node, 80);

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[test]
fn cursor_tracked_for_focused_input() {
    let input = InputNode {
        value: "hello".into(),
        cursor: 3,
        placeholder: None,
        style: Style::default(),
        focused: true,
    };
    let node = Node::Input(input);
    let result = render_with_cursor(&node, 80);

    assert!(
        result.cursor.visible,
        "Cursor should be visible for focused input"
    );
    assert_eq!(
        result.cursor.col, 3,
        "Cursor column should match input cursor position"
    );
}

#[test]
fn cursor_not_tracked_for_unfocused_input() {
    let input = InputNode {
        value: "hello".into(),
        cursor: 3,
        placeholder: None,
        style: Style::default(),
        focused: false,
    };
    let node = Node::Input(input);
    let result = render_with_cursor(&node, 80);

    assert!(
        !result.cursor.visible,
        "Cursor should not be visible for unfocused input"
    );
}

#[test]
fn cursor_position_accounts_for_prefix_in_row() {
    let node = row([
        text(" > "),
        Node::Input(InputNode {
            value: "hello".into(),
            cursor: 2,
            placeholder: None,
            style: Style::default(),
            focused: true,
        }),
    ]);
    let result = render_with_cursor(&node, 80);

    assert!(result.cursor.visible);
    assert_eq!(
        result.cursor.col, 5,
        "Cursor should be at prefix(3) + cursor(2) = 5"
    );
}

#[test]
fn cursor_row_from_end_for_input_on_last_line() {
    let node = col([
        text("Line 1"),
        text("Line 2"),
        Node::Input(InputNode {
            value: "input".into(),
            cursor: 0,
            placeholder: None,
            style: Style::default(),
            focused: true,
        }),
    ]);
    let result = render_with_cursor(&node, 80);

    assert!(result.cursor.visible);
    assert_eq!(
        result.cursor.row_from_end, 0,
        "Input on last line should have row_from_end=0"
    );
}

#[test]
fn cursor_row_from_end_for_input_with_lines_below() {
    let node = col([
        text("Line 1"),
        Node::Input(InputNode {
            value: "input".into(),
            cursor: 0,
            placeholder: None,
            style: Style::default(),
            focused: true,
        }),
        text("Line 3"),
    ]);
    let result = render_with_cursor(&node, 80);

    assert!(result.cursor.visible);
    assert_eq!(
        result.cursor.row_from_end, 1,
        "Input with 1 line below should have row_from_end=1"
    );
}

#[test]
fn cursor_in_chat_input_structure() {
    let node = col([
        text("History line"),
        row([
            text(" > "),
            Node::Input(InputNode {
                value: "user input".into(),
                cursor: 5,
                placeholder: None,
                style: Style::default(),
                focused: true,
            }),
        ]),
        text("Status bar"),
    ]);
    let result = render_with_cursor(&node, 80);

    assert!(result.cursor.visible);
    assert_eq!(result.cursor.col, 8, "Cursor at prefix(3) + cursor(5) = 8");
    assert_eq!(
        result.cursor.row_from_end, 1,
        "Input row with status bar below"
    );
}

#[test]
fn cursor_in_full_input_box_structure() {
    // Mimics actual chat app input box structure:
    // - top_edge (▄▄▄)
    // - row with prompt + InputNode
    // - bottom_edge (▀▀▀)
    // - status bar
    let input_box = col([
        text("top_edge"),
        row([
            text(" > "),
            Node::Input(InputNode {
                value: "hello".into(),
                cursor: 2,
                placeholder: None,
                style: Style::default(),
                focused: true,
            }),
        ]),
        text("bottom_edge"),
    ]);

    let node = col([text("History"), input_box, text("Status bar")]);

    let result = render_with_cursor(&node, 80);

    // Structure:
    // Line 0: History
    // Line 1: top_edge
    // Line 2: " > hello" (input line - cursor here)
    // Line 3: bottom_edge
    // Line 4: Status bar
    // Total: 5 lines, cursor on line 2, so row_from_end = 5 - 3 = 2

    assert!(result.cursor.visible);
    assert_eq!(result.cursor.col, 5, "Cursor at prefix(3) + cursor(2) = 5");
    assert_eq!(
        result.cursor.row_from_end, 2,
        "Input line with bottom_edge and status below"
    );
}

#[test]
fn cursor_row_from_end_uses_visual_rows() {
    // Test that row_from_end accounts for line wrapping
    // With width=20, a 30-char line wraps to 2 visual rows
    let node = col([
        Node::Input(InputNode {
            value: "input".into(),
            cursor: 0,
            placeholder: None,
            style: Style::default(),
            focused: true,
        }),
        text("this line is exactly thirty!!"), // 30 chars, wraps at width 20
    ]);

    let result = render_with_cursor(&node, 20);

    assert!(result.cursor.visible);
    // The line below wraps to 2 visual rows, so row_from_end should be 2
    assert_eq!(
        result.cursor.row_from_end, 2,
        "30-char line at width 20 = 2 visual rows below cursor"
    );
}

#[test]
fn row_with_fixed_child_renders_in_multiline() {
    let node = row([fixed(10, text("Label")), text("Multi\nline\ncontent")]);
    let output = render_to_string(&node, 80);
    assert!(
        output.contains("Label"),
        "Fixed child 'Label' should be rendered"
    );
    assert!(
        output.contains("Multi"),
        "Multi-line content should be rendered"
    );
}

#[test]
fn row_with_multiline_fixed_child_affects_height() {
    let node = row([fixed(10, text("Line1\nLine2")), text("A")]);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "Row should have 2 lines (Fixed child has 2 lines)"
    );
}

/// Documents known issue: Flex children don't render their content.
/// This test captures the BROKEN behavior where only "Label:" appears.
/// Remove/update this test when Flex content rendering is fixed.
/// See: .sisyphus/notepads/oil-row-multiline-fix/issues.md
#[test]
fn snapshot_flex_content_not_rendered_known_issue() {
    let node = row([
        col([text("Label:")]),
        flex(
            1,
            col([text(
                "This is a longer description that should fill the remaining space in the row",
            )]),
        ),
    ]);
    // Expected: "Label:This is a longer description..."
    // Actual: "Label:" (Flex content missing - this is a bug)
    assert_snapshot!(render_to_string(&node, 80));
}

#[test]
fn snapshot_two_column_row_layout_with_fixed() {
    let node = row([
        fixed(8, text("Label:")),
        text("This is content that follows the label"),
    ]);
    assert_snapshot!(render_to_string(&node, 80));
}

/// Documents behavior: Empty Fixed children produce no output in single-line rows.
/// The declared width is used for layout calculation but empty content renders nothing.
#[test]
fn empty_fixed_child_produces_no_output() {
    let node = row([fixed(10, Node::Empty), text("Content")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "Content");
}

/// Documents behavior: Zero-width Fixed still renders its content.
/// The width=0 declaration doesn't truncate or hide the content.
#[test]
fn zero_width_fixed_still_renders_content() {
    let node = row([fixed(0, text("Hidden")), text("Visible")]);
    let output = render_to_string(&node, 80);
    assert_eq!(output, "HiddenVisible");
}

/// Gap of 1 should add exactly one blank line between column children
#[test]
fn gap_1_adds_blank_line_between_children() {
    let node = col([text("Line 1"), text("Line 2"), text("Line 3")]).gap(Gap::all(1));
    let output = render_to_string(&node, 80);

    // Gap of 1 means 1 extra blank line between each child
    // So: "Line 1" + "\r\n\r\n" (base newline + 1 gap) + "Line 2" + "\r\n\r\n" + "Line 3"
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "Expected 5 lines (3 content + 2 blank), got {}: {:?}",
        lines.len(),
        lines
    );
    assert_eq!(lines[0], "Line 1");
    assert!(lines[1].is_empty(), "Expected blank line after Line 1");
    assert_eq!(lines[2], "Line 2");
    assert!(lines[3].is_empty(), "Expected blank line after Line 2");
    assert_eq!(lines[4], "Line 3");
}

/// Gap of 2 should add two blank lines between column children
#[test]
fn gap_2_adds_two_blank_lines_between_children() {
    let node = col([text("First"), text("Second")]).gap(Gap::all(2));
    let output = render_to_string(&node, 80);

    // Gap of 2 means 2 extra blank lines between each child
    // First\r\n\r\n\r\nSecond = ["First", "", "", "Second"] = 4 lines
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "Expected 4 lines (2 content + 2 blank), got {}: {:?}",
        lines.len(),
        lines
    );
    assert_eq!(lines[0], "First");
    assert!(lines[1].is_empty());
    assert!(lines[2].is_empty());
    assert_eq!(lines[3], "Second");
}

/// Gap of 0 should not add any blank lines (regression test)
#[test]
fn gap_0_has_no_extra_blank_lines() {
    let node = col([text("A"), text("B"), text("C")]).gap(Gap::all(0));
    let output = render_to_string(&node, 80);

    // Gap of 0 means just 1 newline between each (the base newline)
    // No blank lines should appear
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "Expected 3 lines with no blanks, got {}: {:?}",
        lines.len(),
        lines
    );
    assert_eq!(lines[0], "A");
    assert_eq!(lines[1], "B");
    assert_eq!(lines[2], "C");
}
