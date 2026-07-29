#![cfg(feature = "test-utils")]

// Consolidated from layout_properties.rs, overlay_properties.rs, and
// render_properties.rs. Each former file lives behind its own `mod` block so
// the test function names (which T8's regression seeds key on) are unchanged;
// only the file location moved.

mod common;

use common::default_cases;
use crucible_oil::proptest_strategies::*;
use crucible_oil::*;
use proptest::prelude::*;

mod layout {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(default_cases().max(200)))]

        #[test]
        fn prop_column_output_has_expected_line_structure(
            texts in prop::collection::vec("[a-zA-Z]{1,20}", 1..5),
            width in 30usize..80
        ) {
            let nodes: Vec<Node> = texts.iter().map(|t| text(t.clone())).collect();
            let column = col(nodes);
            let output = render_to_string(&column, width);

            let lines: Vec<&str> = output.split("\r\n").collect();

            prop_assert!(!lines.is_empty(), "Column should produce at least one line");

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
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(100)))]

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
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(100)))]

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
}

mod overlay {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(default_cases().max(100)))]

        #[test]
        fn prop_overlay_renders_child(
            content in "[a-zA-Z]{1,30}",
            offset in 0usize..5,
            width in 30usize..80
        ) {
            let child = text(content.clone());
            let overlay = overlay_from_bottom(child, offset);
            let output = render_to_string(&overlay, width);

            prop_assert!(
                output.contains(&content) || content.is_empty(),
                "Overlay should render its child content"
            );
            assert_render_fits_width(&output, width)?;
        }

        #[test]
        fn prop_overlay_with_box_fits_width(
            content in "[a-zA-Z]{1,20}",
            offset in 0usize..3,
            width in 30usize..80
        ) {
            let inner = text(content).with_border(Border::Single);
            let overlay = overlay_from_bottom(inner, offset);
            let output = render_to_string(&overlay, width);
            assert_render_fits_width(&output, width)?;
        }

        #[test]
        fn prop_overlay_nested_in_column_fits(
            texts in prop::collection::vec("[a-zA-Z]{1,15}", 1..4),
            overlay_content in "[a-zA-Z]{1,10}",
            width in 40usize..80
        ) {
            let mut children: Vec<Node> = texts.iter().map(|t| text(t.clone())).collect();
            let overlay = overlay_from_bottom(text(overlay_content), 1);
            children.push(overlay);

            let column = col(children);
            let output = render_to_string(&column, width);
            assert_render_fits_width(&output, width)?;
        }
    }

    #[cfg(test)]
    mod composite_overlay_tests {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(50)))]

            #[test]
            fn prop_composite_overlays_fit_dimensions(
                base_lines in prop::collection::vec("[a-zA-Z ]{1,30}", 3..8),
                overlay_lines in prop::collection::vec("[a-zA-Z]{1,15}", 1..3),
                width in 40usize..80
            ) {
                let base: Vec<String> = base_lines.iter()
                    .map(|line| {
                        let w = utils::visible_width(line);
                        if w < width {
                            format!("{}{}", line, " ".repeat(width - w))
                        } else {
                            line.chars().take(width).collect()
                        }
                    })
                    .collect();

                let overlay = Overlay::from_bottom(overlay_lines.clone(), 1);

                let result = composite_overlays(&base, &[overlay], width);

                prop_assert!(
                    result.len() >= base.len(),
                    "Composite should maintain or grow height"
                );

                for (i, line) in result.iter().enumerate() {
                    let line_width = utils::visible_width(line);
                    prop_assert!(
                        line_width <= width,
                        "Composite line {} exceeds width: {} > {}",
                        i, line_width, width
                    );
                }
            }
        }
    }
}

mod render {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(default_cases().max(200)))]

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
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(100)))]

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
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(100)))]

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

    #[cfg(test)]
    mod styled_tests {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(default_cases().max(200)))]

            #[test]
            fn prop_styled_node_renders_within_width(
                node in arb_visible_node(),
                style in arb_style(),
                width in arb_width()
            ) {
                let styled = node.with_style(style);
                let output = render_to_string(&styled, width);
                assert_render_fits_width(&output, width)?;
            }
        }
    }
}
