//! Property-based invariant tests for the viewport+scrollback graduation system.
//!
//! All rendering goes through the real terminal path (Terminal<Vec<u8>> → vt100).
//!
//! # Core Invariants
//!
//! 1. **No Duplication**: Content appears exactly once in the rendered output
//! 2. **Content Preservation**: All streamed content is visible after graduation
//! 3. **Atomicity**: No intermediate state with duplication during graduation
//! 4. **Idempotence**: Rendering the same state multiple times produces identical output

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};

use super::vt100_runtime::Vt100TestRuntime;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Render through vt100 and return stripped screen contents.
fn full_output(vt: &mut Vt100TestRuntime, app: &mut OilChatApp) -> String {
    vt.render_frame(app);
    strip_ansi(&vt.screen_contents())
}

/// Count occurrences of a needle in the screen output.
fn count_occurrences(vt: &mut Vt100TestRuntime, app: &mut OilChatApp, needle: &str) -> usize {
    let output = full_output(vt, app);
    output.matches(needle).count()
}

/// Verify no content line appears duplicated in the output.
fn verify_no_duplication(vt: &mut Vt100TestRuntime, app: &mut OilChatApp, phase: &str) {
    let output = full_output(vt, app);
    let lines: Vec<&str> = output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.chars().all(|c| "▄▀─│┌┐└┘├┤┬┴┼═║╔╗╚╝●○•◦ >".contains(c)))
        .filter(|l| l.len() >= 5)
        .collect();

    for (i, line) in lines.iter().enumerate() {
        for (j, other) in lines.iter().enumerate() {
            if i != j && line == other {
                panic!(
                    "Duplication at {}: line '{}' appears at positions {} and {}.\nFull output:\n{}",
                    phase, line, i, j, output
                );
            }
        }
    }
}

// ============================================================================
// TEST 1: XOR INVARIANT
// ============================================================================

#[test]
fn graduation_xor_invariant_content_never_in_both() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    // Stream content in chunks
    app.on_message(ChatAppMsg::UserMessage("Test question".to_string()));
    verify_no_duplication(&mut vt, &mut app, "after user message");

    app.on_message(ChatAppMsg::TextDelta("First chunk. ".to_string()));
    verify_no_duplication(&mut vt, &mut app, "after first chunk");

    app.on_message(ChatAppMsg::TextDelta("Second chunk. ".to_string()));
    verify_no_duplication(&mut vt, &mut app, "mid-stream");

    app.on_message(ChatAppMsg::TextDelta("Third chunk.".to_string()));
    verify_no_duplication(&mut vt, &mut app, "before completion");

    // Trigger graduation
    app.on_message(ChatAppMsg::StreamComplete);
    verify_no_duplication(&mut vt, &mut app, "after graduation");
}

#[test]
fn graduation_xor_invariant_with_multiple_paragraphs() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Multi-paragraph test".to_string()));
    vt.render_frame(&mut app);

    // Stream multiple paragraphs (blank lines trigger graduation)
    app.on_message(ChatAppMsg::TextDelta(
        "First paragraph content.\n\n".to_string(),
    ));
    verify_no_duplication(&mut vt, &mut app, "after first paragraph");

    app.on_message(ChatAppMsg::TextDelta(
        "Second paragraph content.\n\n".to_string(),
    ));
    verify_no_duplication(&mut vt, &mut app, "after second paragraph");

    app.on_message(ChatAppMsg::TextDelta(
        "Third paragraph in progress".to_string(),
    ));
    verify_no_duplication(&mut vt, &mut app, "during third paragraph");

    app.on_message(ChatAppMsg::StreamComplete);
    verify_no_duplication(&mut vt, &mut app, "after completion");
}

// ============================================================================
// TEST 2: CONTENT PRESERVATION
// ============================================================================

#[test]
fn graduation_preserves_all_content() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    let chunks = vec!["First. ", "Second. ", "Third."];

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    vt.render_frame(&mut app);

    for chunk in &chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));
        vt.render_frame(&mut app);
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let total = full_output(&mut vt, &mut app);

    // The content should be preserved (may have formatting around it)
    assert!(
        total.contains("First.") && total.contains("Second.") && total.contains("Third."),
        "Content lost or corrupted. Expected all chunks present.\nOutput: '{}'",
        total
    );
}

