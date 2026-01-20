#![cfg(feature = "test-utils")]

use crucible_oil::proptest_strategies::*;
use crucible_oil::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_cursor_in_bounds(
        value in "[a-zA-Z0-9 ]{1,50}",
        width in 20usize..100
    ) {
        let char_count = value.chars().count();
        let cursor_pos = char_count / 2;
        let input = text_input(value, cursor_pos);
        let result = render_with_cursor(&input, width);

        if result.cursor.visible {
            let lines: Vec<&str> = result.content.split("\r\n").collect();
            let total_lines = lines.len();

            prop_assert!(
                (result.cursor.row_from_end as usize) < total_lines,
                "Cursor row_from_end {} should be < total lines {}",
                result.cursor.row_from_end, total_lines
            );

            let line_idx = total_lines.saturating_sub(1).saturating_sub(result.cursor.row_from_end as usize);
            if line_idx < lines.len() {
                let line_width = utils::visible_width(lines[line_idx]);
                prop_assert!(
                    (result.cursor.col as usize) <= line_width.max(1),
                    "Cursor col {} should be <= line width {} (line: {:?})",
                    result.cursor.col, line_width, lines[line_idx]
                );
            }
        }
    }

    #[test]
    fn prop_cursor_invisible_without_focus(
        value in "[a-zA-Z0-9]{0,30}",
        width in 20usize..80
    ) {
        let cursor_pos = value.chars().count() / 2;
        let input = Node::Input(InputNode {
            value,
            cursor: cursor_pos,
            placeholder: None,
            style: Style::default(),
            focused: false,
        });
        let result = render_with_cursor(&input, width);
        prop_assert!(
            !result.cursor.visible,
            "Unfocused input should not show cursor"
        );
    }

    #[test]
    fn prop_cursor_visible_with_focus(
        value in "[a-zA-Z]{1,20}",
        width in 20usize..80
    ) {
        let cursor_pos = value.chars().count() / 2;
        let input = text_input(value, cursor_pos);
        let result = render_with_cursor(&input, width);
        prop_assert!(
            result.cursor.visible,
            "Focused input should show cursor"
        );
    }

    #[test]
    fn prop_cursor_at_valid_char_position(
        value in "[a-zA-Z0-9]{1,30}",
        cursor_ratio in 0.0f64..=1.0
    ) {
        let char_count = value.chars().count();
        let cursor_pos = ((char_count as f64) * cursor_ratio).floor() as usize;
        let cursor_pos = cursor_pos.min(char_count);

        let input = text_input(value.clone(), cursor_pos);
        let result = render_with_cursor(&input, 80);

        if result.cursor.visible {
            prop_assert!(
                (result.cursor.col as usize) <= char_count,
                "Cursor col {} should be <= char count {}",
                result.cursor.col, char_count
            );
        }
    }
}

#[cfg(test)]
mod input_in_container_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_cursor_in_row_is_offset(
            prefix in "[a-zA-Z]{1,10}",
            value in "[a-zA-Z]{1,20}",
            width in 40usize..80
        ) {
            let cursor_pos = value.chars().count() / 2;
            let prefix_node = text(prefix.clone());
            let input_node = text_input(value, cursor_pos);
            let container = row([prefix_node, input_node]);

            let result = render_with_cursor(&container, width);

            if result.cursor.visible {
                let prefix_width = prefix.chars().count();
                prop_assert!(
                    (result.cursor.col as usize) >= prefix_width,
                    "Cursor col {} should be >= prefix width {}",
                    result.cursor.col, prefix_width
                );
            }
        }

        #[test]
        fn prop_cursor_in_bordered_box_is_offset(
            value in "[a-zA-Z]{1,15}",
            width in 30usize..80
        ) {
            let cursor_pos = value.chars().count() / 2;
            let input = text_input(value, cursor_pos);
            let bordered = input.with_border(Border::Single);

            let result = render_with_cursor(&bordered, width);

            if result.cursor.visible {
                prop_assert!(
                    result.cursor.col >= 1,
                    "Cursor in bordered box should be offset by border"
                );
            }
        }
    }
}
