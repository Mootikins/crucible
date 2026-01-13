use crate::tui::ink::*;

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
    assert_eq!(output, "Line 1\nLine 2");
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
