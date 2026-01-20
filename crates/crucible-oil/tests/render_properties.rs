#![cfg(feature = "test-utils")]

use crucible_oil::proptest_strategies::*;
use crucible_oil::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_render_fits_width(node in arb_node(), width in arb_width()) {
        let output = render_to_string(&node, width);
        assert_render_fits_width(&output, width)?;
    }

    #[test]
    fn prop_render_idempotent(node in arb_node(), width in 1usize..100) {
        let output1 = render_to_string(&node, width);
        let output2 = render_to_string(&node, width);
        prop_assert_eq!(output1, output2, "Rendering should be deterministic");
    }

    #[test]
    fn prop_render_entrypoints_agree(node in arb_node(), width in 1usize..100) {
        let via_string = render_to_string(&node, width);
        let via_cursor = render_with_cursor(&node, width);
        prop_assert_eq!(
            via_string, via_cursor.content,
            "render_to_string and render_with_cursor.content should match"
        );
    }

    #[test]
    fn prop_row_never_exceeds_width(
        children in prop::collection::vec(arb_leaf(), 0..5),
        width in 10usize..100
    ) {
        let node = row(children);
        let output = render_to_string(&node, width);
        assert_render_fits_width(&output, width)?;
    }

    #[test]
    fn prop_popup_line_count_and_width(
        items in prop::collection::vec(arb_popup_item(), 1..8),
        width in 20usize..100
    ) {
        let len = items.len();
        let selected = len / 2;
        let max_visible = len.min(5);
        let popup_node = popup(items, selected, max_visible);

        let output = render_to_string(&popup_node, width);
        let lines: Vec<&str> = output.split("\r\n").collect();

        prop_assert_eq!(
            lines.len(), max_visible,
            "Popup should have exactly max_visible lines"
        );

        for (i, line) in lines.iter().enumerate() {
            let line_width = utils::visible_width(line);
            prop_assert!(
                line_width <= width,
                "Popup line {} exceeds width {}: got {}",
                i, width, line_width
            );
        }
    }
}

#[cfg(test)]
mod bordered_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_bordered_box_fits_width(
            content in arb_text(),
            border in arb_border().prop_filter("has border", |b| b.is_some()),
            width in 10usize..80
        ) {
            let inner = text(content);
            let node = inner.with_border(border.unwrap());
            let output = render_to_string(&node, width);
            assert_render_fits_width(&output, width)?;
        }

        #[test]
        fn prop_nested_borders_fit_width(
            content in "[a-zA-Z ]{1,20}",
            width in 20usize..80
        ) {
            let inner = text(content);
            let node = inner
                .with_border(Border::Single)
                .with_border(Border::Rounded);
            let output = render_to_string(&node, width);
            assert_render_fits_width(&output, width)?;
        }
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_column_children_fit_width(
            children in prop::collection::vec(arb_leaf(), 1..6),
            width in 10usize..80
        ) {
            let node = col(children);
            let output = render_to_string(&node, width);
            assert_render_fits_width(&output, width)?;
        }

        #[test]
        fn prop_deeply_nested_columns_fit_width(
            texts in prop::collection::vec("[a-zA-Z ]{1,15}", 2..5),
            width in 20usize..80
        ) {
            let nodes: Vec<Node> = texts.into_iter().map(text).collect();
            let inner = col(nodes);
            let middle = col([inner]);
            let outer = col([middle]);
            let output = render_to_string(&outer, width);
            assert_render_fits_width(&output, width)?;
        }
    }
}
