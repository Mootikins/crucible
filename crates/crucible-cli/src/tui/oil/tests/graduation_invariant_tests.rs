//! Property-based tests for graduation invariants.
//!
//! # Invariants Tested
//!
//! ## P0: No Duplication
//! - Content appears in stdout XOR viewport, never both
//! - After graduation, content only in stdout
//! - Before graduation (streaming), content only in viewport
//!
//! ## P0: Viewport Stability (Anti-Shaking)
//! - Height reduction doesn't cause content loss
//! - Content remains visible during graduation transition
//!
//! # Test Strategy
//!
//! Uses unique markers in content to track specific strings through the graduation
//! pipeline. Markers are UUIDs or unique prefixes that won't appear in other content.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::node::{
    col, overlay_from_bottom, scrollback, scrollback_continuation, text, Node,
};
use crate::tui::oil::runtime::TestRuntime;
use proptest::prelude::*;

// ============================================================================
// GENERATORS
// ============================================================================

/// Generate a unique marker string that can be tracked through stdout/viewport.
/// Format: `[MKR-{id}-{suffix}]` where id is 0-999 and suffix is random alpha.
fn arb_marker(id: usize) -> impl Strategy<Value = String> {
    prop::string::string_regex("[A-Z]{4}")
        .unwrap()
        .prop_map(move |suffix| format!("[MKR-{:03}-{}]", id, suffix))
}

/// Generate a message with embedded unique marker for tracking.
/// Returns (marker, full_message_content).
fn arb_message_with_marker(id: usize) -> impl Strategy<Value = (String, String)> {
    (
        arb_marker(id),
        prop::string::string_regex("[a-zA-Z ]{5,30}").unwrap(),
    )
        .prop_map(|(marker, text)| {
            let content = format!("{} {}", marker, text);
            (marker, content)
        })
}

/// Generate terminal dimensions.
fn arb_terminal_size() -> impl Strategy<Value = (u16, u16)> {
    (40u16..120, 10u16..40)
}

/// A message in a conversation with its tracking marker.
#[derive(Debug, Clone)]
pub struct TrackedMessage {
    pub key: String,
    pub marker: String,
    pub content: String,
    pub is_continuation: bool,
}

/// Generate a sequence of tracked messages for a conversation.
fn arb_message_sequence(
    count: std::ops::Range<usize>,
) -> impl Strategy<Value = Vec<TrackedMessage>> {
    count.prop_flat_map(|n| {
        (0..n)
            .map(|i| {
                (
                    arb_message_with_marker(i),
                    prop::bool::weighted(0.2), // 20% chance of continuation
                )
                    .prop_map(move |((marker, content), is_cont)| TrackedMessage {
                        key: format!("msg-{}", i),
                        marker,
                        content,
                        is_continuation: is_cont && i > 0,
                    })
            })
            .collect::<Vec<_>>()
    })
}

/// Build a Node tree from tracked messages.
fn build_message_tree(messages: &[TrackedMessage], include_viewport_content: bool) -> Node {
    let mut nodes: Vec<Node> = messages
        .iter()
        .map(|msg| {
            if msg.is_continuation {
                scrollback_continuation(&msg.key, [text(&msg.content)])
            } else {
                scrollback(&msg.key, [text(&msg.content)])
            }
        })
        .collect();

    if include_viewport_content {
        nodes.push(text("VIEWPORT_ANCHOR"));
    }

    col(nodes)
}

/// Count occurrences of a marker in a string.
fn count_marker(haystack: &str, marker: &str) -> usize {
    haystack.matches(marker).count()
}

// ============================================================================
// PROPERTY TESTS - PHASE 1: DUPLICATION PREVENTION
// ============================================================================