#[test]
fn graduation_preserves_content_with_code_blocks() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Show code".to_string()));
    vt.render_frame(&mut app);

    // Stream a code block
    app.on_message(ChatAppMsg::TextDelta("Here's some code:\n\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```rust\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("fn main() {\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta(
        "    println!(\"hello\");\n".to_string(),
    ));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("}\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```\n\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("That's the code.".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);

    let combined = full_output(&mut vt, &mut app);

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
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    let unique_marker = "[UNIQUE_MARKER_12345]";

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta(format!(
        "{} Content to graduate",
        unique_marker
    )));

    // Count content before graduation
    let before_count = count_occurrences(&mut vt, &mut app, unique_marker);
    assert_eq!(
        before_count, 1,
        "Content should appear exactly once before graduation, got {}",
        before_count
    );

    // Trigger graduation
    app.on_message(ChatAppMsg::StreamComplete);

    // Count content after graduation
    let after_count = count_occurrences(&mut vt, &mut app, unique_marker);
    assert_eq!(
        after_count, 1,
        "Content should appear exactly once after graduation, got {}",
        after_count
    );
}

#[test]
fn graduation_atomicity_with_rapid_chunks() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Rapid test".to_string()));
    vt.render_frame(&mut app);

    // Send many rapid chunks
    for i in 0..10 {
        let marker = format!("[CHUNK_{}]", i);
        app.on_message(ChatAppMsg::TextDelta(format!("{} ", marker)));

        // Verify no duplication at each step
        let count = count_occurrences(&mut vt, &mut app, &marker);
        assert_eq!(
            count, 1,
            "Chunk {} should appear exactly once during streaming, got {}",
            i, count
        );
    }

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Verify all chunks appear exactly once after completion
    let output = full_output(&mut vt, &mut app);
    for i in 0..10 {
        let marker = format!("[CHUNK_{}]", i);
        let count = output.matches(&marker).count();
        assert_eq!(
            count, 1,
            "Chunk {} should appear exactly once after graduation, got {}",
            i, count
        );
    }
}

#[test]
fn graduation_atomicity_across_multiple_renders() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    let marker = "[ATOMIC_TEST_MARKER]";

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta(format!("{} content", marker)));
    app.on_message(ChatAppMsg::StreamComplete);

    // Render multiple times
    for render_num in 1..=5 {
        let count = count_occurrences(&mut vt, &mut app, marker);
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
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Content to render".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    vt.render_frame(&mut app);
    let render1 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render2 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render3 = strip_ansi(&vt.screen_contents());

    assert_eq!(render1, render2, "First and second render differ");
    assert_eq!(render2, render3, "Second and third render differ");
}

