//! Adversarial markdown fuzz tests for `markdown_to_node`.
//!
//! Property-based tests that feed malformed, edge-case, and adversarial markdown
//! through the rendering pipeline to ensure it never panics or crashes.
//! Each test wraps the call in `catch_unwind` so that even upstream parser panics
//! are caught and reported as test failures rather than aborting the suite.

use crate::tui::oil::markdown::{markdown_to_node, markdown_to_node_with_width};
use crucible_oil::render::render_to_string;
use proptest::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render markdown through the full pipeline, catching any panics.
/// Returns Err if the pipeline panicked.
fn safe_render(md: &str) -> Result<String, String> {
    let md = md.to_string();
    catch_unwind(AssertUnwindSafe(move || {
        let node = markdown_to_node(&md);
        render_to_string(&node, 80)
    }))
    .map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        }
    })
}

/// Render markdown with a specific width, catching any panics.
fn safe_render_with_width(md: &str, width: usize) -> Result<String, String> {
    let md = md.to_string();
    catch_unwind(AssertUnwindSafe(move || {
        let node = markdown_to_node_with_width(&md, width);
        render_to_string(&node, width)
    }))
    .map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        }
    })
}

// ---------------------------------------------------------------------------
// Deterministic adversarial inputs (always run, no randomness)
// ---------------------------------------------------------------------------