proptest! {
    // 50 cases balances coverage vs memory/time for CI
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property: Each unique marker appears exactly once in (stdout + viewport) combined.
    ///
    /// This is the fundamental no-duplication invariant. A marker should never appear
    /// in both stdout (graduated) and viewport (live) simultaneously.
    #[test]
    fn content_appears_exactly_once(
        messages in arb_message_sequence(1..8),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);
        let tree = build_message_tree(&messages, true);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        for msg in &messages {
            let stdout_count = count_marker(&stdout, &msg.marker);
            let viewport_count = count_marker(&viewport, &msg.marker);
            let total = stdout_count + viewport_count;

            prop_assert_eq!(
                total, 1,
                "Marker {} should appear exactly once total.\n\
                 stdout_count={}, viewport_count={}\n\
                 stdout (stripped):\n{}\n\
                 viewport (stripped):\n{}",
                msg.marker, stdout_count, viewport_count, stdout, viewport
            );
        }
    }

    /// Property: After graduation (scrollback nodes in a complete tree),
    /// graduated content appears only in stdout, not in viewport.
    ///
    /// When the tree has viewport content (input or streaming text), scrollback
    /// nodes should graduate to stdout and be filtered from viewport.
    #[test]
    fn graduated_content_only_in_stdout(
        messages in arb_message_sequence(1..6),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);

        // Tree with viewport content triggers graduation
        let tree = build_message_tree(&messages, true);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        for msg in &messages {
            let in_stdout = stdout.contains(&msg.marker);
            let in_viewport = viewport.contains(&msg.marker);

            prop_assert!(
                in_stdout && !in_viewport,
                "After graduation, marker {} should be in stdout only.\n\
                 in_stdout={}, in_viewport={}\n\
                 stdout:\n{}\n\
                 viewport:\n{}",
                msg.marker, in_stdout, in_viewport, stdout, viewport
            );
        }
    }

    /// Property: Non-scrollback content (streaming) appears only in viewport.
    ///
    /// Content that isn't wrapped in scrollback nodes should never appear in
    /// stdout since it hasn't been marked as complete.
    #[test]
    fn streaming_content_only_in_viewport(
        graduated_msgs in arb_message_sequence(1..4),
        (streaming_marker, streaming_content) in arb_message_with_marker(100),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);

        // Build tree with graduated messages and streaming (non-scrollback) content
        let mut nodes: Vec<Node> = graduated_msgs
            .iter()
            .map(|msg| scrollback(&msg.key, [text(&msg.content)]))
            .collect();
        nodes.push(text(&streaming_content)); // Streaming, not wrapped in scrollback

        let tree = col(nodes);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        // Streaming content should only be in viewport
        let in_stdout = stdout.contains(&streaming_marker);
        let in_viewport = viewport.contains(&streaming_marker);

        prop_assert!(
            !in_stdout && in_viewport,
            "Streaming marker {} should be in viewport only.\n\
             in_stdout={}, in_viewport={}\n\
             stdout:\n{}\n\
             viewport:\n{}",
            streaming_marker, in_stdout, in_viewport, stdout, viewport
        );

        // Graduated content should only be in stdout
        for msg in &graduated_msgs {
            let grad_in_stdout = stdout.contains(&msg.marker);
            let grad_in_viewport = viewport.contains(&msg.marker);

            prop_assert!(
                grad_in_stdout && !grad_in_viewport,
                "Graduated marker {} should be in stdout only.\n\
                 in_stdout={}, in_viewport={}",
                msg.marker, grad_in_stdout, grad_in_viewport
            );
        }
    }

    /// Property: Empty messages don't create duplication artifacts.
    ///
    /// Empty scrollback nodes should be skipped entirely - they shouldn't
    /// appear in stdout or viewport, and shouldn't affect graduation of
    /// other nodes.
    #[test]
    fn empty_messages_handled_correctly(
        real_msgs in arb_message_sequence(1..4),
        empty_positions in prop::collection::vec(0usize..5, 0..3),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);

        // Insert empty scrollback nodes at various positions
        let mut nodes: Vec<Node> = Vec::new();
        let mut msg_iter = real_msgs.iter();

        for i in 0..6 {
            if empty_positions.contains(&i) {
                nodes.push(scrollback(format!("empty-{}", i), [Node::Empty]));
            }
            if let Some(msg) = msg_iter.next() {
                nodes.push(scrollback(&msg.key, [text(&msg.content)]));
            }
        }
        nodes.push(text("ANCHOR"));

        let tree = col(nodes);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        // Real messages should appear exactly once
        for msg in &real_msgs {
            let total = count_marker(&stdout, &msg.marker)
                + count_marker(&viewport, &msg.marker);
            prop_assert_eq!(
                total, 1,
                "Real marker {} should appear exactly once, got {}",
                msg.marker, total
            );
        }
    }

    /// Property: Continuation nodes append correctly without duplication.
    ///
    /// A scrollback_continuation should append to the previous message
    /// without creating duplicate content.
    #[test]
    fn continuations_no_duplication(
        base_msg in arb_message_with_marker(0),
        cont_msg in arb_message_with_marker(1),
        (width, height) in arb_terminal_size()
    ) {
        let (base_marker, base_content) = base_msg;
        let (cont_marker, cont_content) = cont_msg;

        let mut runtime = TestRuntime::new(width, height);

        let tree = col([
            scrollback("msg-base", [text(&base_content)]),
            scrollback_continuation("msg-cont", [text(&cont_content)]),
            text("ANCHOR"),
        ]);

        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        prop_assert_eq!(
            count_marker(&combined, &base_marker), 1,
            "Base marker should appear once"
        );
        prop_assert_eq!(
            count_marker(&combined, &cont_marker), 1,
            "Continuation marker should appear once"
        );

        // Both should be in stdout (graduated), neither in viewport
        prop_assert!(stdout.contains(&base_marker));
        prop_assert!(stdout.contains(&cont_marker));
        prop_assert!(!viewport.contains(&base_marker));
        prop_assert!(!viewport.contains(&cont_marker));
    }

    /// Property: Multi-render stability - content doesn't duplicate across renders.
    ///
    /// Rendering the same tree multiple times should not cause content to
    /// appear more than once in stdout (no re-graduation).
    #[test]
    fn multi_render_no_duplication(
        messages in arb_message_sequence(2..6),
        render_count in 2usize..5,
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);
        let tree = build_message_tree(&messages, true);

        for _ in 0..render_count {
            runtime.render(&tree);
        }

        let stdout = strip_ansi(runtime.stdout_content());

        for msg in &messages {
            let count = count_marker(&stdout, &msg.marker);
            prop_assert_eq!(
                count, 1,
                "After {} renders, marker {} should appear once in stdout, got {}",
                render_count, msg.marker, count
            );
        }
    }

    /// Property: Incremental graduation maintains no-duplication.
    ///
    /// Adding new messages to the tree across renders should not cause
    /// previously graduated content to appear again.
    #[test]
    fn incremental_graduation_no_duplication(
        all_messages in arb_message_sequence(3..7),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);

        // Render incrementally, adding one message at a time
        for i in 1..=all_messages.len() {
            let tree = build_message_tree(&all_messages[..i], true);
            runtime.render(&tree);
        }

        let stdout = strip_ansi(runtime.stdout_content());

        for msg in &all_messages {
            let count = count_marker(&stdout, &msg.marker);
            prop_assert_eq!(
                count, 1,
                "After incremental renders, marker {} should appear once, got {}",
                msg.marker, count
            );
        }
    }
}