#[test]
fn rendering_is_idempotent_during_streaming() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Streaming content".to_string()));

    // Don't complete - still streaming
    vt.render_frame(&mut app);
    let render1 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render2 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render3 = strip_ansi(&vt.screen_contents());

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
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Run a tool".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "test_tool".to_string(),
        args: r#"{"arg": "value"}"#.to_string(),
        call_id: None,
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "test_tool".to_string(),
        delta: "Tool output line 1\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "test_tool".to_string(),
        delta: "Tool output line 2\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "test_tool".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::TextDelta("After tool".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    vt.render_frame(&mut app);
    let render1 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render2 = strip_ansi(&vt.screen_contents());

    vt.render_frame(&mut app);
    let render3 = strip_ansi(&vt.screen_contents());

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
fn graduation_content_accumulates_across_messages() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    // First message
    app.on_message(ChatAppMsg::UserMessage("First".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 1".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let combined1 = full_output(&mut vt, &mut app);
    assert!(
        combined1.contains("First"),
        "First message should be present"
    );

    // Second message
    app.on_message(ChatAppMsg::UserMessage("Second".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 2".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let combined2 = full_output(&mut vt, &mut app);
    assert!(
        combined2.contains("First") && combined2.contains("Second"),
        "Both messages should be present"
    );

    // Third message
    app.on_message(ChatAppMsg::UserMessage("Third".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response 3".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let combined3 = full_output(&mut vt, &mut app);
    assert!(
        combined3.contains("First") && combined3.contains("Second") && combined3.contains("Third"),
        "All messages should be present"
    );
}

#[test]
fn graduation_stable_across_resize() {
    let marker = "Content before resize";

    // Helper: build a fresh app with the same message sequence
    let make_app = || {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Resize test".to_string()));
        app.on_message(ChatAppMsg::TextDelta(marker.to_string()));
        app.on_message(ChatAppMsg::StreamComplete);
        app
    };

    // Render at original size (80 wide)
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = make_app();
    let count_before = count_occurrences(&mut vt, &mut app, marker);
    assert_eq!(count_before, 1, "Content should appear once at width 80");

    // Render at smaller width (60 wide)
    let mut vt = Vt100TestRuntime::new(60, 100);
    let mut app = make_app();
    let count_after = count_occurrences(&mut vt, &mut app, marker);
    assert_eq!(
        count_after, 1,
        "Content should appear exactly once at width 60, got {}",
        count_after
    );

    // Render at larger width (100 wide)
    let mut vt = Vt100TestRuntime::new(100, 100);
    let mut app = make_app();
    let count_final = count_occurrences(&mut vt, &mut app, marker);
    assert_eq!(
        count_final, 1,
        "Content should appear exactly once at width 100, got {}",
        count_final
    );
}

#[test]
fn graduation_xor_with_cancelled_stream() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Cancel test".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta(
        "Partial content before cancel".to_string(),
    ));
    verify_no_duplication(&mut vt, &mut app, "before cancel");

    // Cancel instead of complete
    app.on_message(ChatAppMsg::StreamCancelled);
    verify_no_duplication(&mut vt, &mut app, "after cancel");

    // Verify content is preserved
    let combined = full_output(&mut vt, &mut app);
    assert!(
        combined.contains("Partial content"),
        "Cancelled content should be preserved somewhere. Combined: '{}'",
        combined
    );
}

#[test]
fn graduation_handles_empty_messages_correctly() {
    let mut vt = Vt100TestRuntime::new(80, 100);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Empty test".to_string()));
    vt.render_frame(&mut app);

    // Send empty delta
    app.on_message(ChatAppMsg::TextDelta("".to_string()));
    verify_no_duplication(&mut vt, &mut app, "after empty delta");

    // Send actual content
    app.on_message(ChatAppMsg::TextDelta("Real content".to_string()));
    verify_no_duplication(&mut vt, &mut app, "after real content");

    app.on_message(ChatAppMsg::StreamComplete);
    verify_no_duplication(&mut vt, &mut app, "after completion");

    let combined = full_output(&mut vt, &mut app);
    assert!(
        combined.contains("Real content"),
        "Real content should be preserved"
    );
}

// ============================================================================
// PROPERTY-BASED TESTS (proptest)
// ============================================================================

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        max_shrink_iters: 1000,
        .. ProptestConfig::default()
    })]

    /// Property 1: XOR invariant holds for arbitrary random chunk sequences.
    ///
    /// Generates random strings as chunks and verifies that after streaming
    /// and graduation, content never appears in both viewport and scrollback.
    #[test]
    fn prop_xor_invariant_holds_for_random_chunks(
        chunks in prop::collection::vec("[a-zA-Z0-9 ]{1,50}", 1..20)
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Property test".to_string()));
        vt.render_frame(&mut app);

        for chunk in &chunks {
            app.on_message(ChatAppMsg::TextDelta(chunk.clone()));
            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        verify_no_duplication(&mut vt, &mut app, "random chunks");
    }

    /// Property 2: Content is preserved for arbitrary random chunks.
    ///
    /// Generates random alphanumeric chunks and verifies that all content
    /// appears in the combined output after graduation.
    #[test]
    fn prop_content_preserved_for_random_chunks(
        chunks in prop::collection::vec("[a-zA-Z0-9]{5,20}", 1..15)
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Content preservation test".to_string()));
        vt.render_frame(&mut app);

        for chunk in &chunks {
            app.on_message(ChatAppMsg::TextDelta(format!("{} ", chunk)));
            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        let combined = full_output(&mut vt, &mut app);

        // Verify each chunk appears in the combined output
        for chunk in &chunks {
            prop_assert!(
                combined.contains(chunk.as_str()),
                "Content lost: chunk '{}' not found in combined output.\nCombined: '{}'",
                chunk,
                combined
            );
        }
    }

    /// Property 3: Atomicity holds for arbitrary chunk counts.
    ///
    /// Generates a random number of identical chunks and verifies that
    /// the count of content occurrences remains stable through graduation.
    #[test]
    fn prop_atomicity_holds_for_random_chunk_count(
        chunk_count in 1usize..30,
        chunk_content in "[A-Z]{10,15}"
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        // Use a unique marker to track this specific content
        let marker = format!("[MARKER_{}]", chunk_content);

        app.on_message(ChatAppMsg::UserMessage("Atomicity test".to_string()));
        vt.render_frame(&mut app);

        // Send the marker once, then send chunk_count chunks
        app.on_message(ChatAppMsg::TextDelta(marker.clone()));
        vt.render_frame(&mut app);

        for _ in 0..chunk_count {
            app.on_message(ChatAppMsg::TextDelta(" chunk".to_string()));
            vt.render_frame(&mut app);
        }

        let before_count = count_occurrences(&mut vt, &mut app, &marker);

        app.on_message(ChatAppMsg::StreamComplete);

        let after_count = count_occurrences(&mut vt, &mut app, &marker);

        prop_assert_eq!(
            before_count, after_count,
            "Content count changed during graduation (duplication or loss). Before: {}, After: {}",
            before_count, after_count
        );

        // Also verify the marker appears exactly once
        prop_assert_eq!(
            after_count, 1,
            "Marker should appear exactly once, got {}",
            after_count
        );
    }

    /// Property 4: Idempotence holds for arbitrary content.
    ///
    /// Generates random content and verifies that rendering the same
    /// state multiple times produces identical output.
    #[test]
    fn prop_rendering_idempotent_for_random_content(
        content in "[a-zA-Z0-9 .,!?]{10,100}"
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Idempotence test".to_string()));
        app.on_message(ChatAppMsg::TextDelta(content));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);
        let render1 = strip_ansi(&vt.screen_contents());

        vt.render_frame(&mut app);
        let render2 = strip_ansi(&vt.screen_contents());

        vt.render_frame(&mut app);
        let render3 = strip_ansi(&vt.screen_contents());

        prop_assert_eq!(&render1, &render2, "First and second render differ");
        prop_assert_eq!(&render2, &render3, "Second and third render differ");
    }

    /// Property 5: XOR invariant holds with paragraph breaks.
    ///
    /// Generates chunks with embedded newlines (paragraph breaks) and
    /// verifies the XOR invariant still holds.
    #[test]
    fn prop_xor_invariant_with_paragraph_breaks(
        chunks in prop::collection::vec("[a-zA-Z0-9 ]{5,30}", 1..10)
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Paragraph test".to_string()));
        vt.render_frame(&mut app);

        for (i, chunk) in chunks.iter().enumerate() {
            // Add paragraph break every 3rd chunk
            let content = if i % 3 == 2 {
                format!("{}\n\n", chunk)
            } else {
                format!("{} ", chunk)
            };
            app.on_message(ChatAppMsg::TextDelta(content));
            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        verify_no_duplication(&mut vt, &mut app, "paragraph breaks");
    }

    /// Property 6: Content accumulates for arbitrary message sequences.
    ///
    /// Generates multiple message/response pairs and verifies that all
    /// messages appear in the combined output.
    #[test]
    fn prop_content_accumulates_across_messages(
        message_count in 1usize..5,
        response_lengths in prop::collection::vec(1usize..10, 1..5)
    ) {
        let mut vt = Vt100TestRuntime::new(80, 100);
        let mut app = OilChatApp::default();

        for (msg_idx, &resp_len) in response_lengths.iter().take(message_count).enumerate() {
            app.on_message(ChatAppMsg::UserMessage(format!("Message {}", msg_idx)));

            for chunk_idx in 0..resp_len {
                app.on_message(ChatAppMsg::TextDelta(format!("Chunk {} ", chunk_idx)));
            }

            app.on_message(ChatAppMsg::StreamComplete);
            vt.render_frame(&mut app);
        }

        let combined = full_output(&mut vt, &mut app);
        // Verify all messages are present
        for msg_idx in 0..message_count.min(response_lengths.len()) {
            let marker = format!("Message {}", msg_idx);
            prop_assert!(
                combined.contains(&marker),
                "Message '{}' not found in combined output:\n{}",
                marker, combined
            );
        }
    }
}
