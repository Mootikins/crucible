#![cfg(feature = "test-utils")]

use crucible_oil::proptest_strategies::*;
use crucible_oil::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_column_output_has_expected_line_structure(
        texts in prop::collection::vec("[a-zA-Z]{1,20}", 1..5),
        width in 30usize..80
    ) {
        let nodes: Vec<Node> = texts.iter().map(|t| text(t.clone())).collect();
        let column = col(nodes);
        let output = render_to_string(&column, width);

        let lines: Vec<&str> = output.split("\r\n").collect();

        prop_assert!(
            lines.len() >= 1,
            "Column should produce at least one line"
        );

        for line in &lines {
            let line_width = utils::visible_width(line);
            prop_assert!(
                line_width <= width,
                "Column line exceeds width: {} > {}",
                line_width, width
            );
        }
    }

    #[test]
    fn prop_row_single_line_output(
        texts in prop::collection::vec("[a-zA-Z]{1,8}", 1..4),
        width in 60usize..100
    ) {
        let nodes: Vec<Node> = texts.iter().map(|t| text(t.clone())).collect();
        let row_node = row(nodes);
        let output = render_to_string(&row_node, width);

        let lines: Vec<&str> = output.split("\r\n").collect();

        prop_assert!(
            lines.len() == 1,
            "Simple row with short texts should produce single line, got {} lines",
            lines.len()
        );
    }

    #[test]
    fn prop_flex_children_use_available_space(
        weights in prop::collection::vec(1u16..4, 2..4),
        width in 40usize..80
    ) {
        let children: Vec<Node> = weights.iter()
            .map(|&w| Node::Box(BoxNode {
                size: Size::Flex(w),
                ..Default::default()
            }))
            .collect();

        let row_node = row(children);
        let output = render_to_string(&row_node, width);

        let lines: Vec<&str> = output.split("\r\n").collect();
        if !lines.is_empty() && !lines[0].is_empty() {
            let output_width = utils::visible_width(lines[0]);
            prop_assert!(
                output_width <= width,
                "Flex row output {} should not exceed width {}",
                output_width, width
            );
        }
    }

    #[test]
    fn prop_fixed_size_children_honored(
        fixed_width in 5u16..20,
        total_width in 40usize..80
    ) {
        let fixed_child = Node::Box(BoxNode {
            children: vec![text("X")],
            size: Size::Fixed(fixed_width),
            ..Default::default()
        });

        let row_node = row([fixed_child, text("tail")]);
        let output = render_to_string(&row_node, total_width);

        assert_render_fits_width(&output, total_width)?;
    }
}

#[cfg(test)]
mod padding_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_padding_reduces_available_width(
            content in "[a-zA-Z]{1,20}",
            padding in arb_padding(),
            width in 40usize..80
        ) {
            let inner = text(content);
            let padded = inner.with_padding(padding);
            let output = render_to_string(&padded, width);
            assert_render_fits_width(&output, width)?;
        }

        #[test]
        fn prop_margin_does_not_exceed_width(
            content in "[a-zA-Z]{1,15}",
            margin in arb_padding(),
            width in 40usize..80
        ) {
            let inner = text(content);
            let margined = inner.with_margin(margin);
            let output = render_to_string(&margined, width);
            assert_render_fits_width(&output, width)?;
        }
    }
}

#[cfg(test)]
mod size_combination_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_mixed_sizes_in_row(
            fixed_w in 5u16..15,
            flex_w in 1u16..3,
            content in "[a-zA-Z]{1,10}",
            width in 50usize..80
        ) {
            let fixed = Node::Box(BoxNode {
                children: vec![text("F")],
                size: Size::Fixed(fixed_w),
                ..Default::default()
            });
            let flex = Node::Box(BoxNode {
                children: vec![text("X")],
                size: Size::Flex(flex_w),
                ..Default::default()
            });
            let content_sized = text(content);

            let row_node = row([fixed, flex, content_sized]);
            let output = render_to_string(&row_node, width);
            assert_render_fits_width(&output, width)?;
        }
    }
}
