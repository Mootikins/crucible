#![cfg(feature = "test-utils")]

use crucible_oil::utils::{strip_ansi, visible_width, visual_rows};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_strip_ansi_idempotent(s in ".*") {
        let once = strip_ansi(&s);
        let twice = strip_ansi(&once);
        prop_assert_eq!(
            once, twice,
            "strip_ansi should be idempotent"
        );
    }

    #[test]
    fn prop_strip_ansi_no_escape_sequences(s in ".*") {
        let stripped = strip_ansi(&s);
        prop_assert!(
            !stripped.contains('\x1b'),
            "Stripped string should not contain escape character"
        );
    }

    #[test]
    fn prop_visible_width_leq_byte_len(s in "[a-zA-Z0-9 ]{0,100}") {
        let width = visible_width(&s);
        let char_count = s.chars().count();
        prop_assert!(
            width <= char_count,
            "Visible width {} should be <= char count {} for ASCII",
            width, char_count
        );
    }

    #[test]
    fn prop_visible_width_equals_char_count_for_ascii(s in "[a-zA-Z0-9]{0,50}") {
        let width = visible_width(&s);
        let char_count = s.chars().count();
        prop_assert_eq!(
            width, char_count,
            "Visible width should equal char count for pure ASCII alphanumeric"
        );
    }

    #[test]
    fn prop_visual_rows_monotonic_with_width(
        s in "[a-zA-Z0-9 ]{1,100}",
        w1 in 10usize..50,
        w2 in 50usize..100
    ) {
        let rows_narrow = visual_rows(&s, w1);
        let rows_wide = visual_rows(&s, w2);
        prop_assert!(
            rows_narrow >= rows_wide,
            "Wider terminal should have <= rows: narrow({})={} wide({})={}",
            w1, rows_narrow, w2, rows_wide
        );
    }

    #[test]
    fn prop_visual_rows_at_least_one(s in ".*", width in 1usize..100) {
        let rows = visual_rows(&s, width);
        prop_assert!(
            rows >= 1,
            "visual_rows should return at least 1"
        );
    }

    #[test]
    fn prop_visual_rows_zero_width_returns_one(s in ".*") {
        let rows = visual_rows(&s, 0);
        prop_assert_eq!(
            rows, 1,
            "visual_rows with zero width should return 1"
        );
    }
}

#[cfg(test)]
mod ansi_with_codes_tests {
    use super::*;

    fn make_colored(s: &str) -> String {
        format!("\x1b[31m{}\x1b[0m", s)
    }

    fn make_bold(s: &str) -> String {
        format!("\x1b[1m{}\x1b[0m", s)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_visible_width_ignores_color_codes(s in "[a-zA-Z0-9]{1,30}") {
            let plain_width = visible_width(&s);
            let colored = make_colored(&s);
            let colored_width = visible_width(&colored);
            prop_assert_eq!(
                plain_width, colored_width,
                "Color codes should not affect visible width"
            );
        }

        #[test]
        fn prop_visible_width_ignores_bold_codes(s in "[a-zA-Z0-9]{1,30}") {
            let plain_width = visible_width(&s);
            let bold = make_bold(&s);
            let bold_width = visible_width(&bold);
            prop_assert_eq!(
                plain_width, bold_width,
                "Bold codes should not affect visible width"
            );
        }

        #[test]
        fn prop_strip_ansi_removes_color(s in "[a-zA-Z0-9]{1,30}") {
            let colored = make_colored(&s);
            let stripped = strip_ansi(&colored);
            prop_assert_eq!(
                s, stripped,
                "strip_ansi should recover original text from colored"
            );
        }

        #[test]
        fn prop_visual_rows_same_with_or_without_ansi(
            s in "[a-zA-Z0-9]{1,50}",
            width in 10usize..50
        ) {
            let plain_rows = visual_rows(&s, width);
            let colored = make_colored(&s);
            let colored_rows = visual_rows(&colored, width);
            prop_assert_eq!(
                plain_rows, colored_rows,
                "ANSI codes should not affect visual_rows"
            );
        }
    }
}
