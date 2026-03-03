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
fn multi_frame_xor_invariant() {
    let mut runtime = TestRuntime::new(80, 24);

    let frame_1 = col([scrollback("msg-1", [text("First")]), text("live")]);
    runtime.render(&frame_1);
    assert!(runtime.stdout_content().contains("First"));
    assert!(!runtime.viewport_content().contains("First"));

    let frame_2 = col([
        scrollback("msg-1", [text("First")]),
        scrollback("msg-2", [text("Second")]),
        text("live"),
    ]);
    runtime.render(&frame_2);
    let stdout = runtime.stdout_content();
    let viewport = runtime.viewport_content();
    assert!(stdout.contains("First"));
    assert!(stdout.contains("Second"));
    assert!(!viewport.contains("First"));
    assert!(!viewport.contains("Second"));
}

#[test]
fn viewport_resize_preserves_graduated_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([scrollback("msg-1", [text("Persist me")]), text("live")]);

    runtime.render(&tree);
    assert!(runtime.stdout_content().contains("Persist me"));

    runtime.resize(80, 10);
    runtime.render(&tree);

    assert_eq!(runtime.height(), 10);
    assert!(runtime.stdout_content().contains("Persist me"));
    assert_eq!(count_occurrences(runtime.stdout_content(), "Persist me"), 1);
}

#[test]
fn test_runtime_accumulates_stdout_across_frames() {
    let mut runtime = TestRuntime::new(80, 24);

    for i in 1..=5 {
        let mut nodes = Vec::new();
        for j in 1..=i {
            let key = format!("msg-{}", j);
            let content = format!("msg-{}", j);
            nodes.push(scrollback(key, [text(content)]));
        }
        nodes.push(text(format!("live-{}", i)));
        runtime.render(&col(nodes));
    }

    let stdout = runtime.stdout_content();
    let viewport = runtime.viewport_content();

    for i in 1..=5 {
        let marker = format!("msg-{}", i);
        assert!(stdout.contains(&marker));
        assert!(!viewport.contains(&marker));
    }
    assert_eq!(runtime.graduated_count(), 5);
}

#[test]
fn spacing_between_block_elements() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([
        scrollback_with_kind("msg-1", ElementKind::Block, [text("Block A")]),
        scrollback_with_kind("msg-2", ElementKind::Block, [text("Block B")]),
    ]);

    runtime.render(&tree);
    let stdout = runtime.stdout_content();

    assert!(stdout.contains("Block A"));
    assert!(stdout.contains("Block B"));
    assert!(stdout.contains("Block A\r\n\r\nBlock B"));
}
