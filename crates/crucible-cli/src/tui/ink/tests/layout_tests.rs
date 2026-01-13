use crate::tui::ink::*;

#[test]
fn text_layout_single_line() {
    let node = text("Hello");
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.rect.width, 80);
    assert_eq!(layout.rect.height, 1);
}

#[test]
fn text_layout_wrapping() {
    let node = text("Hello world this is a long line that should wrap");
    let layout = calculate_layout(&node, 20, 24);

    assert!(layout.rect.height > 1, "Should wrap to multiple lines");
}

#[test]
fn column_layout_stacks_vertically() {
    let node = col([text("Line 1"), text("Line 2"), text("Line 3")]);
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.children.len(), 3);
    assert_eq!(layout.rect.height, 3);

    assert_eq!(layout.children[0].rect.y, 0);
    assert_eq!(layout.children[1].rect.y, 1);
    assert_eq!(layout.children[2].rect.y, 2);
}

#[test]
fn row_layout_side_by_side() {
    let node = row([text("A"), text("B"), text("C")]);
    let layout = calculate_layout(&node, 90, 24);

    assert_eq!(layout.children.len(), 3);

    assert_eq!(layout.children[0].rect.x, 0);
    assert_eq!(layout.children[1].rect.x, 30);
    assert_eq!(layout.children[2].rect.x, 60);
}

#[test]
fn fixed_size_respected() {
    let node = Node::Box(BoxNode {
        children: vec![text("Content")],
        size: Size::Fixed(10),
        direction: Direction::Column,
        ..Default::default()
    });

    let layout = calculate_layout(&node, 80, 24);
    assert_eq!(layout.rect.height, 10);
}

#[test]
fn flex_distributes_space() {
    let node = col([
        Node::Box(BoxNode {
            children: vec![text("Fixed")],
            size: Size::Fixed(4),
            ..Default::default()
        }),
        Node::Box(BoxNode {
            children: vec![text("Flex")],
            size: Size::Flex(1),
            ..Default::default()
        }),
    ]);

    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.children[0].rect.height, 4);
    assert_eq!(layout.children[1].rect.height, 20);
}

#[test]
fn content_size_shrinks_to_fit() {
    let node = col([text("Line 1"), text("Line 2")]);
    let layout = calculate_layout(&node, 80, 100);

    assert_eq!(layout.rect.height, 2);
}

#[test]
fn padding_adds_space() {
    let node = text("X").with_padding(Padding::all(1));
    let layout = calculate_layout(&node, 80, 24);

    assert!(layout.rect.height >= 3);
}

#[test]
fn nested_layout_calculates_correctly() {
    let node = col([row([text("A"), text("B")]), row([text("C"), text("D")])]);

    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.children.len(), 2);
    assert_eq!(layout.children[0].children.len(), 2);
    assert_eq!(layout.children[1].children.len(), 2);
}

#[test]
fn empty_node_zero_height() {
    let node = Node::Empty;
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.rect.height, 0);
}

#[test]
fn spinner_single_line() {
    let node = spinner(Some("Loading...".into()), 0);
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.rect.height, 1);
}

#[test]
fn input_single_line() {
    let node = text_input("hello", 5);
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.rect.height, 1);
}

#[test]
fn static_node_layout_like_fragment() {
    let node = scrollback("key", [text("Line 1"), text("Line 2")]);
    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.rect.height, 2);
}

#[test]
fn chat_interface_layout() {
    let node = col([
        scrollback("msg-1", [text("User message")]),
        scrollback("msg-2", [text("Assistant reply")]),
        spacer(),
        text_input("", 0),
    ]);

    let layout = calculate_layout(&node, 80, 24);

    assert_eq!(layout.children.len(), 4);

    let messages_height = layout.children[0].rect.height + layout.children[1].rect.height;
    let spacer_height = layout.children[2].rect.height;
    let input_height = layout.children[3].rect.height;

    assert_eq!(messages_height + spacer_height + input_height, 24);
}

#[test]
fn multiple_flex_items_share_space() {
    let node = col([
        Node::Box(BoxNode {
            size: Size::Flex(1),
            children: vec![text("A")],
            ..Default::default()
        }),
        Node::Box(BoxNode {
            size: Size::Flex(1),
            children: vec![text("B")],
            ..Default::default()
        }),
    ]);

    let layout = calculate_layout(&node, 80, 20);

    assert_eq!(layout.children[0].rect.height, 10);
    assert_eq!(layout.children[1].rect.height, 10);
}

#[test]
fn weighted_flex() {
    let node = col([
        Node::Box(BoxNode {
            size: Size::Flex(1),
            children: vec![text("Small")],
            ..Default::default()
        }),
        Node::Box(BoxNode {
            size: Size::Flex(3),
            children: vec![text("Large")],
            ..Default::default()
        }),
    ]);

    let layout = calculate_layout(&node, 80, 20);

    assert_eq!(layout.children[0].rect.height, 5);
    assert_eq!(layout.children[1].rect.height, 15);
}