// ============================================================================
// PHASE 2: VIEWPORT STABILITY (ANTI-SHAKING)
// ============================================================================

fn arb_height_sequence() -> impl Strategy<Value = Vec<u16>> {
    prop::collection::vec(12u16..35, 2..5)
}

fn arb_height_reduction() -> impl Strategy<Value = (u16, u16)> {
    (15u16..40).prop_flat_map(|h1| (Just(h1), 10u16..h1))
}

fn combined_content(runtime: &TestRuntime) -> String {
    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());
    format!("{}{}", stdout, viewport)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn height_change_during_streaming_no_duplication(
        graduated_msgs in arb_message_sequence(1..3),
        (streaming_marker, streaming_content) in arb_message_with_marker(50),
        heights in arb_height_sequence(),
        width in 60u16..100
    ) {
        let initial_height = heights.first().copied().unwrap_or(25);
        let mut runtime = TestRuntime::new(width, initial_height);

        let mut nodes: Vec<Node> = graduated_msgs
            .iter()
            .map(|msg| scrollback(&msg.key, [text(&msg.content)]))
            .collect();
        nodes.push(text(&streaming_content));

        let tree = col(nodes);

        for &h in &heights {
            runtime.resize(width, h);
            runtime.render(&tree);

            let stdout = strip_ansi(runtime.stdout_content());
            let viewport = strip_ansi(runtime.viewport_content());

            for msg in &graduated_msgs {
                let in_stdout = stdout.contains(&msg.marker);
                let in_viewport = viewport.contains(&msg.marker);
                prop_assert!(
                    in_stdout ^ in_viewport,
                    "At h={}: graduated marker {} in exactly one place",
                    h, msg.marker
                );
            }

            let stream_in_stdout = stdout.contains(&streaming_marker);
            let stream_in_viewport = viewport.contains(&streaming_marker);
            prop_assert!(
                !stream_in_stdout && stream_in_viewport,
                "At h={}: streaming marker {} must be in viewport only.\n\
                 stdout={}, viewport={}",
                h, streaming_marker, stream_in_stdout, stream_in_viewport
            );
        }
    }

    #[test]
    fn rapid_message_arrival_with_resize(
        msg_count in 3usize..6,
        resize_points in prop::collection::vec(0usize..6, 1..3),
        (h1, h2) in arb_height_reduction(),
        width in 60u16..100
    ) {
        let mut runtime = TestRuntime::new(width, h1);
        let mut markers = Vec::new();
        let mut current_height = h1;

        for i in 0..msg_count {
            if resize_points.contains(&i) {
                current_height = if current_height == h1 { h2 } else { h1 };
                runtime.resize(width, current_height);
            }

            let marker = format!("[RMA-{:03}]", i);
            markers.push(marker);

            let nodes: Vec<Node> = markers
                .iter()
                .enumerate()
                .map(|(j, m)| {
                    let c = format!("{} message content", m);
                    scrollback(format!("msg-{}", j), [text(&c)])
                })
                .collect();
            let mut tree_nodes = nodes;
            tree_nodes.push(text("ANCHOR"));

            let tree = col(tree_nodes);
            runtime.render(&tree);
        }

        let final_content = combined_content(&runtime);
        for marker in &markers {
            let count = count_marker(&final_content, marker);
            prop_assert_eq!(
                count, 1,
                "After rapid arrivals+resizes: {} should appear once, got {}",
                marker, count
            );
        }
    }

    #[test]
    fn height_reduction_preserves_content(
        messages in arb_message_sequence(2..5),
        (h1, h2) in arb_height_reduction(),
        width in 60u16..100
    ) {
        let mut runtime = TestRuntime::new(width, h1);
        let tree = build_message_tree(&messages, true);

        runtime.render(&tree);
        let before_resize = combined_content(&runtime);

        runtime.resize(width, h2);
        runtime.render(&tree);
        let after_resize = combined_content(&runtime);

        for msg in &messages {
            let before_count = count_marker(&before_resize, &msg.marker);
            let after_count = count_marker(&after_resize, &msg.marker);

            prop_assert_eq!(
                before_count, 1,
                "Before resize: marker {} should appear once, got {}",
                msg.marker, before_count
            );
            prop_assert_eq!(
                after_count, 1,
                "After resize to h={}: marker {} should appear once, got {}.\n\
                 combined:\n{}",
                h2, msg.marker, after_count, after_resize
            );
        }
    }

    #[test]
    fn graduation_atomic_on_height_change(
        messages in arb_message_sequence(2..4),
        heights in arb_height_sequence(),
        width in 60u16..100
    ) {
        let initial_height = heights.first().copied().unwrap_or(20);
        let mut runtime = TestRuntime::new(width, initial_height);
        let tree = build_message_tree(&messages, true);

        for &h in &heights {
            runtime.resize(width, h);
            runtime.render(&tree);

            let stdout = strip_ansi(runtime.stdout_content());
            let viewport = strip_ansi(runtime.viewport_content());

            for msg in &messages {
                let in_stdout = stdout.contains(&msg.marker);
                let in_viewport = viewport.contains(&msg.marker);

                prop_assert!(
                    in_stdout ^ in_viewport,
                    "At height {}: marker {} must be in exactly one location.\n\
                     in_stdout={}, in_viewport={}",
                    h, msg.marker, in_stdout, in_viewport
                );
            }
        }
    }

    #[test]
    fn rapid_resize_no_corruption(
        messages in arb_message_sequence(2..5),
        heights in arb_height_sequence(),
        width in 60u16..100
    ) {
        let initial_height = heights.first().copied().unwrap_or(20);
        let mut runtime = TestRuntime::new(width, initial_height);
        let tree = build_message_tree(&messages, true);

        runtime.render(&tree);
        let initial_graduated = runtime.graduated_count();

        for &h in &heights {
            runtime.resize(width, h);
            runtime.render(&tree);

            prop_assert!(
                runtime.graduated_count() >= initial_graduated,
                "Graduated count should never decrease: {} -> {}",
                initial_graduated, runtime.graduated_count()
            );
        }

        let final_content = combined_content(&runtime);
        for msg in &messages {
            let count = count_marker(&final_content, &msg.marker);
            prop_assert_eq!(
                count, 1,
                "After rapid resizes: marker {} should appear exactly once, got {}",
                msg.marker, count
            );
        }
    }

    #[test]
    fn resize_during_incremental_graduation(
        all_messages in arb_message_sequence(3..6),
        resize_at in 1usize..3,
        (h1, h2) in arb_height_reduction(),
        width in 60u16..100
    ) {
        let mut runtime = TestRuntime::new(width, h1);
        let resize_point = resize_at.min(all_messages.len() - 1);

        for (i, _) in all_messages.iter().enumerate() {
            if i == resize_point {
                runtime.resize(width, h2);
            }

            let tree = build_message_tree(&all_messages[..=i], true);
            runtime.render(&tree);
        }

        let final_content = combined_content(&runtime);
        for msg in &all_messages {
            let count = count_marker(&final_content, &msg.marker);
            prop_assert_eq!(
                count, 1,
                "After resize mid-graduation: marker {} should appear once, got {}",
                msg.marker, count
            );
        }
    }
}

