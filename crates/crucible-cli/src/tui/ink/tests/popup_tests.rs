use crate::tui::ink::*;

fn sample_items() -> Vec<PopupItemNode> {
    vec![
        PopupItemNode {
            label: "search".into(),
            description: Some("Search notes".into()),
            kind: Some("tool".into()),
        },
        PopupItemNode {
            label: "create".into(),
            description: Some("Create note".into()),
            kind: Some("tool".into()),
        },
        PopupItemNode {
            label: "/help".into(),
            description: Some("Show help".into()),
            kind: Some("command".into()),
        },
    ]
}

#[test]
fn popup_renders_items() {
    let node = popup(sample_items(), 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.contains("search"), "should show first item label");
    assert!(output.contains("create"), "should show second item label");
    assert!(output.contains("/help"), "should show third item label");
}

#[test]
fn popup_renders_descriptions() {
    let node = popup(sample_items(), 0, 10);
    let output = render_to_string(&node, 80);

    assert!(
        output.contains("Search notes"),
        "should show first description"
    );
    assert!(
        output.contains("Create note"),
        "should show second description"
    );
}

#[test]
fn popup_shows_selection_indicator() {
    let node = popup(sample_items(), 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.contains("▸"), "should show selection indicator");
}

#[test]
fn popup_selection_moves_with_index() {
    let items = sample_items();

    let output0 = render_to_string(&popup(items.clone(), 0, 10), 80);
    let output1 = render_to_string(&popup(items.clone(), 1, 10), 80);

    let indicator_pos_0 = output0.find('▸').expect("should have indicator");
    let indicator_pos_1 = output1.find('▸').expect("should have indicator");

    assert_ne!(
        indicator_pos_0, indicator_pos_1,
        "indicator should move with selection"
    );
}

#[test]
fn popup_empty_items_renders_empty() {
    let node = popup(vec![], 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn popup_item_without_description() {
    let items = vec![PopupItemNode {
        label: "simple".into(),
        description: None,
        kind: None,
    }];
    let node = popup(items, 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.contains("simple"));
}

#[test]
fn popup_respects_max_visible() {
    let items: Vec<PopupItemNode> = (0..20)
        .map(|i| PopupItemNode {
            label: format!("item{}", i),
            description: None,
            kind: None,
        })
        .collect();

    let node = popup(items, 0, 5);
    let output = render_to_string(&node, 80);

    assert!(output.contains("item0"));
    assert!(output.contains("item4"));
    assert!(
        !output.contains("item5"),
        "should not show items beyond max_visible"
    );
}

#[test]
fn popup_helper_creates_valid_node() {
    let node = popup(sample_items(), 1, 10);

    match node {
        Node::Popup(popup_node) => {
            assert_eq!(popup_node.selected, 1);
            assert_eq!(popup_node.max_visible, 10);
            assert_eq!(popup_node.items.len(), 3);
        }
        _ => panic!("popup() should return Node::Popup"),
    }
}

#[test]
fn popup_item_builder_chain() {
    let item = popup_item("label").desc("desc").kind("tool");

    assert_eq!(item.label, "label");
    assert_eq!(item.description, Some("desc".into()));
    assert_eq!(item.kind, Some("tool".into()));
}

#[test]
fn popup_node_default_viewport() {
    let items = sample_items();
    let node = popup(items, 0, 10);

    if let Node::Popup(popup_node) = node {
        assert_eq!(popup_node.viewport_offset, 0);
    }
}

#[test]
fn popup_renders_kind_indicator() {
    let items = vec![
        popup_item("tool_item").kind("tool"),
        popup_item("cmd_item").kind("command"),
    ];
    let node = popup(items, 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.contains("tool_item"));
    assert!(output.contains("cmd_item"));
}

#[test]
fn popup_in_chat_view_with_scrollback() {
    let messages: Vec<Node> = (0..10)
        .map(|i| {
            scrollback(
                format!("msg-{}", i),
                [col([
                    text(format!("User message {}", i)),
                    text(format!("Assistant response {} with lots of content", i)),
                ])],
            )
        })
        .collect();

    let popup_node = popup(sample_items(), 0, 10);

    let view = col([
        fragment(messages),
        spacer(),
        popup_node,
        text("▄".repeat(80)),
        text(" > input"),
        text("▀".repeat(80)),
        text(" [plan] │ Ready"),
    ]);

    let output = render_to_string(&view, 80);

    let popup_count = output.matches("▸").count();
    assert_eq!(
        popup_count, 1,
        "popup selection indicator should appear exactly once, found {}",
        popup_count
    );

    let search_count = output.matches("search").count();
    assert_eq!(
        search_count, 1,
        "popup item 'search' should appear exactly once, found {}",
        search_count
    );
}

#[test]
fn popup_positioned_above_input_bar() {
    let popup_node = popup(sample_items(), 0, 10);

    let view = col([
        text("Header content"),
        text("More content"),
        spacer(),
        popup_node,
        text("▄▄▄▄▄▄▄▄"),
        text(" > input"),
        text("▀▀▀▀▀▀▀▀"),
        text(" [plan]"),
    ]);

    let output = render_to_string(&view, 80);
    let lines: Vec<&str> = output.lines().collect();

    let popup_line_idx = lines.iter().position(|l| l.contains("▸")).unwrap();
    let input_line_idx = lines.iter().position(|l| l.contains(" > input")).unwrap();

    assert!(
        popup_line_idx < input_line_idx,
        "popup (line {}) should be above input bar (line {})",
        popup_line_idx,
        input_line_idx
    );
}
