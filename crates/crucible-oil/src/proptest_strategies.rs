//! Proptest strategies for oil renderer property tests
//!
//! Provides reusable generators for testing rendering, layout, and cursor logic.

use crate::node::*;
use crate::style::*;
use crate::utils::visible_width;
use proptest::prelude::*;

/// Terminal width: 0 (edge case) or realistic (20-120).
///
/// Widths below ~12 are excluded from the realistic band because `arb_node`
/// generates `Box` trees with padding up to 6 per side; at narrow widths the
/// tree's natural content size exceeds the supplied bound and Taffy lays it
/// out wider than requested (treating `width` as available-space, not a
/// hard cap). That's a real oil contract — width is a hint, not a clip —
/// and property tests that assert `line_width <= width` legitimately fail
/// in the impossibly-narrow regime.
pub fn arb_width() -> impl Strategy<Value = usize> {
    prop_oneof![
        1 => Just(0usize),           // Edge case: zero width (handled specially)
        9 => 20usize..=120,          // Realistic terminal range
    ]
}

/// Safe text generator: ASCII words with single spaces.
/// Avoids textwrap quirks with multi-byte chars initially.
pub fn arb_text() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-zA-Z0-9]{0,12}", 0..20).prop_map(|words| words.join(" "))
}

/// Small padding values to avoid huge renders
pub fn arb_padding() -> impl Strategy<Value = Padding> {
    (0u16..6, 0u16..6, 0u16..6, 0u16..6).prop_map(|(t, r, b, l)| Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    })
}

/// Border variants including None
pub fn arb_border() -> impl Strategy<Value = Option<Border>> {
    prop_oneof![
        4 => Just(None),
        1 => Just(Some(Border::Single)),
        1 => Just(Some(Border::Double)),
        1 => Just(Some(Border::Rounded)),
        1 => Just(Some(Border::Heavy)),
    ]
}

/// Gap generator for rows/columns
pub fn arb_gap() -> impl Strategy<Value = Gap> {
    (0u16..4, 0u16..4).prop_map(|(r, c)| Gap { row: r, column: c })
}

/// Size variants
pub fn arb_size() -> impl Strategy<Value = Size> {
    prop_oneof![
        3 => Just(Size::Content),
        2 => (1u16..30).prop_map(Size::Fixed),
        2 => (1u16..4).prop_map(Size::Flex),
    ]
}

/// Direction: Row or Column
pub fn arb_direction() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Column), Just(Direction::Row),]
}

/// Style generator (simple, no RGB to keep shrinking fast)
pub fn arb_style() -> impl Strategy<Value = Style> {
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(bold, dim, italic, underline)| {
            let mut s = Style::new();
            if bold {
                s = s.bold();
            }
            if dim {
                s = s.dim();
            }
            if italic {
                s = s.italic();
            }
            if underline {
                s = s.underline();
            }
            s
        },
    )
}

/// Leaf node generator: Empty, Text, Input, Spinner, Raw
pub fn arb_leaf() -> impl Strategy<Value = Node> {
    prop_oneof![
        2 => Just(Node::Empty),
        5 => arb_text().prop_map(text),
        2 => (arb_text(), 0usize..40).prop_map(|(s, c)| {
            let cursor = c.min(s.chars().count());
            text_input(s, cursor)
        }),
        1 => (prop::option::of(arb_text()), 0usize..20).prop_map(|(lbl, frame)| spinner(lbl, frame)),
        1 => (1u16..20, 1u16..5).prop_map(|(w, h)| {
            raw("\x1b]1337;test=placeholder\x07".to_string(), w, h)
        }),
    ]
}

/// Popup item generator
pub fn arb_popup_item() -> impl Strategy<Value = PopupItemNode> {
    (
        "[a-zA-Z0-9 ]{1,20}",
        prop::option::of("[a-zA-Z0-9 ]{1,30}"),
        prop::option::of("[a-zA-Z]{1,8}"),
    )
        .prop_map(|(label, desc, kind)| {
            let mut item = popup_item(label);
            if let Some(d) = desc {
                item = item.desc(d);
            }
            if let Some(k) = kind {
                item = item.kind(k);
            }
            item
        })
}

/// Popup node generator
pub fn arb_popup() -> impl Strategy<Value = Node> {
    prop::collection::vec(arb_popup_item(), 1..10).prop_flat_map(|items| {
        let len = items.len();
        (Just(items), 0..len, 1usize..=10)
            .prop_map(|(items, selected, max_visible)| popup(items, selected, max_visible.min(10)))
    })
}

/// Recursive node generator with bounded depth
pub fn arb_node() -> impl Strategy<Value = Node> {
    arb_leaf().prop_recursive(
        3,  // max depth
        32, // max size
        6,  // items per collection
        |inner| {
            prop_oneof![
                // Column with children
                3 => prop::collection::vec(inner.clone(), 0..5).prop_map(col),
                // Row with children
                3 => prop::collection::vec(inner.clone(), 0..5).prop_map(row),
                // Fragment
                2 => prop::collection::vec(inner.clone(), 0..5).prop_map(fragment),
                // Box with border
                1 => (
                    prop::collection::vec(inner.clone(), 0..3),
                    arb_direction(),
                    arb_border(),
                    arb_padding(),
                ).prop_map(|(children, dir, border, padding)| {
                    Node::Box(BoxNode {
                        children,
                        direction: dir,
                        padding,
                        border,
                        ..Default::default()
                    })
                }),
            ]
        },
    )
}

