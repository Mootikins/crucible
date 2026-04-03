//! Property tests for graduation rendering invariants.
//!
//! These verify structural properties of the graduation system:
//! - Graduated content never contains spinner characters
//! - Sync update markers are always balanced
//! - Rendering sequences through Terminal produce clean scrollback

#[cfg(feature = "test-utils")]
mod tests {
    use crucible_oil::node::*;
    use crucible_oil::planning::Graduation;
    use crucible_oil::proptest_strategies::*;
    use crucible_oil::TestRuntime;
    use proptest::prelude::*;

    /// Generate a node tree that simulates graduation content.
    /// Graduation content should never contain Spinner nodes since graduated
    /// containers are complete. This strategy generates trees without spinners
    /// to model correct graduation content.
    fn arb_graduation_node() -> impl Strategy<Value = Node> {
        // Same as arb_leaf but without spinners
        let leaf = prop_oneof![
            2 => Just(Node::Empty),
            6 => arb_text().prop_map(text),
            2 => (arb_text(), 0usize..40).prop_map(|(s, c)| {
                let cursor = c.min(s.chars().count());
                text_input(s, cursor)
            }),
        ];
        leaf.prop_recursive(
            3,
            32,
            6,
            |inner| {
                prop_oneof![
                    3 => prop::collection::vec(inner.clone(), 0..5).prop_map(col),
                    3 => prop::collection::vec(inner.clone(), 0..5).prop_map(row),
                    2 => prop::collection::vec(inner.clone(), 0..5).prop_map(fragment),
                ]
            },
        )
    }

    /// Generate a Graduation struct with random content and dimensions.
    fn arb_graduation() -> impl Strategy<Value = Graduation> {
        (arb_graduation_node(), 20u16..200).prop_map(|(node, width)| Graduation { node, width })
    }

    /// Generate a viewport node that MAY contain spinners (the normal case).
    fn arb_viewport_node() -> impl Strategy<Value = Node> {
        arb_node()
    }

    /// Terminal dimensions: height 5-50, width 40-200
    fn arb_terminal_dims() -> impl Strategy<Value = (u16, u16)> {
        prop_oneof![
            2 => (5u16..=12, 40u16..=80),     // Small terminals
            5 => (20u16..=40, 80u16..=120),    // Normal terminals
            1 => (40u16..=50, 120u16..=200),   // Large terminals
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Graduated content rendered to string never contains spinner characters.
        /// This is a structural invariant: completed containers are rendered with
        /// is_complete=true, which suppresses spinner nodes.
        #[test]
        fn graduation_content_has_no_spinners(grad in arb_graduation()) {
            let rendered = grad.render();

            for ch in SPINNER_FRAMES.iter().chain(BRAILLE_SPINNER_FRAMES.iter()) {
                prop_assert!(
                    !rendered.contains(*ch),
                    "Spinner char '{}' found in graduation content:\n{}",
                    ch,
                    crucible_oil::ansi::strip_ansi(&rendered)
                );
            }
        }

        /// Rendering a graduation followed by a viewport produces balanced
        /// sync update markers in the raw byte output.
        #[test]
        fn graduation_sequence_has_balanced_sync_markers(
            grad in arb_graduation(),
            viewport in arb_viewport_node(),
            dims in arb_terminal_dims(),
        ) {
            let (width, height) = dims;
            let mut runtime = TestRuntime::new(width, height);

            // Render with graduation
            runtime.render_with_graduation(&viewport, Some(&grad));
            let bytes = runtime.take_bytes();
            let byte_str = String::from_utf8_lossy(&bytes);

            let begin_count = byte_str.matches("\x1b[?2026h").count();
            let end_count = byte_str.matches("\x1b[?2026l").count();

            prop_assert_eq!(
                begin_count,
                end_count,
                "Unbalanced sync markers: {} begins, {} ends.\nBytes:\n{}",
                begin_count,
                end_count,
                byte_str.replace('\x1b', "ESC")
            );
        }

        /// Multiple sequential graduations produce correct byte output:
        /// sync markers are always balanced across the full sequence.
        #[test]
        fn sequential_graduations_balanced_sync_markers(
            grads in prop::collection::vec(arb_graduation(), 1..6),
            viewport in arb_viewport_node(),
            dims in arb_terminal_dims(),
        ) {
            let (width, height) = dims;
            let mut runtime = TestRuntime::new(width, height);

            for grad in &grads {
                runtime.render_with_graduation(&viewport, Some(grad));
            }
            // Final render without graduation
            runtime.render_with_graduation(&viewport, None);

            // Check cumulative bytes
            let bytes = runtime.take_bytes();
            let byte_str = String::from_utf8_lossy(&bytes);

            let begin_count = byte_str.matches("\x1b[?2026h").count();
            let end_count = byte_str.matches("\x1b[?2026l").count();

            prop_assert_eq!(
                begin_count,
                end_count,
                "Unbalanced sync markers across {} graduations: {} begins, {} ends",
                grads.len(),
                begin_count,
                end_count,
            );
        }
    }
}
