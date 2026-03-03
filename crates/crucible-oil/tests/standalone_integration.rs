use crucible_oil::{col, scrollback, scrollback_with_kind, text, ElementKind, TestRuntime};

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn graduation_lifecycle_basic() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([scrollback("msg-1", [text("Hello")]), text("Live content")]);

    runtime.render(&tree);
    assert!(runtime.stdout_content().contains("Hello"));
    assert!(!runtime.viewport_content().contains("Hello"));
    assert!(runtime.viewport_content().contains("Live content"));

    runtime.render(&tree);
    assert!(runtime.stdout_content().contains("Hello"));
    assert_eq!(count_occurrences(runtime.stdout_content(), "Hello"), 1);
}

#[test]
fn multi_frame_graduation_xor_invariant() {
    let mut runtime = TestRuntime::new(80, 24);

    let frames = [
        col([scrollback("msg-1", [text("Message 1")]), text("live-1")]),
        col([
            scrollback("msg-1", [text("Message 1")]),
            scrollback("msg-2", [text("Message 2")]),
            text("live-2"),
        ]),
        col([
            scrollback("msg-1", [text("Message 1")]),
            scrollback("msg-2", [text("Message 2")]),
            scrollback("msg-3", [text("Message 3")]),
            text("live-3"),
        ]),
    ];

    for (idx, tree) in frames.iter().enumerate() {
        runtime.render(tree);
        let stdout = runtime.stdout_content();
        let viewport = runtime.viewport_content();

        for i in 1..=(idx + 1) {
            let marker = format!("Message {}", i);
            let in_stdout = stdout.contains(&marker);
            let in_viewport = viewport.contains(&marker);
            assert!(
                in_stdout ^ in_viewport,
                "{} must be in stdout XOR viewport",
                marker
            );
        }
    }

    let stdout = runtime.stdout_content();
    let viewport = runtime.viewport_content();
    for i in 1..=3 {
        let marker = format!("Message {}", i);
        assert!(stdout.contains(&marker));
        assert!(!viewport.contains(&marker));
    }
}

#[test]
fn viewport_resize_preserves_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([
        scrollback("msg-1", [text("Graduated before resize")]),
        text("live-line-1"),
        text("live-line-2"),
        text("live-line-3"),
        text("live-line-4"),
        text("live-line-5"),
        text("live-line-6"),
        text("live-line-7"),
        text("live-line-8"),
        text("live-line-9"),
        text("live-line-10"),
        text("live-line-11"),
        text("live-line-12"),
    ]);

    runtime.render(&tree);
    assert!(runtime.stdout_content().contains("Graduated before resize"));
    let before_resize_lines = runtime.viewport_content().lines().count();

    runtime.resize(80, 10);
    runtime.render(&tree);

    assert_eq!(runtime.height(), 10);
    assert!(runtime.stdout_content().contains("Graduated before resize"));
    assert_eq!(
        count_occurrences(runtime.stdout_content(), "Graduated before resize"),
        1
    );
    let after_resize = runtime.viewport_content();
    let after_resize_lines = after_resize.lines().count();
    assert!(after_resize.contains("live-line"));
    assert!(after_resize_lines <= before_resize_lines);
}

#[test]
fn test_runtime_accumulates_stdout() {
    let mut runtime = TestRuntime::new(80, 24);

    for i in 1..=6 {
        let mut nodes = Vec::new();
        for j in 1..=i {
            let key = format!("msg-{}", j);
            let content = format!("history-{}", j);
            nodes.push(scrollback(key, [text(content)]));
        }
        nodes.push(text(format!("live-{}", i)));
        runtime.render(&col(nodes));
    }

    let stdout = runtime.stdout_content();
    let viewport = runtime.viewport_content();

    for i in 1..=6 {
        let marker = format!("history-{}", i);
        assert!(stdout.contains(&marker));
        assert!(!viewport.contains(&marker));
    }
    assert!(viewport.contains("live-6"));
    assert!(!stdout.contains("live-6"));
}

#[test]
fn spacing_rules_between_elements() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([
        scrollback_with_kind("block-1", ElementKind::Block, [text("Block 1")]),
        scrollback_with_kind("block-2", ElementKind::Block, [text("Block 2")]),
        scrollback_with_kind("cont-1", ElementKind::Continuation, [text("cont-1")]),
        scrollback_with_kind("cont-2", ElementKind::Continuation, [text("cont-2")]),
        text("live"),
    ]);

    runtime.render(&tree);
    let stdout = runtime.stdout_content();

    assert!(stdout.contains("Block 1\r\n\r\nBlock 2"));
    assert!(stdout.contains("Block 2cont-1cont-2"));
    assert!(!stdout.contains("cont-1\r\n\r\ncont-2"));
}