/// Node that must have some visible content (for cursor tests)
pub fn arb_visible_node() -> impl Strategy<Value = Node> {
    prop_oneof![
        5 => "[a-zA-Z0-9 ]{1,50}".prop_map(text),
        3 => (arb_text().prop_filter("non-empty", |s| !s.is_empty()), 0usize..20)
            .prop_map(|(s, c)| {
                let cursor = c.min(s.chars().count());
                text_input(s, cursor)
            }),
    ]
}

/// Input node generator with valid cursor position
pub fn arb_input() -> impl Strategy<Value = InputNode> {
    (
        arb_text(),
        prop::option::of("[a-zA-Z ]{1,20}"),
        any::<bool>(),
        arb_style(),
    )
        .prop_map(|(value, placeholder, focused, style)| {
            let cursor = if value.is_empty() {
                0
            } else {
                value.chars().count() / 2
            };
            InputNode {
                value,
                cursor,
                placeholder,
                style,
                focused,
            }
        })
}

// ─── Sequencing proofs (Stage C) ──────────────────────────────────────────

/// Terminal dimensions for sequencing tests. Skews toward common terminal
/// sizes; rare wide/narrow values stress the wrap path.
pub fn arb_dims() -> impl Strategy<Value = (u16, u16)> {
    prop_oneof![
        1 => (40u16..=60, 12u16..=20),    // small terminals
        6 => (80u16..=120, 24u16..=40),   // normal terminals
        1 => (140u16..=200, 40u16..=60),  // wide terminals
    ]
}

/// A single operation applied to a `FramePlanner`/`Terminal` pair.
/// The sequencing harness drives oil through arbitrary `Vec<Op>` and
/// checks invariants after every frame.
#[derive(Debug, Clone)]
pub enum Op {
    /// Plan a frame for `tree` with no graduation.
    RenderFrame { tree: Node },
    /// Graduate `node` (writes to scrollback), plan a frame for `viewport`.
    Graduate { node: Node, viewport: Node },
    /// Resize the planner/terminal to `(width, height)`.
    Resize { width: u16, height: u16 },
}

/// Tree generator tuned for sequencing tests: depth-bounded, branch-bounded,
/// always has *some* visible content so invariants are non-trivial.
pub fn arb_chat_like_node() -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
        4 => "[a-zA-Z0-9 ]{1,30}".prop_map(text),
        2 => Just(Node::Empty),
    ];
    leaf.prop_recursive(
        2, // shallow trees keep shrinking fast
        16,
        4,
        |inner| {
            prop_oneof![
                4 => prop::collection::vec(inner.clone(), 1..4).prop_map(col),
                2 => prop::collection::vec(inner.clone(), 1..3).prop_map(row),
                1 => prop::collection::vec(inner.clone(), 1..3).prop_map(fragment),
            ]
        },
    )
}

/// Generator for a single `Op`, weighted per Stage C plan:
/// 60% RenderFrame, 25% Graduate, 15% Resize.
pub fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        12 => arb_chat_like_node().prop_map(|tree| Op::RenderFrame { tree }),
        5 => (arb_chat_like_node(), arb_chat_like_node())
            .prop_map(|(node, viewport)| Op::Graduate { node, viewport }),
        3 => arb_dims().prop_map(|(width, height)| Op::Resize { width, height }),
    ]
}

/// A bounded sequence of `Op`s. 1..25 covers typical session flows
/// without blowing up shrink time.
pub fn arb_operation_sequence() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(arb_op(), 1..=25)
}

// ─── End sequencing proofs ────────────────────────────────────────────────

/// Helper: Assert all rendered lines fit within width
pub fn assert_render_fits_width(output: &str, width: usize) -> Result<(), TestCaseError> {
    for (i, line) in output.split("\r\n").enumerate() {
        let line_width = visible_width(line);
        prop_assert!(
            line_width <= width || width == 0,
            "Line {} exceeds width {}: got {} (content: {:?})",
            i,
            width,
            line_width,
            if line.len() > 100 {
                format!("{}...", &line[..100])
            } else {
                line.to_string()
            }
        );
    }
    Ok(())
}

/// Helper: Assert all rendered lines have exactly the expected width (for bordered content)
pub fn assert_lines_exact_width(output: &str, width: usize) -> Result<(), TestCaseError> {
    for (i, line) in output.split("\r\n").enumerate() {
        // Skip empty lines
        if line.is_empty() {
            continue;
        }
        let line_width = visible_width(line);
        prop_assert!(
            line_width == width,
            "Line {} should be exactly {} wide, got {} (content: {:?})",
            i,
            width,
            line_width,
            if line.len() > 100 {
                format!("{}...", &line[..100])
            } else {
                line.to_string()
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    #[test]
    fn arb_width_produces_valid_values() {
        let mut runner = TestRunner::default();
        for _ in 0..100 {
            let w = arb_width().new_tree(&mut runner).unwrap().current();
            assert!(w <= 120);
        }
    }

    #[test]
    fn arb_node_produces_valid_trees() {
        let mut runner = TestRunner::default();
        for _ in 0..20 {
            let node = arb_node().new_tree(&mut runner).unwrap().current();
            // Just verify it doesn't panic
            let _ = format!("{:?}", node);
        }
    }

    #[test]
    fn arb_input_has_valid_cursor() {
        let mut runner = TestRunner::default();
        for _ in 0..50 {
            let input = arb_input().new_tree(&mut runner).unwrap().current();
            assert!(input.cursor <= input.value.chars().count());
        }
    }
}
