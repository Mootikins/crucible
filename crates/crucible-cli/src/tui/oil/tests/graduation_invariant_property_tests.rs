//! Property-based invariant tests for the viewport+scrollback graduation system.
//!
//! These tests verify fundamental invariants that must hold to prevent content duplication bugs.
//!
//! # Core Invariants
//!
//! 1. **XOR Invariant**: Content appears in viewport XOR scrollback, never both
//! 2. **Content Preservation**: Total content (viewport + scrollback) equals all streamed content
//! 3. **Atomicity**: Graduation happens atomically - no intermediate state with duplication
//! 4. **Idempotence**: Rendering the same state multiple times produces identical output
//!
//! # Test Strategy
//!
//! Uses the `InkChatApp` directly with `ChatAppMsg` events to simulate real streaming scenarios,
//! then verifies invariants at each step using helper functions to extract and compare content.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::Node;
use crate::tui::oil::TestRuntime;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Render the app to a Node tree using default context.
fn view_with_default_ctx(app: &InkChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

/// Render app and return the raw string output.
fn render_app(app: &InkChatApp, width: usize) -> String {
    let tree = view_with_default_ctx(app);
    render_to_string(&tree, width)
}

/// Render app and strip ANSI codes for content comparison.
fn render_and_strip(app: &InkChatApp, width: usize) -> String {
    strip_ansi(&render_app(app, width))
}

/// Extract viewport content from TestRuntime (stripped of ANSI).
fn extract_viewport_content(runtime: &TestRuntime) -> Vec<String> {
    let viewport = strip_ansi(runtime.viewport_content());
    viewport
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// Extract scrollback/stdout content from TestRuntime (stripped of ANSI).
fn extract_scrollback_content(runtime: &TestRuntime) -> Vec<String> {
    let stdout = strip_ansi(runtime.stdout_content());
    stdout
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// Extract viewport text as a single string.
fn extract_viewport_text(runtime: &TestRuntime) -> String {
    strip_ansi(runtime.viewport_content())
}

/// Extract scrollback text as a single string.
fn extract_scrollback_text(runtime: &TestRuntime) -> String {
    strip_ansi(runtime.stdout_content())
}

/// Normalize a line for comparison (strip ANSI, trim whitespace).
fn normalize_line(line: &str) -> String {
    strip_ansi(line).trim().to_string()
}

/// Check if a line is purely decorative (borders, separators, etc.)
fn is_decorative_line(line: &str) -> bool {
    let normalized = normalize_line(line);
    if normalized.is_empty() {
        return true;
    }

    // Check if line is all border/box-drawing characters
    let decorative_chars = [
        '▄', '▀', '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '═', '║', '╔', '╗', '╚',
        '╝', '●', '○', '•', '◦',
    ];
    normalized
        .chars()
        .all(|c| decorative_chars.contains(&c) || c.is_whitespace())
}

/// Count occurrences of a needle in the combined output (stdout + viewport).
fn count_content_occurrences(runtime: &TestRuntime, needle: &str) -> usize {
    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());
    stdout.matches(needle).count() + viewport.matches(needle).count()
}

/// Get combined content from both stdout and viewport.
fn combined_content(runtime: &TestRuntime) -> String {
    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());
    format!("{}{}", stdout, viewport)
}

// ============================================================================
// TEST 1: XOR INVARIANT
// ============================================================================

/// Verify that content appears in viewport XOR scrollback, never both.
fn verify_xor_invariant(runtime: &TestRuntime, phase: &str) {
    let viewport_lines = extract_viewport_content(runtime);
    let scrollback_lines = extract_scrollback_content(runtime);

    for vp_line in &viewport_lines {
        let vp_normalized = normalize_line(vp_line);
        if vp_normalized.is_empty() || is_decorative_line(vp_line) {
            continue;
        }

        for sb_line in &scrollback_lines {
            let sb_normalized = normalize_line(sb_line);
            if sb_normalized.is_empty() || is_decorative_line(sb_line) {
                continue;
            }

            if vp_normalized.len() < 5 || sb_normalized.len() < 5 {
                continue;
            }

            assert!(
                vp_normalized != sb_normalized,
                "XOR violation at {}: Line '{}' appears in BOTH viewport and scrollback",
                phase,
                vp_normalized
            );
        }
    }
}

#[test]
fn graduation_xor_invariant_content_never_in_both() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    // Stream content in chunks
    app.on_message(ChatAppMsg::UserMessage("Test question".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after user message");

    app.on_message(ChatAppMsg::TextDelta("First chunk. ".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after first chunk");

    app.on_message(ChatAppMsg::TextDelta("Second chunk. ".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "mid-stream");

    app.on_message(ChatAppMsg::TextDelta("Third chunk.".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "before completion");

    // Trigger graduation
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after graduation");
}

#[test]
fn graduation_xor_invariant_with_multiple_paragraphs() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Multi-paragraph test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Stream multiple paragraphs (blank lines trigger graduation)
    app.on_message(ChatAppMsg::TextDelta(
        "First paragraph content.\n\n".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after first paragraph");

    app.on_message(ChatAppMsg::TextDelta(
        "Second paragraph content.\n\n".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after second paragraph");

    app.on_message(ChatAppMsg::TextDelta(
        "Third paragraph in progress".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "during third paragraph");

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    verify_xor_invariant(&runtime, "after completion");
}

// ============================================================================
// TEST 2: CONTENT PRESERVATION
// ============================================================================

#[test]
fn graduation_preserves_all_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    let chunks = vec!["First. ", "Second. ", "Third."];
    let expected_total: String = chunks.iter().copied().collect();

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    for chunk in &chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let viewport = extract_viewport_text(&runtime);
    let scrollback = extract_scrollback_text(&runtime);
    let total = format!("{}{}", scrollback, viewport);

    // The content should be preserved (may have formatting around it)
    assert!(
        total.contains("First.") && total.contains("Second.") && total.contains("Third."),
        "Content lost or corrupted. Expected all chunks present.\n\
         Expected chunks: {:?}\n\
         Scrollback: '{}'\n\
         Viewport: '{}'",
        expected_total,
        scrollback,
        viewport
    );
}

#[test]
fn graduation_preserves_content_with_code_blocks() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Show code".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Stream a code block
    app.on_message(ChatAppMsg::TextDelta("Here's some code:\n\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("```rust\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("fn main() {\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta(
        "    println!(\"hello\");\n".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("}\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("```\n\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("That's the code.".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let combined = combined_content(&runtime);

    // Verify key content is preserved
    assert!(
        combined.contains("fn main()"),
        "Code content should be preserved. Combined: '{}'",
        combined
    );
    assert!(
        combined.contains("println!"),
        "Code content should be preserved. Combined: '{}'",
        combined
    );
}

// ============================================================================
// TEST 3: ATOMICITY
// ============================================================================

#[test]
fn graduation_is_atomic_no_intermediate_duplication() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    let unique_marker = "[UNIQUE_MARKER_12345]";

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta(format!(
        "{} Content to graduate",
        unique_marker
    )));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Count content before graduation
    let before_count = count_content_occurrences(&runtime, unique_marker);
    assert_eq!(
        before_count, 1,
        "Content should appear exactly once before graduation, got {}",
        before_count
    );

    // Trigger graduation
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Count content after graduation
    let after_count = count_content_occurrences(&runtime, unique_marker);
    assert_eq!(
        after_count, 1,
        "Content should appear exactly once after graduation, got {}",
        after_count
    );
}

#[test]
fn graduation_atomicity_with_rapid_chunks() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Rapid test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Send many rapid chunks
    for i in 0..10 {
        let marker = format!("[CHUNK_{}]", i);
        app.on_message(ChatAppMsg::TextDelta(format!("{} ", marker)));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        // Verify no duplication at each step
        let count = count_content_occurrences(&runtime, &marker);
        assert_eq!(
            count, 1,
            "Chunk {} should appear exactly once during streaming, got {}",
            i, count
        );
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Verify all chunks appear exactly once after completion
    for i in 0..10 {
        let marker = format!("[CHUNK_{}]", i);
        let count = count_content_occurrences(&runtime, &marker);
        assert_eq!(
            count, 1,
            "Chunk {} should appear exactly once after graduation, got {}",
            i, count
        );
    }
}

#[test]
fn graduation_atomicity_across_multiple_renders() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    let marker = "[ATOMIC_TEST_MARKER]";

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta(format!("{} content", marker)));
    app.on_message(ChatAppMsg::StreamComplete);

    // Render multiple times
    for render_num in 1..=5 {
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let count = count_content_occurrences(&runtime, marker);
        assert_eq!(
            count, 1,
            "After render {}: marker should appear exactly once, got {}",
            render_num, count
        );
    }
}

// ============================================================================
// TEST 4: IDEMPOTENCE
// ============================================================================

#[test]
fn rendering_is_idempotent_after_graduation() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Content to render".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let render1 = render_and_strip(&app, 80);
    let render2 = render_and_strip(&app, 80);
    let render3 = render_and_strip(&app, 80);

    assert_eq!(render1, render2, "First and second render differ");
    assert_eq!(render2, render3, "Second and third render differ");
}

#[test]
fn rendering_is_idempotent_during_streaming() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Streaming content".to_string()));

    // Don't complete - still streaming
    let render1 = render_and_strip(&app, 80);
    let render2 = render_and_strip(&app, 80);
    let render3 = render_and_strip(&app, 80);

    assert_eq!(
        render1, render2,
        "First and second render differ during streaming"
    );
    assert_eq!(
        render2, render3,
        "Second and third render differ during streaming"
    );
}

#[test]
fn rendering_is_idempotent_with_tool_calls() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Run a tool".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "test_tool".to_string(),
        args: r#"{"arg": "value"}"#.to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "test_tool".to_string(),
        delta: "Tool output line 1\n".to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "test_tool".to_string(),
        delta: "Tool output line 2\n".to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "test_tool".to_string(),
    });
    app.on_message(ChatAppMsg::TextDelta("After tool".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let render1 = render_and_strip(&app, 80);
    let render2 = render_and_strip(&app, 80);
    let render3 = render_and_strip(&app, 80);

    assert_eq!(
        render1, render2,
        "First and second render differ with tool calls"
    );
    assert_eq!(
        render2, render3,
        "Second and third render differ with tool calls"
    );
}

// ============================================================================
// ADDITIONAL INVARIANT TESTS
// ============================================================================

#[test]
fn graduation_monotonic_count_never_decreases() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();
    let mut prev_count = 0;

    // First message
    app.on_message(ChatAppMsg::UserMessage("First".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 1".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let count1 = runtime.graduated_count();
    assert!(
        count1 >= prev_count,
        "Graduated count should not decrease: {} -> {}",
        prev_count,
        count1
    );
    prev_count = count1;

    // Second message
    app.on_message(ChatAppMsg::UserMessage("Second".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 2".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let count2 = runtime.graduated_count();
    assert!(
        count2 >= prev_count,
        "Graduated count should not decrease: {} -> {}",
        prev_count,
        count2
    );
    prev_count = count2;

    // Third message
    app.on_message(ChatAppMsg::UserMessage("Third".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 3".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let count3 = runtime.graduated_count();
    assert!(
        count3 >= prev_count,
        "Graduated count should not decrease: {} -> {}",
        prev_count,
        count3
    );
}

#[test]
fn graduation_stable_across_resize() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Resize test".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Content before resize".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let marker = "Content before resize";
    let count_before = count_content_occurrences(&runtime, marker);
    assert_eq!(count_before, 1, "Content should appear once before resize");

    // Resize terminal
    runtime.resize(60, 20);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let count_after = count_content_occurrences(&runtime, marker);
    assert_eq!(
        count_after, 1,
        "Content should still appear exactly once after resize, got {}",
        count_after
    );

    // Resize again
    runtime.resize(100, 30);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let count_final = count_content_occurrences(&runtime, marker);
    assert_eq!(
        count_final, 1,
        "Content should still appear exactly once after second resize, got {}",
        count_final
    );
}

#[test]
fn graduation_xor_with_cancelled_stream() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Cancel test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta(
        "Partial content before cancel".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    verify_xor_invariant(&runtime, "before cancel");

    // Cancel instead of complete
    app.on_message(ChatAppMsg::StreamCancelled);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    verify_xor_invariant(&runtime, "after cancel");

    // Verify content is preserved (either in viewport or scrollback)
    let combined = combined_content(&runtime);
    assert!(
        combined.contains("Partial content"),
        "Cancelled content should be preserved somewhere. Combined: '{}'",
        combined
    );
}

#[test]
fn graduation_handles_empty_messages_correctly() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Empty test".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Send empty delta
    app.on_message(ChatAppMsg::TextDelta("".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    verify_xor_invariant(&runtime, "after empty delta");

    // Send actual content
    app.on_message(ChatAppMsg::TextDelta("Real content".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    verify_xor_invariant(&runtime, "after real content");

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    verify_xor_invariant(&runtime, "after completion");

    let combined = combined_content(&runtime);
    assert!(
        combined.contains("Real content"),
        "Real content should be preserved"
    );
}
