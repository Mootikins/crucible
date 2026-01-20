use crate::tui::oil::*;

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

mod composer_stability_tests {
    use super::*;
    use crate::tui::oil::ansi::strip_ansi;
    use crate::tui::oil::app::{App, ViewContext};
    use crate::tui::oil::chat_app::InkChatApp;
    use crate::tui::oil::event::Event;
    use crate::tui::oil::focus::FocusContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn view_with_default_ctx(app: &InkChatApp) -> Node {
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        app.view(&ctx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn popup_overlay_does_not_affect_base_height() {
        use crate::tui::oil::planning::FramePlanner;

        let mut app = InkChatApp::default();
        app.set_workspace_files(vec![
            "file1.rs".to_string(),
            "file2.rs".to_string(),
            "file3.rs".to_string(),
        ]);

        let tree_hidden = view_with_default_ctx(&app);
        let output_hidden = render_to_string(&tree_hidden, 80);
        let base_lines_hidden = strip_ansi(&output_hidden).lines().count();

        app.update(Event::Key(key(KeyCode::Char('@'))));

        let tree_visible = view_with_default_ctx(&app);
        let mut planner = FramePlanner::new(80, 24);
        let snapshot = planner.plan(&tree_visible);

        let base_lines_visible = snapshot.plan.viewport.content.lines().count();
        let composited_lines = snapshot.viewport_with_overlays(80).lines().count();

        assert_eq!(
            base_lines_hidden, base_lines_visible,
            "Base viewport should have same height regardless of popup visibility ({} vs {})",
            base_lines_hidden, base_lines_visible
        );

        assert!(
            composited_lines > base_lines_visible,
            "Composited output with overlay should be taller ({}) than base ({})",
            composited_lines,
            base_lines_visible
        );
    }

    #[test]
    fn popup_overlay_preserves_input_status_at_bottom() {
        use crate::tui::oil::planning::FramePlanner;

        let mut app = InkChatApp::default();
        app.set_workspace_files(vec!["file1.rs".to_string()]);

        app.update(Event::Key(key(KeyCode::Char('@'))));

        let tree = view_with_default_ctx(&app);
        let mut planner = FramePlanner::new(80, 24);
        let snapshot = planner.plan(&tree);
        let composited = snapshot.viewport_with_overlays(80);
        let stripped = strip_ansi(&composited);
        let lines: Vec<&str> = stripped.lines().collect();

        let status_line_idx = lines
            .iter()
            .position(|l| l.contains("NORMAL") || l.contains("PLAN"));
        assert!(status_line_idx.is_some(), "Should have status bar");

        let status_idx = status_line_idx.unwrap();
        assert_eq!(
            status_idx,
            lines.len() - 1,
            "Status bar should be the last line (index {} of {})",
            status_idx,
            lines.len()
        );
    }

    #[test]
    fn input_height_stable_with_short_content() {
        let app = InkChatApp::default();
        let tree = view_with_default_ctx(&app);
        let output = render_to_string(&tree, 80);
        let height_empty = strip_ansi(&output).lines().count();

        let mut app_with_text = InkChatApp::default();
        app_with_text.update(Event::Key(key(KeyCode::Char('H'))));
        app_with_text.update(Event::Key(key(KeyCode::Char('i'))));

        let tree2 = view_with_default_ctx(&app_with_text);
        let output2 = render_to_string(&tree2, 80);
        let height_short = strip_ansi(&output2).lines().count();

        assert_eq!(
            height_empty, height_short,
            "Input height should be stable with short content that fits on one line"
        );
    }

    #[test]
    fn input_height_bounded_with_long_content() {
        use crate::tui::oil::chat_app::INPUT_MAX_CONTENT_LINES;

        let mut app = InkChatApp::default();
        for _ in 0..200 {
            app.update(Event::Key(key(KeyCode::Char('x'))));
        }

        let tree = view_with_default_ctx(&app);
        let output = render_to_string(&tree, 80);
        let height_long = strip_ansi(&output).lines().count();

        let app_empty = InkChatApp::default();
        let tree_empty = view_with_default_ctx(&app_empty);
        let output_empty = render_to_string(&tree_empty, 80);
        let height_empty = strip_ansi(&output_empty).lines().count();

        assert!(
            height_long > height_empty,
            "Long content should increase height (empty={}, long={})",
            height_empty,
            height_long
        );

        let max_growth = INPUT_MAX_CONTENT_LINES - 1;
        assert!(
            height_long <= height_empty + max_growth,
            "Long content height ({}) should be at most {} more than empty ({})",
            height_long,
            max_growth,
            height_empty
        );
    }
}
