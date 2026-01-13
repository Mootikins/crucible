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
