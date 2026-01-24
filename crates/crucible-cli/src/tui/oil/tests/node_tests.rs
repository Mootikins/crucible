use crate::tui::oil::*;

#[test]
fn text_creates_text_node() {
    let node = text("Hello");
    match node {
        Node::Text(t) => assert_eq!(t.content, "Hello"),
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn styled_applies_style() {
    let node = styled("Error", Style::new().fg(Color::Red).bold());
    match node {
        Node::Text(t) => {
            assert_eq!(t.content, "Error");
            assert_eq!(t.style.fg, Some(Color::Red));
            assert!(t.style.bold);
        }
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn col_creates_column_box() {
    let node = col([text("A"), text("B")]);
    match node {
        Node::Box(b) => {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.children.len(), 2);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn row_creates_row_box() {
    let node = row([text("A"), text("B")]);
    match node {
        Node::Box(b) => {
            assert_eq!(b.direction, Direction::Row);
            assert_eq!(b.children.len(), 2);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn scrollback_creates_static_node_with_newline() {
    let node = scrollback("msg-1", [text("Hello")]);
    match node {
        Node::Static(s) => {
            assert_eq!(s.key, "msg-1");
            assert!(s.newline);
            assert_eq!(s.children.len(), 1);
        }
        _ => panic!("Expected Static node"),
    }
}

#[test]
fn scrollback_continuation_has_no_newline() {
    let node = scrollback_continuation("msg-1-cont", [text("world")]);
    match node {
        Node::Static(s) => {
            assert!(!s.newline);
        }
        _ => panic!("Expected Static node"),
    }
}

#[test]
fn text_input_creates_input_node() {
    let node = text_input("hello", 5);
    match node {
        Node::Input(i) => {
            assert_eq!(i.value, "hello");
            assert_eq!(i.cursor, 5);
            assert!(i.focused);
        }
        _ => panic!("Expected Input node"),
    }
}

#[test]
fn spinner_creates_spinner_node() {
    let node = spinner(Some("Loading...".into()), 2);
    match node {
        Node::Spinner(s) => {
            assert_eq!(s.label, Some("Loading...".into()));
            assert_eq!(s.frame, 2);
        }
        _ => panic!("Expected Spinner node"),
    }
}

#[test]
fn spinner_cycles_frames() {
    let s = SpinnerNode {
        label: None,
        style: Style::default(),
        frame: 0,
        frames: None,
    };
    assert_eq!(s.current_char(), '◐');

    let s = SpinnerNode { frame: 1, ..s };
    assert_eq!(s.current_char(), '◓');

    let s = SpinnerNode { frame: 4, ..s };
    assert_eq!(s.current_char(), '◐');
}

#[test]
fn fragment_collects_children() {
    let node = fragment([text("A"), text("B"), text("C")]);
    match node {
        Node::Fragment(children) => {
            assert_eq!(children.len(), 3);
        }
        _ => panic!("Expected Fragment node"),
    }
}

#[test]
fn spacer_creates_flex_box() {
    let node = spacer();
    match node {
        Node::Box(b) => {
            assert_eq!(b.size, Size::Flex(1));
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn with_style_modifies_text_node() {
    let node = text("Hello").with_style(Style::new().fg(Color::Green));
    match node {
        Node::Text(t) => {
            assert_eq!(t.style.fg, Some(Color::Green));
        }
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn with_padding_wraps_non_box() {
    let node = text("Hello").with_padding(Padding::all(1));
    match node {
        Node::Box(b) => {
            assert_eq!(b.padding, Padding::all(1));
            assert_eq!(b.children.len(), 1);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn with_border_wraps_non_box() {
    let node = text("Hello").with_border(Border::Rounded);
    match node {
        Node::Box(b) => {
            assert_eq!(b.border, Some(Border::Rounded));
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn nested_layout_structure() {
    let node = col([
        row([text(" > "), text("User message")]),
        row([text(" · "), text("Assistant reply")]),
    ]);

    match node {
        Node::Box(outer) => {
            assert_eq!(outer.direction, Direction::Column);
            assert_eq!(outer.children.len(), 2);

            match &outer.children[0] {
                Node::Box(inner) => {
                    assert_eq!(inner.direction, Direction::Row);
                    assert_eq!(inner.children.len(), 2);
                }
                _ => panic!("Expected inner Box"),
            }
        }
        _ => panic!("Expected outer Box"),
    }
}

#[test]
fn when_returns_node_on_true() {
    let node = when(true, text("Visible"));
    match node {
        Node::Text(t) => assert_eq!(t.content, "Visible"),
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn when_returns_empty_on_false() {
    let node = when(false, text("Hidden"));
    assert!(matches!(node, Node::Empty));
}

#[test]
fn if_else_returns_then_on_true() {
    let node = if_else(true, text("Yes"), text("No"));
    match node {
        Node::Text(t) => assert_eq!(t.content, "Yes"),
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn if_else_returns_else_on_false() {
    let node = if_else(false, text("Yes"), text("No"));
    match node {
        Node::Text(t) => assert_eq!(t.content, "No"),
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn maybe_returns_node_on_some() {
    let node = maybe(Some("value"), text);
    match node {
        Node::Text(t) => assert_eq!(t.content, "value"),
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn maybe_returns_empty_on_none() {
    let node = maybe(None::<String>, text);
    assert!(matches!(node, Node::Empty));
}

#[test]
fn progress_bar_renders() {
    let node = progress_bar(0.5, 10);
    match node {
        Node::Text(t) => {
            assert_eq!(t.content.chars().count(), 10);
            assert!(t.content.contains('█'));
            assert!(t.content.contains('░'));
        }
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn progress_bar_clamps_values() {
    let zero = progress_bar(0.0, 10);
    let full = progress_bar(1.0, 10);
    let over = progress_bar(1.5, 10);
    let under = progress_bar(-0.5, 10);

    match (zero, full, over, under) {
        (Node::Text(z), Node::Text(f), Node::Text(o), Node::Text(u)) => {
            assert!(!z.content.contains('█'));
            assert!(!f.content.contains('░'));
            assert_eq!(f.content, o.content);
            assert_eq!(z.content, u.content);
        }
        _ => panic!("Expected Text nodes"),
    }
}

#[test]
fn divider_creates_repeated_char() {
    let node = divider('─', 20);
    match node {
        Node::Text(t) => {
            assert_eq!(t.content.chars().count(), 20);
            assert!(t.content.chars().all(|c| c == '─'));
        }
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn badge_wraps_with_spaces() {
    let node = badge("info", Style::new().bg(Color::Blue));
    match node {
        Node::Text(t) => {
            assert_eq!(t.content, " info ");
            assert_eq!(t.style.bg, Some(Color::Blue));
        }
        _ => panic!("Expected Text node"),
    }
}

#[test]
fn key_value_creates_row() {
    let node = key_value("Name", "Alice");
    match node {
        Node::Box(b) => {
            assert_eq!(b.direction, Direction::Row);
            assert_eq!(b.children.len(), 2);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn bullet_list_creates_column() {
    let node = bullet_list(["Item 1", "Item 2", "Item 3"]);
    match node {
        Node::Box(b) => {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.children.len(), 3);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn numbered_list_creates_column() {
    let node = numbered_list(["First", "Second"]);
    match node {
        Node::Box(b) => {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.children.len(), 2);
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn spacer_fills_remaining_space_in_row() {
    use crate::tui::oil::render::render_to_string;

    let node = row([text("Left"), spacer(), text("Right")]);
    let output = render_to_string(&node, 20);

    assert_eq!(output.len(), 20);
    assert!(output.starts_with("Left"));
    assert!(output.ends_with("Right"));
}

#[test]
fn multiple_spacers_distribute_space_evenly() {
    use crate::tui::oil::render::render_to_string;

    let node = row([text("A"), spacer(), text("B"), spacer(), text("C")]);
    let output = render_to_string(&node, 17);

    assert_eq!(output.len(), 17);
    assert!(output.starts_with("A"));
    assert!(output.ends_with("C"));
    assert!(output.contains("B"));
}

#[test]
fn flex_with_weights_distributes_proportionally() {
    use crate::tui::oil::render::render_to_string;

    let node = row([
        text("X"),
        flex(1, text("")),
        text("Y"),
        flex(2, text("")),
        text("Z"),
    ]);
    let output = render_to_string(&node, 12);

    assert_eq!(output.len(), 12);
    assert!(output.starts_with("X"));
    assert!(output.ends_with("Z"));
}