fn adversarial_inputs() -> Vec<String> {
    let mut inputs: Vec<String> = vec![
        // Empty / whitespace
        "".into(),
        " ".into(),
        "   ".into(),
        "\t".into(),
        "\n".into(),
        "\n\n\n".into(),
        "\r\n".into(),
        "\r".into(),
        // Single characters
        "#".into(),
        "*".into(),
        "_".into(),
        "`".into(),
        "|".into(),
        ">".into(),
        "-".into(),
        "\\".into(),
        // Unclosed formatting
        "**bold".into(),
        "*italic".into(),
        "~~strike".into(),
        "`code".into(),
        "```\nunclosed fence".into(),
        "```rust\nfn main() {}\n".into(),
        // Deeply nested lists (12 levels)
        "- a\n  - b\n    - c\n      - d\n        - e\n          - f\n            - g\n              - h\n                - i\n                  - j\n                    - k\n                      - l".into(),
        // Deeply nested blockquotes
        "> > > > > > > > > > deeply nested quote".into(),
        // Malformed tables
        "| |\n||\n||".into(),
        "| a | b |\n|---|".into(),
        "| a | b | c |\n|---|---|\n| 1 |".into(),
        "||||\n||||\n||||".into(),
        "| | | |\n|---|---|---|\n".into(),
        "| header |\n|---|\n| cell |\n| extra | columns | here |".into(),
        // Backtick sequences
        "`` ` ``".into(),
        "``` `` ```".into(),
        "````\n```\n````".into(),
        "`````".into(),
        // Null bytes and control characters
        "hello\x00world".into(),
        "test\x01\x02\x03".into(),
        "line\x1b[31mred\x1b[0m".into(),
        "\x1b\x1b\x1b".into(),
        // Rapid formatting toggles
        "**__**__**__**__**__**__".into(),
        "***___***___***___***___".into(),
        "*_*_*_*_*_*_*_*_*_*_*_*".into(),
        // Mixed valid/invalid
        "# Heading\n\n**bold *nested italic** more*\n\n> quote\n\n```\ncode\n```\n\n| broken | table\n|---\n| cell".into(),
        // Heading edge cases
        "######".into(),
        "####### seven hashes".into(),
        "# ".into(),
        "## \n## \n## ".into(),
        // Link edge cases
        "[]()".into(),
        "[text]()".into(),
        "[](url)".into(),
        "[[wikilink]]".into(),
        "[link](url \"title with \\\" escaped\")".into(),
        "[unclosed link(url)".into(),
        // Image edge cases
        "![]()".into(),
        "![alt](broken url with spaces)".into(),
        // Horizontal rules
        "---".into(),
        "***".into(),
        "___".into(),
        "------------------------------------".into(),
        // Lists with empty items
        "- \n- \n- ".into(),
        "1. \n2. \n3. ".into(),
        "- item\n-\n- item".into(),
        // Mixed list types
        "- bullet\n1. ordered\n- bullet\n2. ordered".into(),
        // Code blocks with special content
        "```\n\x00\x01\x02\n```".into(),
        "```\n\x1b[31mred\x1b[0m\n```".into(),
        "```\n<script>alert('xss')</script>\n```".into(),
        // Unicode edge cases
        "🎉🎊🎈🎁🎀".into(),
        "👨\u{200D}👩\u{200D}👧\u{200D}👦".into(), // ZWJ family
        "مرحبا".into(),                             // RTL Arabic
        "שלום".into(),                               // RTL Hebrew
        "你好世界".into(),                           // CJK
        "日本語テスト".into(),                       // Japanese
        "한국어".into(),                             // Korean
        "café résumé naïve".into(),                  // Latin with diacritics
        "\u{200B}\u{200B}\u{200B}".into(),           // Zero-width spaces
        "\u{FEFF}BOM marker".into(),                 // BOM
        "\u{200F}RTL mark\u{200E}LTR mark".into(),   // Directional marks
        "a\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}".into(), // Stacked combining marks
        // Pathological emphasis patterns
        "a]b".into(),
        "*a]b*".into(),
        "**a **b **c **d **e **f **g **h".into(),
        "__a __b __c __d __e __f __g __h".into(),
        // Trailing/leading whitespace oddities
        "  # indented heading  ".into(),
        "    code block by indent".into(),
        "\there\ttabs\there".into(),
    ];

    // Dynamic inputs that require runtime allocation
    inputs.push("x".repeat(10_000));
    inputs.push("a ".repeat(5_000));
    inputs.push("# ".repeat(100));
    inputs.push("> ".repeat(100));
    inputs.push("- ".repeat(100));
    inputs.push("| ".repeat(100));

    // Large table (20 columns, 10 rows)
    let header = format!(
        "| {} |",
        (0..20)
            .map(|i| format!("h{}", i))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let sep = format!("|{}|", (0..20).map(|_| "---").collect::<Vec<_>>().join("|"));
    let rows = (0..10)
        .map(|r| {
            format!(
                "| {} |",
                (0..20)
                    .map(|c| format!("r{}c{}", r, c))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    inputs.push(format!("{}\n{}\n{}", header, sep, rows));

    inputs
}

#[test]
fn markdown_fuzz_deterministic_adversarial_inputs() {
    let inputs = adversarial_inputs();
    let mut failures = Vec::new();

    for (i, input) in inputs.iter().enumerate() {
        if let Err(panic_msg) = safe_render(input) {
            failures.push(format!(
                "Input #{} panicked: {}\n  Input (first 200 chars): {:?}",
                i,
                panic_msg,
                &input[..input.len().min(200)]
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "markdown_to_node panicked on {} / {} adversarial inputs:\n{}",
        failures.len(),
        inputs.len(),
        failures.join("\n\n")
    );
}

// ---------------------------------------------------------------------------
// Property-based fuzz tests (proptest)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Completely arbitrary byte strings interpreted as UTF-8.
    /// The renderer must never panic regardless of content.
    #[test]
    fn markdown_fuzz_arbitrary_string(input in "\\PC{0,500}") {
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on arbitrary input: {:?}\nPanic: {}",
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Strings built from markdown-significant characters.
    #[test]
    fn markdown_fuzz_markdown_chars(
        input in prop::string::string_regex("[#*_`|>\\-\\[\\]()!\\\\~ \n\t]{0,300}").unwrap()
    ) {
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on markdown-chars input: {:?}\nPanic: {}",
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Mixed valid markdown fragments interleaved with garbage.
    #[test]
    fn markdown_fuzz_mixed_valid_invalid(
        fragments in prop::collection::vec(
            prop_oneof![
                // Valid markdown fragments
                Just("# Heading\n".to_string()),
                Just("**bold**".to_string()),
                Just("*italic*".to_string()),
                Just("`code`".to_string()),
                Just("- list item\n".to_string()),
                Just("> quote\n".to_string()),
                Just("```\ncode\n```\n".to_string()),
                Just("| a | b |\n|---|---|\n| 1 | 2 |\n".to_string()),
                Just("---\n".to_string()),
                // Garbage fragments
                Just("\x00\x01\x02".to_string()),
                Just("***___~~~".to_string()),
                Just("||||".to_string()),
                Just("```".to_string()),
                Just("[[[[".to_string()),
                Just("]()(][".to_string()),
            ],
            1..15
        )
    ) {
        let input = fragments.join("");
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on mixed input: {:?}\nPanic: {}",
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Deeply nested list structures generated by proptest.
    #[test]
    fn markdown_fuzz_deep_nested_lists(depth in 5usize..30, item in "[a-z]{1,10}") {
        let input: String = (0..depth)
            .map(|d| format!("{}- {}\n", "  ".repeat(d), item))
            .collect();
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on nested list (depth {}): {:?}\nPanic: {}",
            depth,
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Tables with mismatched column counts.
    #[test]
    fn markdown_fuzz_malformed_tables(
        header_cols in 1usize..8,
        separator_cols in 0usize..8,
        row_cols in 0usize..8,
        num_rows in 0usize..5,
    ) {
        let header = format!("| {} |", (0..header_cols).map(|i| format!("h{}", i)).collect::<Vec<_>>().join(" | "));
        let sep = if separator_cols > 0 {
            format!("|{}|", (0..separator_cols).map(|_| "---").collect::<Vec<_>>().join("|"))
        } else {
            "|".to_string()
        };
        let rows: String = (0..num_rows)
            .map(|r| {
                if row_cols > 0 {
                    format!("| {} |", (0..row_cols).map(|c| format!("r{}c{}", r, c)).collect::<Vec<_>>().join(" | "))
                } else {
                    "||".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let input = format!("{}\n{}\n{}", header, sep, rows);
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on malformed table: {:?}\nPanic: {}",
            &input[..input.len().min(300)],
            result.unwrap_err()
        );
    }

    /// Narrow widths that stress layout calculations.
    #[test]
    fn markdown_fuzz_narrow_widths(
        input in "[a-zA-Z0-9#*_`| \n\\-]{0,200}",
        width in 1usize..15
    ) {
        let result = safe_render_with_width(&input, width);
        prop_assert!(
            result.is_ok(),
            "Panicked at width {} on: {:?}\nPanic: {}",
            width,
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Zero width — an edge case that should not crash.
    #[test]
    fn markdown_fuzz_zero_width(input in "[a-zA-Z *#]{0,100}") {
        let result = safe_render_with_width(&input, 0);
        prop_assert!(
            result.is_ok(),
            "Panicked at width 0 on: {:?}\nPanic: {}",
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Unicode-heavy content including emoji, CJK, RTL, combining marks.
    #[test]
    fn markdown_fuzz_unicode_heavy(
        fragments in prop::collection::vec(
            prop_oneof![
                Just("🎉🎊🎈".to_string()),
                Just("👨‍👩‍👧‍👦".to_string()),
                Just("你好世界".to_string()),
                Just("مرحبا".to_string()),
                Just("한국어".to_string()),
                Just("café".to_string()),
                Just("\u{200B}\u{200B}".to_string()),  // zero-width space
                Just("\u{0300}\u{0301}\u{0302}".to_string()),  // combining marks
                Just("normal text".to_string()),
                Just("**bold emoji 🔥**".to_string()),
                Just("| 列1 | 列2 |\n|---|---|\n| 值 | 值 |\n".to_string()),
            ],
            1..10
        )
    ) {
        let input = fragments.join(" ");
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on unicode input: {:?}\nPanic: {}",
            &input[..input.len().min(200)],
            result.unwrap_err()
        );
    }

    /// Long lines without any whitespace — tests wrapping behavior.
    #[test]
    fn markdown_fuzz_long_lines_no_whitespace(len in 1000usize..15000) {
        let input = "a".repeat(len);
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on long line (len {})\nPanic: {}",
            len,
            result.unwrap_err()
        );
    }

    /// Repeated formatting toggling that can cause exponential parsing.
    #[test]
    fn markdown_fuzz_formatting_toggles(count in 10usize..100) {
        let input = "*_".repeat(count);
        let result = safe_render(&input);
        prop_assert!(
            result.is_ok(),
            "Panicked on formatting toggles (count {})\nPanic: {}",
            count,
            result.unwrap_err()
        );
    }
}
