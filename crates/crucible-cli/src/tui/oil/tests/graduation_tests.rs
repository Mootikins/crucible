use crate::tui::oil::*;

mod viewport_layout_tests {
    use super::*;

    #[test]
    fn viewport_with_input_at_top_no_streaming() {
        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("msg-1", [text("User question")]),
            scrollback("msg-2", [text("Assistant answer")]),
            text_input("next question", 13),
        ]);

        runtime.render(&tree);

        let viewport = runtime.viewport_content();
        assert!(
            viewport.contains("next question"),
            "Input should be in viewport"
        );
        assert!(
            !viewport.contains("User question"),
            "Graduated content should not be in viewport"
        );
        assert!(
            !viewport.contains("Assistant answer"),
            "Graduated content should not be in viewport"
        );
    }

    #[test]
    fn viewport_input_appears_before_streaming_content() {
        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("msg-1", [text("Previous message")]),
            text_input("typing here", 11),
            text("Streaming response..."),
        ]);

        runtime.render(&tree);

        let viewport = runtime.viewport_content();
        let input_pos = viewport.find("typing here");
        let streaming_pos = viewport.find("Streaming");

        assert!(
            input_pos.is_some(),
            "Input should be in viewport, got: {:?}",
            viewport
        );
        assert!(
            streaming_pos.is_some(),
            "Streaming should be in viewport, got: {:?}",
            viewport
        );

        if let (Some(inp), Some(stream)) = (input_pos, streaming_pos) {
            assert!(
                inp < stream,
                "Input ({}) should appear before streaming content ({})",
                inp,
                stream
            );
        }
    }

    #[test]
    fn all_completed_messages_graduate_to_stdout() {
        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("user-1", [text("First question")]),
            scrollback("assistant-1", [text("First answer")]),
            scrollback("user-2", [text("Second question")]),
            scrollback("assistant-2", [text("Second answer")]),
            text_input("", 0),
        ]);

        runtime.render(&tree);

        let stdout = runtime.stdout_content();
        assert!(stdout.contains("First question"));
        assert!(stdout.contains("First answer"));
        assert!(stdout.contains("Second question"));
        assert!(stdout.contains("Second answer"));

        let viewport = runtime.viewport_content();
        assert!(!viewport.contains("First question"));
        assert!(!viewport.contains("Second answer"));
    }

    #[test]
    fn blank_line_between_graduated_content_and_viewport() {
        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("msg-1", [text("Graduated content")]),
            text(""),
            text_input("input", 5),
        ]);

        runtime.render(&tree);

        let stdout = runtime.stdout_content();
        assert!(
            stdout.ends_with("\r\n") || stdout.ends_with('\n'),
            "Stdout should end with newline for clean separation"
        );
    }
}

#[test]
fn static_node_graduates_to_stdout() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("Hello from scrollback")]),
        text("Viewport content"),
    ]);

    runtime.render(&tree);

    assert!(
        runtime.stdout_content().contains("Hello from scrollback"),
        "Static content should be in stdout"
    );
    assert!(
        !runtime.viewport_content().contains("Hello from scrollback"),
        "Graduated content should not be in viewport"
    );
    assert!(
        runtime.viewport_content().contains("Viewport content"),
        "Non-static content should be in viewport"
    );
}

#[test]
fn static_node_only_graduates_once() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = scrollback("msg-1", [text("Graduate me")]);

    runtime.render(&tree);
    runtime.render(&tree);

    assert_eq!(
        runtime.stdout_content().matches("Graduate me").count(),
        1,
        "Content should only appear once in stdout"
    );
}

#[test]
fn new_static_node_graduates_incrementally() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree1 = col([
        scrollback("msg-1", [text("First message")]),
        text("Viewport"),
    ]);
    runtime.render(&tree1);

    let tree2 = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
        text("Viewport"),
    ]);
    runtime.render(&tree2);

    let stdout = runtime.stdout_content();
    assert_eq!(
        stdout.matches("First message").count(),
        1,
        "First message should appear exactly once"
    );
    assert_eq!(
        stdout.matches("Second message").count(),
        1,
        "Second message should appear exactly once"
    );
}

#[test]
fn continuation_appends_without_newline() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1-part-1", [text("Hello ")]),
        scrollback_continuation("msg-1-part-2", [text("world!")]),
    ]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    assert!(
        stdout.contains("Hello world!"),
        "Continuation should append without newline, got: {:?}",
        stdout
    );
}

#[test]
fn newline_between_separate_messages() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("First")]),
        scrollback("msg-2", [text("Second")]),
    ]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    assert!(
        stdout.contains("First\nSecond") || stdout.contains("First\r\nSecond"),
        "Should have newline between messages, got: {:?}",
        stdout
    );
}

#[test]
fn graduated_count_tracks_keys() {
    let mut runtime = TestRuntime::new(80, 24);

    assert_eq!(runtime.graduated_count(), 0);

    let tree1 = scrollback("msg-1", [text("First")]);
    runtime.render(&tree1);
    assert_eq!(runtime.graduated_count(), 1);

    let tree2 = col([
        scrollback("msg-1", [text("First")]),
        scrollback("msg-2", [text("Second")]),
    ]);
    runtime.render(&tree2);
    assert_eq!(runtime.graduated_count(), 2);

    runtime.render(&tree2);
    assert_eq!(runtime.graduated_count(), 2, "Should not double-count");
}

#[test]
fn empty_static_node_not_graduated() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = scrollback("empty", [Node::Empty]);
    runtime.render(&tree);

    assert_eq!(
        runtime.graduated_count(),
        0,
        "Empty static should not graduate"
    );
    assert_eq!(runtime.stdout_content(), "");
}

