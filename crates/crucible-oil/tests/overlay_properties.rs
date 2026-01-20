#![cfg(feature = "test-utils")]

use crucible_oil::proptest_strategies::*;
use crucible_oil::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

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
        #![proptest_config(ProptestConfig::with_cases(50))]

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
