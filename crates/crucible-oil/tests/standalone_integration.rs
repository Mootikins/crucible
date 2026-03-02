//! Standalone integration tests for crucible-oil.
//!
//! These tests prove that crucible-oil works as a standalone TUI framework
//! with zero crucible-* domain dependencies. All imports are from `crucible_oil` only.

use crucible_oil::{
    col, scrollback, scrollback_continuation, scrollback_with_kind, text, ElementKind, TestRuntime,
};

/// Test 1: Basic graduation lifecycle.
///
/// Verifies that scrollback nodes graduate to stdout on first render,
/// and that live nodes remain in the viewport.
#[test]
fn graduation_lifecycle_basic() {
    let mut runtime = TestRuntime::new(80, 24);

    let tree = col([scrollback("msg-1", [text("Hello")]), text("Live content")]);

    // First render: msg-1 graduates to stdout
    runtime.render(&tree);

    assert!(
        runtime.stdout_content().contains("Hello"),
        "Graduated content should appear in stdout"
    );
    assert!(
        !runtime.viewport_content().contains("Hello"),
        "Graduated content should NOT appear in viewport"
    );
    assert!(
        runtime.viewport_content().contains("Live content"),
        "Live content should appear in viewport"
    );

    // Second render with same tree: msg-1 already graduated, no duplicate
    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    let hello_count = stdout.matches("Hello").count();
    assert_eq!(
        hello_count, 1,
        "Graduated content should appear exactly once in stdout, not duplicated"
    );
}

/// Test 2: XOR invariant across multiple frames.
///
/// Content must appear in exactly one of stdout OR viewport, never both.
#[test]
fn multi_frame_xor_invariant() {
    let mut runtime = TestRuntime::new(80, 24);

    // Frame 1: msg-1 graduates
    let tree1 = col([scrollback("msg-1", [text("First message")]), text("live")]);
    runtime.render(&tree1);

    assert!(
        runtime.stdout_content().contains("First message"),
        "msg-1 should be in stdout"
    );
    assert!(
        !runtime.viewport_content().contains("First message"),
        "msg-1 should NOT be in viewport"
    );

    // Frame 2: msg-2 graduates, msg-1 stays in stdout
    let tree2 = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
        text("live"),
    ]);
    runtime.render(&tree2);

    assert!(
        runtime.stdout_content().contains("First message"),
        "msg-1 still in stdout"
    );
    assert!(
        runtime.stdout_content().contains("Second message"),
        "msg-2 should be in stdout"
    );
    assert!(
        !runtime.viewport_content().contains("First message"),
        "msg-1 NOT in viewport"
    );
    assert!(
        !runtime.viewport_content().contains("Second message"),
        "msg-2 NOT in viewport"
    );

    // Frame 3: msg-3 graduates
    let tree3 = col([
        scrollback("msg-1", [text("First message")]),
        scrollback("msg-2", [text("Second message")]),
        scrollback("msg-3", [text("Third message")]),
        text("live"),
    ]);
    runtime.render(&tree3);

    assert!(
        runtime.stdout_content().contains("Third message"),
        "msg-3 should be in stdout"
    );
    assert!(
        !runtime.viewport_content().contains("Third message"),
        "msg-3 NOT in viewport"
    );

    // All 3 graduated
    assert_eq!(
        runtime.graduated_count(),
        3,
        "Should have 3 graduated nodes"
    );
}

/// Test 3: Viewport resize preserves graduated content.
///
/// Resizing the terminal should not lose already-graduated content.
#[test]
fn viewport_resize_preserves_graduated_content() {
    let mut runtime = TestRuntime::new(80, 24);

    // Graduate some content
    let tree = col([
        scrollback("msg-1", [text("Graduated content")]),
        text("Live"),
    ]);
    runtime.render(&tree);

    assert!(runtime.stdout_content().contains("Graduated content"));

    // Resize to smaller height
    runtime.resize(80, 10);
    assert_eq!(runtime.height(), 10, "Height should update after resize");
    assert_eq!(runtime.width(), 80, "Width should remain unchanged");

    // Render again after resize
    runtime.render(&tree);

    // Graduated content should still be in stdout (not lost)
    assert!(
        runtime.stdout_content().contains("Graduated content"),
        "Graduated content should survive resize"
    );
}

