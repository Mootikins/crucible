#![cfg(feature = "test-utils")]

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
        let input = text_input(value.clone(), cursor_pos);
        let result = render_with_cursor(&input, width);

        if result.cursor.visible {
            let lines: Vec<&str> = result.content.split("\r\n").collect();
            let total_lines = lines.len();

            prop_assert!(
                (result.cursor.row_from_end as usize) < total_lines,
                "Cursor row_from_end {} should be < total lines {}",
                result.cursor.row_from_end, total_lines
            );

            // The cursor may never leave the terminal.
            prop_assert!(
                (result.cursor.col as usize) <= width,
                "Cursor col {} should be <= terminal width {}",
                result.cursor.col, width
            );

            // Rendered lines are right-trimmed of unstyled trailing padding,
            // and the cursor legitimately sits AFTER trailing spaces it was
            // typed past (EOL cursor sits after the text, e81e80185) — e.g.
            // value "a   " with cursor 2 renders line "a" with cursor col 2.
            // The strict per-line bound therefore only holds for values the
            // trim cannot have shortened.
            if !value.contains(' ') {
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

/// CI counterexample (run 29138851287): a value with trailing spaces renders
/// as a right-trimmed line, and the cursor rests inside the trimmed padding.
/// The renderer keeps the cursor at its typed position; the trimmed content
/// must not pull it back onto the last glyph.
#[test]
fn cursor_rests_inside_trimmed_trailing_padding() {
    let input = text_input("a   ".to_string(), 2);
    let result = render_with_cursor(&input, 20);
    assert!(result.cursor.visible);
    assert_eq!(result.content, "a", "trailing padding is right-trimmed");
    assert_eq!(result.cursor.col, 2, "cursor stays at its typed position");
}