#[test]
fn nested_static_in_box_graduates() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        col([scrollback("nested", [text("Nested content")])]),
        text("Viewport"),
    ]);

    runtime.render(&tree);

    assert!(runtime.stdout_content().contains("Nested content"));
    assert!(!runtime.viewport_content().contains("Nested content"));
}

#[test]
fn cross_frame_newline_handling() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree1 = col([scrollback("msg-1", [text("First")]), text("Viewport")]);
    runtime.render(&tree1);

    let tree2 = col([
        scrollback("msg-1", [text("First")]),
        scrollback("msg-2", [text("Second")]),
        text("Viewport"),
    ]);
    runtime.render(&tree2);

    let stdout = runtime.stdout_content();
    assert!(stdout.contains("First"));
    assert!(stdout.contains("Second"));
    assert!(
        stdout.contains("First\r\nSecond") || stdout.contains("First\nSecond"),
        "Cross-frame graduation should have newline between messages, got: {:?}",
        stdout
    );
}

#[test]
fn nested_box_graduates_in_order() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        row([scrollback("a", [text("A")])]),
        col([scrollback("b", [text("B")]), scrollback("c", [text("C")])]),
        scrollback("d", [text("D")]),
    ]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    let pos_a = stdout.find('A').expect("A not found");
    let pos_b = stdout.find('B').expect("B not found");
    let pos_c = stdout.find('C').expect("C not found");
    let pos_d = stdout.find('D').expect("D not found");

    assert!(pos_a < pos_b, "A should come before B");
    assert!(pos_b < pos_c, "B should come before C");
    assert!(pos_c < pos_d, "C should come before D");
}

#[test]
fn nested_fragment_graduates_in_order() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = fragment([
        scrollback("x", [text("X")]),
        fragment([scrollback("y", [text("Y")]), scrollback("z", [text("Z")])]),
    ]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    let pos_x = stdout.find('X').expect("X not found");
    let pos_y = stdout.find('Y').expect("Y not found");
    let pos_z = stdout.find('Z').expect("Z not found");

    assert!(pos_x < pos_y, "X should come before Y");
    assert!(pos_y < pos_z, "Y should come before Z");
}

#[test]
fn deeply_nested_structure_graduates_correctly() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        col([col([col([scrollback("deep", [text("Deep content")])])])]),
        text("Viewport"),
    ]);

    runtime.render(&tree);

    assert!(runtime.stdout_content().contains("Deep content"));
    assert!(!runtime.viewport_content().contains("Deep content"));
    assert_eq!(runtime.graduated_count(), 1);
}

#[test]
fn graduation_skips_empty_static_content() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("empty-1", [Node::Empty]),
        scrollback("real", [text("Real content")]),
        scrollback("empty-2", [text("")]),
    ]);

    runtime.render(&tree);

    assert_eq!(
        runtime.graduated_count(),
        1,
        "Only non-empty should graduate"
    );
    assert!(runtime.stdout_content().contains("Real content"));
}

#[test]
fn multiline_content_preserves_newlines_in_graduation() {
    let mut runtime = TestRuntime::new(80, 24);

    let user_msg = scrollback(
        "user-1",
        [col([
            text("TOP_BORDER"),
            text(" > Hello world"),
            text("BOTTOM_BORDER"),
        ])],
    );

    let tree = col([user_msg, text_input("", 0)]);
    runtime.render(&tree);

    let stdout = runtime.stdout_content();

    assert!(stdout.contains("TOP_BORDER"), "stdout: {:?}", stdout);
    assert!(stdout.contains("Hello world"), "stdout: {:?}", stdout);
    assert!(stdout.contains("BOTTOM_BORDER"), "stdout: {:?}", stdout);

    let top_pos = stdout.find("TOP_BORDER").unwrap();
    let msg_pos = stdout.find("Hello world").unwrap();
    let bottom_pos = stdout.find("BOTTOM_BORDER").unwrap();

    assert!(top_pos < msg_pos);
    assert!(msg_pos < bottom_pos);

    let between_top_and_msg = &stdout[top_pos..msg_pos];
    let between_msg_and_bottom = &stdout[msg_pos..bottom_pos];

    assert!(
        between_top_and_msg.contains("\r\n") || between_top_and_msg.contains('\n'),
        "between: {:?}",
        between_top_and_msg
    );
    assert!(
        between_msg_and_bottom.contains("\r\n") || between_msg_and_bottom.contains('\n'),
        "between: {:?}",
        between_msg_and_bottom
    );
}

#[test]
fn stdout_delta_uses_terminal_line_endings() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("Line one")]),
        scrollback("msg-2", [text("Line two")]),
        text("Viewport"),
    ]);

    runtime.render(&tree);

    let snapshot = runtime.last_snapshot().expect("should have snapshot");
    let delta = &snapshot.stdout_delta;

    assert!(
        delta.contains("\r\n"),
        "stdout_delta should use \\r\\n for terminal compatibility, got: {:?}",
        delta
    );
    assert!(
        !delta.contains("\n\n"),
        "should not have bare \\n\\n (double newline without \\r)"
    );
}

#[test]
fn stdout_delta_matches_accumulated_stdout() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree1 = scrollback("msg-1", [text("First message")]);
    runtime.render(&tree1);
    let delta1 = runtime
        .last_snapshot()
        .map(|s| s.stdout_delta.clone())
        .unwrap_or_default();

    let tree2 = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
    ]);
    runtime.render(&tree2);
    let delta2 = runtime
        .last_snapshot()
        .map(|s| s.stdout_delta.clone())
        .unwrap_or_default();

    let expected_stdout = format!("{}{}", delta1, delta2);
    assert_eq!(
        runtime.stdout_content(),
        expected_stdout,
        "Accumulated stdout should equal sum of deltas"
    );
}