// ============================================================================
// PHASE 3: INTEGRATION AND EDGE CASES
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn overlay_with_resize_no_duplication(
        messages in arb_message_sequence(2..4),
        (h1, h2) in arb_height_reduction(),
        width in 60u16..100
    ) {
        let mut runtime = TestRuntime::new(width, h1);

        let mut nodes: Vec<Node> = messages
            .iter()
            .map(|msg| scrollback(&msg.key, [text(&msg.content)]))
            .collect();
        nodes.push(text("INPUT_AREA"));
        nodes.push(overlay_from_bottom(text("OVERLAY_CONTENT"), 1));

        let tree = col(nodes);

        runtime.render(&tree);

        runtime.resize(width, h2);
        runtime.render(&tree);
        let stdout_after = strip_ansi(runtime.stdout_content());
        let viewport_after = strip_ansi(runtime.viewport_content());

        for msg in &messages {
            let count_stdout = count_marker(&stdout_after, &msg.marker);
            let count_viewport = count_marker(&viewport_after, &msg.marker);

            prop_assert_eq!(
                count_stdout, 1,
                "Marker {} should appear once in stdout after resize+overlay",
                msg.marker
            );
            prop_assert_eq!(
                count_viewport, 0,
                "Marker {} should not be in viewport (graduated)",
                msg.marker
            );
        }

        prop_assert!(
            !stdout_after.contains("OVERLAY_CONTENT"),
            "Overlay content should not graduate to stdout"
        );
    }

    #[test]
    fn code_fence_boundary_no_early_graduation(
        prefix_marker in arb_marker(0),
        code_content in "[a-zA-Z0-9_() ]{10,30}",
        suffix_marker in arb_marker(1),
        (width, height) in arb_terminal_size()
    ) {
        let mut runtime = TestRuntime::new(width, height);

        let content_with_unclosed_fence = format!(
            "{}\n\n```rust\n{}\n\n{}",
            prefix_marker, code_content, suffix_marker
        );

        let tree = col([
            scrollback("msg-1", [text(&content_with_unclosed_fence)]),
            text("ANCHOR"),
        ]);

        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());

        prop_assert!(
            stdout.contains(&prefix_marker),
            "Prefix marker should be in stdout (graduated)"
        );
        prop_assert!(
            stdout.contains(&suffix_marker),
            "Suffix marker should also graduate (whole scrollback graduates)"
        );
    }

    #[test]
    fn blank_line_triggers_graduation_boundary(
        first_para in arb_message_with_marker(0),
        second_para in arb_message_with_marker(1),
        (width, height) in arb_terminal_size()
    ) {
        let (marker1, content1) = first_para;
        let (marker2, content2) = second_para;

        let mut runtime = TestRuntime::new(width, height);

        let content = format!("{}\n\n{}", content1, content2);

        let tree = col([
            scrollback("msg-1", [text(&content)]),
            text("ANCHOR"),
        ]);

        runtime.render(&tree);

        let combined = combined_content(&runtime);

        prop_assert_eq!(
            count_marker(&combined, &marker1), 1,
            "First marker should appear once"
        );
        prop_assert_eq!(
            count_marker(&combined, &marker2), 1,
            "Second marker should appear once"
        );
    }

    #[test]
    fn overlay_resize_sequence_stability(
        messages in arb_message_sequence(2..4),
        heights in arb_height_sequence(),
        width in 60u16..100
    ) {
        let initial_height = heights.first().copied().unwrap_or(25);
        let mut runtime = TestRuntime::new(width, initial_height);

        for (round, &h) in heights.iter().enumerate() {
            runtime.resize(width, h);

            let mut nodes: Vec<Node> = messages
                .iter()
                .map(|msg| scrollback(&msg.key, [text(&msg.content)]))
                .collect();
            nodes.push(text("INPUT"));

            if round % 2 == 0 {
                nodes.push(overlay_from_bottom(text("POPUP"), 1));
            }

            let tree = col(nodes);
            runtime.render(&tree);

            let combined = combined_content(&runtime);
            for msg in &messages {
                let count = count_marker(&combined, &msg.marker);
                prop_assert_eq!(
                    count, 1,
                    "Round {} h={}: marker {} should appear once, got {}",
                    round, h, msg.marker, count
                );
            }
        }
    }
}
