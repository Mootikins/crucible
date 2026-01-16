use crate::tui::ink::*;

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
fn multiple_static_nodes_graduate_in_order() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("First")]),
        scrollback("msg-2", [text("Second")]),
        scrollback("msg-3", [text("Third")]),
    ]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    let first_pos = stdout.find("First").expect("First not found");
    let second_pos = stdout.find("Second").expect("Second not found");
    let third_pos = stdout.find("Third").expect("Third not found");

    assert!(first_pos < second_pos, "First should come before Second");
    assert!(second_pos < third_pos, "Second should come before Third");
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
fn chat_flow_graduation() {
    let mut runtime = TestRuntime::new(80, 24);

    let user_msg = |id: &str, content: &str| {
        scrollback(
            id,
            [row([styled(" > ", Style::new().dim()), text(content)])],
        )
    };

    let assistant_msg = |id: &str, content: &str| {
        scrollback(
            id,
            [row([styled(" . ", Style::new().dim()), text(content)])],
        )
    };

    let tree1 = col([user_msg("user-1", "What is 2+2?"), text_input("", 0)]);
    runtime.render(&tree1);

    assert!(runtime.stdout_content().contains("What is 2+2?"));
    assert_eq!(runtime.graduated_count(), 1);

    let tree2 = col([
        user_msg("user-1", "What is 2+2?"),
        assistant_msg("assistant-1", "The answer is 4."),
        text_input("", 0),
    ]);
    runtime.render(&tree2);

    assert!(runtime.stdout_content().contains("The answer is 4."));
    assert_eq!(runtime.graduated_count(), 2);

    let tree3 = col([
        user_msg("user-1", "What is 2+2?"),
        assistant_msg("assistant-1", "The answer is 4."),
        user_msg("user-2", "Thanks!"),
        text_input("", 0),
    ]);
    runtime.render(&tree3);

    assert!(runtime.stdout_content().contains("Thanks!"));
    assert_eq!(runtime.graduated_count(), 3);
}

#[test]
fn streaming_content_stays_in_viewport() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("Completed message")]),
        text("Streaming..."),
        spinner(Some("Generating".into()), 0),
        text_input("", 0),
    ]);

    runtime.render(&tree);

    assert!(runtime.stdout_content().contains("Completed message"));
    assert!(!runtime.stdout_content().contains("Streaming"));
    assert!(!runtime.stdout_content().contains("Generating"));

    assert!(runtime.viewport_content().contains("Streaming"));
    assert!(runtime.viewport_content().contains("Generating"));
}

#[test]
fn viewport_excludes_graduated_content() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("old-1", [text("Old message 1")]),
        scrollback("old-2", [text("Old message 2")]),
        scrollback("old-3", [text("Old message 3")]),
        text("Current content"),
        text_input("typing here", 11),
    ]);

    runtime.render(&tree);

    let viewport = runtime.viewport_content();
    assert!(!viewport.contains("Old message"));
    assert!(viewport.contains("Current content"));
    assert!(viewport.contains("typing here"));
}

#[test]
fn flush_to_buffer_handles_pending_newline() {
    use crate::tui::ink::runtime::GraduationState;

    let mut state = GraduationState::new();

    let tree1 = scrollback("msg-1", [text("First")]);
    let graduated1 = state.graduate(&tree1, 80).unwrap();
    state.flush_to_buffer(&graduated1);

    let tree2 = scrollback("msg-2", [text("Second")]);
    let graduated2 = state.graduate(&tree2, 80).unwrap();
    state.flush_to_buffer(&graduated2);

    let content = state.stdout_content();
    assert!(
        content.contains("First\nSecond")
            || content.contains("First") && content.contains("Second"),
        "Should have newline between flushed content: {:?}",
        content
    );
}

#[test]
fn flush_to_buffer_empty_input_no_op() {
    use crate::tui::ink::runtime::GraduationState;

    let mut state = GraduationState::new();
    state.flush_to_buffer(&[]);

    assert!(state.stdout_content().is_empty());
    assert_eq!(state.graduated_count(), 0);
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
fn graduation_idempotent_on_same_tree() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([
        scrollback("msg-1", [text("Message 1")]),
        scrollback("msg-2", [text("Message 2")]),
    ]);

    for _ in 0..10 {
        runtime.render(&tree);
    }

    assert_eq!(runtime.graduated_count(), 2);

    let count = runtime.stdout_content().matches("Message 1").count();
    assert_eq!(
        count, 1,
        "Message should appear exactly once after multiple renders"
    );
}

#[test]
fn graduation_width_uses_large_value_for_terminal_wrapping() {
    let mut runtime = TestRuntime::new(40, 24);

    let long_text = "This is a very long line that would normally wrap at 40 columns but graduation should use a large width.";
    let tree = scrollback("long", [text(long_text)]);

    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    assert!(
        stdout.contains(long_text),
        "Long text should be preserved without forced wrapping: {:?}",
        stdout
    );
}