/// Test 4: TestRuntime accumulates stdout across frames.
///
/// Each frame's graduated content accumulates in stdout_content().
#[test]
fn test_runtime_accumulates_stdout_across_frames() {
    let mut runtime = TestRuntime::new(80, 24);

    // Render 5 frames, each adding a new scrollback node
    for i in 1..=5 {
        let key = format!("msg-{i}");
        let content = format!("Message number {i}");

        // Build tree with all previous messages + new one
        let mut nodes: Vec<_> = (1..=i)
            .map(|j| scrollback(format!("msg-{j}"), [text(format!("Message number {j}"))]))
            .collect();
        nodes.push(text("live viewport"));

        let tree = col(nodes);
        runtime.render(&tree);

        // Verify the new message graduated
        assert!(
            runtime.stdout_content().contains(&content),
            "Frame {i}: '{content}' should be in stdout"
        );
        assert!(
            !runtime.viewport_content().contains(&content),
            "Frame {i}: '{content}' should NOT be in viewport"
        );
        let _ = key; // suppress unused warning
    }

    // After all 5 frames, all messages should be in stdout
    assert_eq!(
        runtime.graduated_count(),
        5,
        "All 5 messages should be graduated"
    );

    for i in 1..=5 {
        assert!(
            runtime
                .stdout_content()
                .contains(&format!("Message number {i}")),
            "Message {i} should be in accumulated stdout"
        );
    }

    // Viewport should only show live content
    assert!(
        runtime.viewport_content().contains("live viewport"),
        "Live content should be in viewport"
    );
    for i in 1..=5 {
        assert!(
            !runtime
                .viewport_content()
                .contains(&format!("Message number {i}")),
            "Message {i} should NOT be in viewport (graduated)"
        );
    }
}

/// Test 5: Spacing rules between Block and Continuation elements.
///
/// Block elements get blank lines between them.
/// Continuation elements do NOT get extra blank lines.
#[test]
fn spacing_between_block_and_continuation_elements() {
    let mut runtime = TestRuntime::new(80, 24);

    // Two Block elements — should have blank line between them
    let tree = col([
        scrollback_with_kind("block-1", ElementKind::Block, [text("Block one")]),
        scrollback_with_kind("block-2", ElementKind::Block, [text("Block two")]),
        text("live"),
    ]);
    runtime.render(&tree);

    let stdout = runtime.stdout_content();
    assert!(
        stdout.contains("Block one"),
        "Block one should be in stdout"
    );
    assert!(
        stdout.contains("Block two"),
        "Block two should be in stdout"
    );

    // Block elements should have a blank line between them
    // Find positions of both blocks and check there's a blank line between
    let pos1 = stdout.find("Block one").expect("Block one not found");
    let pos2 = stdout.find("Block two").expect("Block two not found");
    let between = &stdout[pos1..pos2];
    assert!(
        between.contains("\n\n") || between.contains("\n \n") || between.contains("\r\n\r\n"),
        "Block elements should have blank line spacing between them, got: {:?}",
        between
    );

    // Now test Continuation elements — no blank lines between them
    let mut runtime2 = TestRuntime::new(80, 24);
    let tree2 = col([
        scrollback("cont-parent", [text("Parent block")]),
        scrollback_continuation("cont-1", [text("Continuation one")]),
        scrollback_continuation("cont-2", [text("Continuation two")]),
        text("live"),
    ]);
    runtime2.render(&tree2);

    let stdout2 = runtime2.stdout_content();
    assert!(
        stdout2.contains("Continuation one"),
        "Continuation one should be in stdout"
    );
    assert!(
        stdout2.contains("Continuation two"),
        "Continuation two should be in stdout"
    );

    // Continuation elements should NOT have blank lines between them
    let pos_c1 = stdout2
        .find("Continuation one")
        .expect("Continuation one not found");
    let pos_c2 = stdout2
        .find("Continuation two")
        .expect("Continuation two not found");
    let between_cont = &stdout2[pos_c1..pos_c2];
    assert!(
        !between_cont.contains("\n\n"),
        "Continuation elements should NOT have blank line spacing, got: {:?}",
        between_cont
    );
}
