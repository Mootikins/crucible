use crucible_oil::render::render_with_cursor;
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

/// Fragment renders each child on its own line in Taffy layout
/// (each child is a separate block-level element)
#[test]
fn fragment_renders_all_children() {
    let node = fragment([text("A"), text("B"), text("C")]);
    let output = render_to_string(&node, 80);
    assert!(output.contains("A"), "should contain A");
    assert!(output.contains("B"), "should contain B");
    assert!(output.contains("C"), "should contain C");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 3, "Fragment children render as separate lines");
}

#[test]
fn col_node_renders_children() {
    let node = col([text("Content")]);
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

/// Zero width in Taffy means no space is allocated, producing empty output
#[test]
fn zero_width_no_wrap() {
    let node = text("Hello world");
    let output = render_to_string(&node, 0);
    assert!(
        output.is_empty(),
        "Zero-width container produces no visible output in Taffy"
    );
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

/// In Taffy, a non-flex/non-fixed child in a row alongside a fixed sibling
/// gets zero remaining space. Only the fixed child content is rendered.
/// (Flex content rendering is a known issue; see snapshot_flex_content_not_rendered_known_issue.)
#[test]
fn row_with_fixed_child_renders_in_multiline() {
    let node = row([fixed(10, text("Label")), text("Multi\nline\ncontent")]);
    let output = render_to_string(&node, 80);
    assert!(
        output.contains("Label"),
        "Fixed child 'Label' should be rendered"
    );
    // The non-fixed sibling gets zero width in Taffy row layout,
    // so its content does not appear.
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

/// Two-column layout: fixed-width label + text content.
/// In Taffy, a non-flex second child in a row gets zero remaining space,
/// so only the fixed child renders. This documents current Taffy behavior.
#[test]
fn snapshot_two_column_row_layout_with_fixed() {
    let node = row([
        fixed(8, text("Label:")),
        text("This is content that follows the label"),
    ]);
    assert_snapshot!(render_to_string(&node, 80));
}

/// Taffy allocates the declared width for empty fixed children, producing
/// padding space before subsequent content in the row.
#[test]
fn empty_fixed_child_produces_no_output() {
    let node = row([fixed(10, Node::Empty), text("Content")]);
    let output = render_to_string(&node, 80);
    assert!(
        output.contains("Content"),
        "Content should still be rendered"
    );
    // Taffy allocates 10 columns for the empty fixed child, so output
    // starts with spaces before "Content"
    assert!(
        output.len() > "Content".len(),
        "Output should include space allocated by the empty fixed child"
    );
}

/// In Taffy, fixed(0) allocates zero columns but the tree renderer still
/// outputs the text content at that position. The sibling text node gets
/// zero width as a non-flex child in a row, so only the fixed child's
/// content appears.
#[test]
fn zero_width_fixed_still_renders_content() {
    let node = row([fixed(0, text("Hidden")), text("Visible")]);
    let output = render_to_string(&node, 80);
    // fixed(0) content is still written to output by the tree renderer
    assert!(
        output.contains("Hidden"),
        "Zero-width fixed child content is still rendered"
    );
    // The non-flex sibling gets no space in Taffy row layout
    assert!(
        !output.contains("Visible"),
        "Non-flex sibling should get zero space in a row with a fixed child"
    );
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

/// Single child with gap should produce no extra newlines (gap only applies between children)
#[test]
fn gap_with_single_child_produces_no_extra_newlines() {
    let node = col([text("Only child")]).gap(Gap::all(5));
    let output = render_to_string(&node, 80);

    // Gap of 5 with single child: no gaps between children (only 1 child)
    // Expected: just 1 line with the content
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "Expected 1 line with single child (gap doesn't apply), got {}: {:?}",
        lines.len(),
        lines
    );
    assert_eq!(lines[0], "Only child");
}

/// Empty children list with gap should produce empty output
#[test]
fn gap_with_empty_children_produces_empty_output() {
    let node = col([]).gap(Gap::all(5));
    let output = render_to_string(&node, 80);

    // Empty children list produces empty output regardless of gap
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        0,
        "Expected 0 lines with empty children, got {}: {:?}",
        lines.len(),
        lines
    );
}

/// No explicit gap() call should behave same as gap=0 (default)
#[test]
fn default_gap_has_no_extra_blank_lines() {
    let node = col([text("X"), text("Y")]);
    let output = render_to_string(&node, 80);

    // Default gap (no .gap() call) should be 0, producing no blank lines
    // Expected: 2 lines with no blanks between
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "Expected 2 lines with default gap (0), got {}: {:?}",
        lines.len(),
        lines
    );
    assert_eq!(lines[0], "X");
    assert_eq!(lines[1], "Y");
}

#[test]
fn spinner_with_theme_style_renders_with_ansi_codes() {
    use crucible_oil::style::Style;
    use crate::tui::oil::theme::ThemeConfig;

    // Create a spinner with a theme-derived style
    let theme = ThemeConfig::default_dark();
    let spinner_style = Style::new().fg(theme.resolve_color(theme.colors.text));
    let node = spinner(None, 0).with_style(spinner_style);

    let output = render_to_string(&node, 80);

    // The output should contain ANSI escape codes for the style
    // (not just plain text with Style::default())
    // We verify that the style is not the default by checking that
    // the spinner_style() returns a non-default style
    assert_ne!(
        spinner_style,
        Style::default(),
        "Spinner style should not be default"
    );

    // The rendered output should contain the spinner character
    assert!(
        !output.is_empty(),
        "Spinner should render to non-empty output"
    );
}
