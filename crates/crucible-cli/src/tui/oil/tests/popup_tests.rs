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

mod overlay_graduation_tests {
    use super::*;
    use crate::tui::oil::ansi::strip_ansi;
    use crate::tui::oil::app::{App, ViewContext};
    use crate::tui::oil::chat_app::OilChatApp;
    use crate::tui::oil::event::Event;
    use crate::tui::oil::focus::FocusContext;
    use crate::tui::oil::planning::FramePlanner;
    use crate::tui::oil::TestRuntime;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn view_with_default_ctx(app: &OilChatApp) -> Node {
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        app.view(&ctx)
    }

    #[test]
    fn slash_backspace_sequence_no_popup_duplication() {
        let mut app = OilChatApp::default();
        let mut planner = FramePlanner::new(80, 24);

        app.update(Event::Key(key(KeyCode::Char('/'))));

        let tree = view_with_default_ctx(&app);
        let snapshot = planner.plan(&tree);
        let composited = snapshot.viewport_with_overlays(80);
        let stripped = strip_ansi(&composited);

        let indicator_count = stripped.matches("▸").count();
        assert_eq!(
            indicator_count, 1,
            "After typing /, popup should appear exactly once, found {}. Output:\n{}",
            indicator_count, stripped
        );

        app.update(Event::Key(key(KeyCode::Backspace)));

        let tree2 = view_with_default_ctx(&app);
        let snapshot2 = planner.plan(&tree2);
        let composited2 = snapshot2.viewport_with_overlays(80);
        let stripped2 = strip_ansi(&composited2);

        let indicator_count2 = stripped2.matches("▸").count();
        assert_eq!(
            indicator_count2, 0,
            "After backspace deleting /, popup should be gone, found {}. Output:\n{}",
            indicator_count2, stripped2
        );
    }

    #[test]
    fn composited_viewport_height_includes_overlays() {
        let base_content = "line1\nline2\nline3";
        let overlays = [crate::tui::oil::planning::RenderedOverlay {
            lines: vec!["overlay1".into(), "overlay2".into(), "overlay3".into()],
            anchor: crate::tui::oil::OverlayAnchor::FromBottom(1),
        }];

        let composited = crate::tui::oil::composite_overlays(
            &base_content.lines().map(String::from).collect::<Vec<_>>(),
            &overlays
                .iter()
                .map(|o| crate::tui::oil::Overlay {
                    lines: o.lines.clone(),
                    anchor: o.anchor,
                })
                .collect::<Vec<_>>(),
            80,
        );

        assert!(
            composited.len() >= 3,
            "Composited should include space for overlay, got {} lines",
            composited.len()
        );
    }

    #[test]
    fn overlay_popup_not_duplicated_with_graduation() {
        let mut runtime = TestRuntime::new(80, 24);

        // Chat view with scrollback messages and overlay popup
        let tree = col([
            scrollback("msg-1", [text("User message")]),
            scrollback("msg-2", [text("Assistant response")]),
            spacer(),
            text("▄".repeat(80)),
            text(" > /"),
            text("▀".repeat(80)),
            text(" NORMAL │ Ready"),
            overlay_from_bottom(popup(sample_items(), 0, 10), 4), // offset from bottom: input + status
        ]);

        runtime.render(&tree);

        let viewport = runtime.viewport_content();
        let snapshot = runtime.last_snapshot().expect("should have snapshot");
        let composited = snapshot.viewport_with_overlays(80);

        // Popup should NOT appear in base viewport (it's an overlay)
        let base_popup_count = viewport.matches("▸").count();
        assert_eq!(
            base_popup_count, 0,
            "Popup should not appear in base viewport (it's an overlay), found {} occurrences",
            base_popup_count
        );

        // Popup SHOULD appear exactly once in composited output
        let composited_popup_count = composited.matches("▸").count();
        assert_eq!(
            composited_popup_count, 1,
            "Popup should appear exactly once in composited output, found {}",
            composited_popup_count
        );

        // Search label should appear exactly once in composited output
        let search_count = composited.matches("search").count();
        assert_eq!(
            search_count, 1,
            "Popup item 'search' should appear exactly once, found {}",
            search_count
        );
    }

    #[test]
    fn overlay_popup_appears_after_graduation() {
        let mut runtime = TestRuntime::new(80, 24);

        // First render: messages graduate
        let tree1 = col([
            scrollback("msg-1", [text("First message")]),
            scrollback("msg-2", [text("Second message")]),
            text_input("typing here", 11),
        ]);
        runtime.render(&tree1);
        assert_eq!(runtime.graduated_count(), 2, "Messages should graduate");

        // Second render: add overlay popup
        let tree2 = col([
            scrollback("msg-1", [text("First message")]),
            scrollback("msg-2", [text("Second message")]),
            text_input("@", 1),
            overlay_from_bottom(popup(sample_items(), 0, 10), 1),
        ]);
        runtime.render(&tree2);

        let snapshot = runtime.last_snapshot().expect("should have snapshot");
        let composited = snapshot.viewport_with_overlays(80);

        // Popup should appear exactly once
        let popup_count = composited.matches("▸").count();
        assert_eq!(
            popup_count, 1,
            "Popup should appear exactly once after graduation, found {}",
            popup_count
        );
    }

    #[test]
    fn overlay_compositing_does_not_include_graduated_content() {
        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("msg-1", [text("GRADUATED_MARKER")]),
            text("VIEWPORT_MARKER"),
            overlay_from_bottom(popup(sample_items(), 0, 5), 1),
        ]);

        runtime.render(&tree);

        // Graduated content should be in stdout, not viewport
        assert!(
            runtime.stdout_content().contains("GRADUATED_MARKER"),
            "Graduated content should be in stdout"
        );

        let snapshot = runtime.last_snapshot().expect("should have snapshot");
        let composited = snapshot.viewport_with_overlays(80);

        // Composited output should NOT include graduated content
        assert!(
            !composited.contains("GRADUATED_MARKER"),
            "Graduated content should not be in composited viewport"
        );

        // But should include viewport content and popup
        assert!(
            composited.contains("VIEWPORT_MARKER"),
            "Viewport content should be in composited output"
        );
        assert!(
            composited.contains("search"),
            "Popup content should be in composited output"
        );
    }
}

mod composer_stability_tests {
    use super::*;
    use crate::tui::oil::ansi::strip_ansi;
    use crate::tui::oil::app::{App, ViewContext};
    use crate::tui::oil::chat_app::OilChatApp;
    use crate::tui::oil::event::Event;
    use crate::tui::oil::focus::FocusContext;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn view_with_default_ctx(app: &OilChatApp) -> Node {
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

        let mut app = OilChatApp::default();
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

        let mut app = OilChatApp::default();
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
        let app = OilChatApp::default();
        let tree = view_with_default_ctx(&app);
        let output = render_to_string(&tree, 80);
        let height_empty = strip_ansi(&output).lines().count();

        let mut app_with_text = OilChatApp::default();
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

        let mut app = OilChatApp::default();
        for _ in 0..200 {
            app.update(Event::Key(key(KeyCode::Char('x'))));
        }

        let tree = view_with_default_ctx(&app);
        let output = render_to_string(&tree, 80);
        let height_long = strip_ansi(&output).lines().count();

        let app_empty = OilChatApp::default();
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
