//! Proptest strategies for oil renderer property tests
//!
//! Provides reusable generators for testing rendering, layout, and cursor logic.

use crate::node::*;
use crate::style::*;
use crate::utils::visible_width;
use proptest::prelude::*;

/// Terminal width: 0 (edge case), narrow (1-20), normal (21-120)
pub fn arb_width() -> impl Strategy<Value = usize> {
    prop_oneof![
        1 => Just(0usize),           // Edge case: zero width
        2 => 1usize..=20,          // Narrow terminals
        7 => 21usize..=120,        // Normal terminals
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
            raw(format!("\x1b]1337;test=placeholder\x07"), w, h)
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
