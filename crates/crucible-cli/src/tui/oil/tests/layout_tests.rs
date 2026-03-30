use crate::tui::oil::*;

#[test]
fn text_layout_single_line() {
    let node = text("Hello");
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.rect.width, 80);
    assert_eq!(layout.rect.height, 1);
}

#[test]
fn text_layout_wrapping() {
    let node = text("Hello world this is a long line that should wrap");
    let layout = build_layout_tree(&node, 20, 24).root;

    assert!(layout.rect.height > 1, "Should wrap to multiple lines");
}

#[test]
fn column_layout_stacks_vertically() {
    let node = col([text("Line 1"), text("Line 2"), text("Line 3")]);
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.children.len(), 3);
    assert_eq!(layout.rect.height, 3);

    assert_eq!(layout.children[0].rect.y, 0);
    assert_eq!(layout.children[1].rect.y, 1);
    assert_eq!(layout.children[2].rect.y, 2);
}

#[test]
fn row_layout_side_by_side() {
    let node = row([text("A"), text("B"), text("C")]);
    let layout = build_layout_tree(&node, 90, 24).root;

    assert_eq!(layout.children.len(), 3);

    // Row children are content-sized: "A"=1, "B"=1, "C"=1
    assert_eq!(layout.children[0].rect.x, 0);
    assert_eq!(layout.children[0].rect.width, 1);
    assert!(
        layout.children[1].rect.x > layout.children[0].rect.x,
        "B should be after A"
    );
    assert!(
        layout.children[2].rect.x > layout.children[1].rect.x,
        "C should be after B"
    );
}

#[test]
fn fixed_size_respected() {
    let node = Node::Box(BoxNode {
        children: vec![text("Content")],
        size: Size::Fixed(10),
        direction: Direction::Column,
        ..Default::default()
    });

    let layout = build_layout_tree(&node, 80, 24).root;
    assert_eq!(layout.rect.height, 10);
}

#[test]
fn flex_distributes_space() {
    // Root must have a fixed height for flex distribution to work in Taffy.
    // With Size::Content (default), the root shrinks to content and there's
    // no remaining space for flex children to grow into.
    let node = Node::Box(BoxNode {
        children: vec![
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
        ],
        size: Size::Fixed(24),
        ..Default::default()
    });

    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.children[0].rect.height, 4);
    assert_eq!(layout.children[1].rect.height, 20);
}

#[test]
fn content_size_shrinks_to_fit() {
    let node = col([text("Line 1"), text("Line 2")]);
    let layout = build_layout_tree(&node, 80, 100).root;

    assert_eq!(layout.rect.height, 2);
}

#[test]
fn padding_adds_space() {
    let node = text("X").with_padding(Padding::all(1));
    let layout = build_layout_tree(&node, 80, 24).root;

    assert!(layout.rect.height >= 3);
}

#[test]
fn nested_layout_calculates_correctly() {
    let node = col([row([text("A"), text("B")]), row([text("C"), text("D")])]);

    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.children.len(), 2);
    assert_eq!(layout.children[0].children.len(), 2);
    assert_eq!(layout.children[1].children.len(), 2);
}

#[test]
fn empty_node_zero_height() {
    let node = Node::Empty;
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.rect.height, 0);
}

#[test]
fn spinner_single_line() {
    let node = spinner(Some("Loading...".into()), 0);
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.rect.height, 1);
}

#[test]
fn input_single_line() {
    let node = text_input("hello", 5);
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.rect.height, 1);
}

#[test]
fn col_node_layout_stacks() {
    let node = col([text("Line 1"), text("Line 2")]);
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.rect.height, 2);
}

#[test]
fn chat_interface_layout() {
    // Fixed-size root needed for spacer (flex) to fill remaining space.
    let node = Node::Box(BoxNode {
        children: vec![
            text("User message"),
            text("Assistant reply"),
            spacer(),
            text_input("", 0),
        ],
        size: Size::Fixed(24),
        ..Default::default()
    });

    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.children.len(), 4);

    let messages_height = layout.children[0].rect.height + layout.children[1].rect.height;
    let spacer_height = layout.children[2].rect.height;
    let input_height = layout.children[3].rect.height;

    assert_eq!(messages_height + spacer_height + input_height, 24);
}

#[test]
fn multiple_flex_items_share_space() {
    let node = Node::Box(BoxNode {
        children: vec![
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
        ],
        size: Size::Fixed(20),
        ..Default::default()
    });

    let layout = build_layout_tree(&node, 80, 20).root;

    assert_eq!(layout.children[0].rect.height, 10);
    assert_eq!(layout.children[1].rect.height, 10);
}

#[test]
fn weighted_flex() {
    let node = Node::Box(BoxNode {
        children: vec![
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
        ],
        size: Size::Fixed(20),
        ..Default::default()
    });

    let layout = build_layout_tree(&node, 80, 20).root;

    // Taffy distributes remaining space (after 1-line intrinsic per child)
    // proportionally: flex(1) gets ~5-6, flex(3) gets ~14-15 depending on rounding
    let small_h = layout.children[0].rect.height;
    let large_h = layout.children[1].rect.height;
    assert_eq!(small_h + large_h, 20, "total should fill container");
    assert!(large_h > small_h, "flex(3) should be larger than flex(1)");
}

#[test]
fn gap_adds_space_between_children() {
    let node = col([text("A"), text("B"), text("C")]).gap(Gap::row(2));
    let layout = build_layout_tree(&node, 80, 24).root;

    assert_eq!(layout.children.len(), 3);
    assert_eq!(layout.children[0].rect.y, 0);
    assert_eq!(layout.children[1].rect.y, 3);
    assert_eq!(layout.children[2].rect.y, 6);
}

#[test]
fn margin_adds_external_space() {
    let node = col([text("Content").with_margin(Padding::all(2))]);
    let layout = build_layout_tree(&node, 80, 24).root;

    assert!(layout.children[0].rect.y >= 2);
}

#[test]
fn builder_methods_work() {
    let node = col([text("A"), text("B")])
        .justify(JustifyContent::Center)
        .align(AlignItems::Center)
        .gap(Gap::all(1));

    match node {
        Node::Box(b) => {
            assert_eq!(b.justify, JustifyContent::Center);
            assert_eq!(b.align, AlignItems::Center);
            assert_eq!(b.gap, Gap::all(1));
        }
        _ => panic!("Expected Box node"),
    }
}

#[test]
fn row_wrapped_spacer_fills_remaining_width() {
    let node = row([text(" NORMAL "), spacer(), text("50% ctx")]);

    let layout = build_layout_tree(&node, 80, 4).root;

    assert_eq!(layout.children.len(), 3);
    assert_eq!(layout.children[2].rect.width, 7);
    assert_eq!(
        layout.children[2].rect.x, 73,
        "right segment should be right-aligned when spacer expands"
    );
}

#[test]
fn gap_types_work() {
    assert_eq!(Gap::all(5), Gap { row: 5, column: 5 });
    assert_eq!(Gap::row(3), Gap { row: 3, column: 0 });
    assert_eq!(Gap::column(4), Gap { row: 0, column: 4 });
    assert_eq!(Gap::new(2, 3), Gap { row: 2, column: 3 });
}
