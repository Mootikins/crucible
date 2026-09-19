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

/// A uniform kind column (every visible item tagged the same, e.g. the model
/// picker where each row is kind="model") is pure noise — the renderer must
/// suppress it. Mixed kinds still render so the column can disambiguate.
#[test]
fn popup_hides_kind_column_when_uniform() {
    let items = vec![
        PopupItemNode {
            label: "llama3.2".into(),
            description: None,
            kind: Some("model".into()),
        },
        PopupItemNode {
            label: "gpt-4o".into(),
            description: None,
            kind: Some("model".into()),
        },
    ];
    let node = popup(items, 0, 10);
    let output = render_to_string(&node, 80);

    assert!(output.contains("llama3.2"), "labels must render: {output}");
    assert!(
        !output.contains("model llama3.2"),
        "uniform kind must not prefix labels: {output}"
    );
}

#[test]
fn popup_shows_kind_column_when_mixed() {
    // sample_items has kinds tool/tool/command — mixed, so kinds render.
    let node = popup(sample_items(), 0, 10);
    let output = render_to_string(&node, 80);

    assert!(
        output.contains("tool search"),
        "mixed kinds keep the kind column: {output}"
    );
}

/// Minimal (nvim-pmenu-style) popup: anchored at a column, sized to its
/// content, so it only occludes its own rectangle instead of a full strip.
#[test]
fn popup_anchored_renders_content_width_at_anchor_column() {
    let items = vec![
        PopupItemNode {
            label: "alpha.md".into(),
            description: None,
            kind: Some("file".into()),
        },
        PopupItemNode {
            label: "beta_notes.md".into(),
            description: None,
            kind: Some("file".into()),
        },
    ];
    let node = match popup(items, 0, 10) {
        Node::Popup(p) => Node::Popup(p.anchored(10)),
        other => other,
    };
    let output = render_to_string(&node, 80);
    let stripped = crucible_oil::ansi::strip_ansi(&output);
    let line = stripped
        .lines()
        .find(|l| l.contains("beta_notes.md"))
        .expect("item rendered");

    // 1-cell pad inside the box: label starts at anchor + 1.
    assert_eq!(
        line.find("beta_notes.md"),
        Some(11),
        "label should sit at anchor column + 1: {line:?}"
    );
    // Content-width box: longest label (13) + 2 pad = 15 wide from col 10.
    assert!(
        line.trim_end().len() <= 10 + 15,
        "anchored popup must not paint a full-width strip: {line:?}"
    );
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

/// Taffy allocates max_visible height for the popup even with 0 items,
/// producing background-colored blank lines. After stripping ANSI codes,
/// the content should be empty or whitespace-only with no item labels.
#[test]
fn popup_empty_items_renders_empty() {
    use crucible_oil::ansi::strip_ansi;

    let node = popup(vec![], 0, 10);
    let output = render_to_string(&node, 80);
    let stripped = strip_ansi(&output);

    assert!(
        !stripped.contains('▸'),
        "Empty popup should have no selection indicator"
    );
    assert!(
        stripped.trim().is_empty(),
        "Empty popup should have no visible text content, got: {:?}",
        stripped
    );
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
fn popup_in_chat_view_with_messages() {
    let messages: Vec<Node> = (0..10)
        .map(|i| {
            col([
                text(format!("User message {}", i)),
                text(format!("Assistant response {} with lots of content", i)),
            ])
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

mod completion_style_behavior {
    use crate::tui::oil::chat_app::OilChatApp;
    use crate::tui::oil::event::Event;
    use crate::tui::oil::tests::helpers::view_with_default_ctx;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crucible_oil::ansi::strip_ansi;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    /// Inline (@file) completion uses the minimal anchored popup by default:
    /// label aligned with the word being completed, not a full-width strip.
    #[test]
    fn file_popup_is_minimal_and_anchored_by_default() {
        use crucible_oil::planning::FramePlanner;

        let mut app = OilChatApp::default();
        app.set_workspace_files(vec!["alpha.rs".to_string(), "beta.rs".to_string()]);

        for c in "hello @".chars() {
            app.update(Event::Key(key(KeyCode::Char(c))));
        }

        let tree = view_with_default_ctx(&app);
        let mut planner = FramePlanner::new(80, 24);
        let snapshot = planner.plan(&tree);
        let stripped = strip_ansi(&snapshot.viewport_with_overlays(80));

        let line = stripped
            .lines()
            .find(|l| l.contains("beta.rs") && !l.contains("hello"))
            .expect("popup item rendered");
        // trigger '@' is at display col 9 (" > hello @"); word start = col 10.
        let col = line
            .find("beta.rs")
            .map(|b| line[..b].chars().count())
            .expect("label present");
        assert_eq!(
            col, 10,
            "minimal popup label should align with the completed word: {line:?}"
        );
    }

    /// `:set completion_style=panel` forces the classic full-width strip for
    /// inline completions too.
    #[test]
    fn completion_style_panel_forces_strip_for_inline() {
        use crucible_oil::planning::FramePlanner;

        let mut app = OilChatApp::default();
        app.set_workspace_files(vec!["alpha.rs".to_string(), "beta.rs".to_string()]);

        for c in ":set completion_style=panel".chars() {
            app.update(Event::Key(key(KeyCode::Char(c))));
        }
        app.update(Event::Key(key(KeyCode::Enter)));

        for c in "hello @".chars() {
            app.update(Event::Key(key(KeyCode::Char(c))));
        }

        let tree = view_with_default_ctx(&app);
        let mut planner = FramePlanner::new(80, 24);
        let snapshot = planner.plan(&tree);
        let stripped = strip_ansi(&snapshot.viewport_with_overlays(80));

        let line = stripped
            .lines()
            .find(|l| l.contains("beta.rs") && !l.contains("hello"))
            .expect("popup item rendered");
        // Panel strip: " ▸ "/"   " prefix puts the label at col 3.
        let col = line
            .find("beta.rs")
            .map(|b| line[..b].chars().count())
            .expect("label present");
        assert_eq!(col, 3, "panel style should left-anchor the strip: {line:?}");
    }
}

/// The completion popup sits above everything the prompt region draws.
///
/// It used to reserve a constant 3 lines, which was the footer height only
/// while the footer was one bar and the input was one line. Both are now
/// variable — an author can add rows, and a wrapped message grows the input —
/// so the reservation has to be derived from what is actually on screen.
///
/// These drive the **panel** style (`/`, a command trigger). The minimal
/// anchored style used for inline `@`/`[[` triggers draws at the trigger column
/// and never consults the offset, so it cannot exercise this.
mod popup_clears_the_prompt_region {
    use crate::tui::oil::chat_app::OilChatApp;
    use crate::tui::oil::event::Event;
    use crate::tui::oil::tests::helpers::view_with_default_ctx;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crucible_lua::statusline_items::{Element, Layout, StatusItem};
    use crucible_oil::ansi::strip_ansi;
    use crucible_oil::planning::FramePlanner;

    fn row(text: &str) -> Element {
        Element::Row(vec![StatusItem::Text(text.to_string())])
    }

    /// The composited frame with a `/` command popup open over `layout`.
    fn frame_with_panel_popup(layout: Layout, typed: &str) -> String {
        crate::tui::oil::theme::bars::set(layout);

        let mut app = OilChatApp::default();
        for c in typed.chars() {
            app.update(Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::empty(),
            )));
        }

        let tree = view_with_default_ctx(&app);
        let mut planner = FramePlanner::new(80, 24);
        strip_ansi(&planner.plan(&tree).viewport_with_overlays(80))
    }

    /// The prompt line is the thing a too-small reservation lands on: extra
    /// rows push the input up, and the popup follows only if it counted them.
    #[test]
    fn extra_rows_below_the_input_do_not_push_the_popup_onto_it() {
        let frame = frame_with_panel_popup(
            Layout {
                prompt: vec![Element::Input, row("<ROW-A>"), row("<ROW-B>")],
                ..Layout::default()
            },
            "/",
        );

        assert!(
            frame.contains("<ROW-A>") && frame.contains("<ROW-B>"),
            "the authored rows vanished:\n{frame}"
        );
        let prompt_line = frame
            .lines()
            .find(|l| l.trim_start().starts_with('>'))
            .unwrap_or_else(|| panic!("the input line survived:\n{frame}"));
        assert!(
            prompt_line.contains('/'),
            "the popup painted over the line being typed into:\n{frame}"
        );
    }

    #[test]
    fn a_bottom_region_row_does_not_push_the_popup_onto_the_input() {
        let frame = frame_with_panel_popup(
            Layout {
                prompt: vec![Element::Input],
                bottom: vec![row("<BOTTOM>")],
                ..Layout::default()
            },
            "/",
        );

        assert!(
            frame.contains("<BOTTOM>"),
            "the bottom row vanished:\n{frame}"
        );
        assert!(
            frame.lines().any(|l| l.trim_start().starts_with('>')),
            "the popup painted over the input:\n{frame}"
        );
    }
}
