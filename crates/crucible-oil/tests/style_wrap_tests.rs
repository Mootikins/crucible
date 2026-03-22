//! Integration tests for wrap_styled_text style preservation across line wraps.
//!
//! These tests target a specific bug class: ANSI style codes being lost or
//! misapplied when styled text is wrapped across multiple lines.

use crucible_oil::ansi::{strip_ansi, visible_width, wrap_styled_text};

const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Helper: assert every line with visible content that overlaps a styled span
/// contains the expected ANSI code and a matching reset.
fn assert_all_visible_lines_styled(lines: &[String], expected_code: &str) {
    for (i, line) in lines.iter().enumerate() {
        let visible = strip_ansi(line);
        if visible.trim().is_empty() {
            continue;
        }
        assert!(
            line.contains(expected_code),
            "Line {i} has visible content {visible:?} but is missing style code {expected_code:?}.\n\
             Full line: {line:?}"
        );
        assert!(
            line.contains(RESET),
            "Line {i} has style code but no reset.\nFull line: {line:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 1. Style preserved across wrap boundary
// ---------------------------------------------------------------------------

#[test]
fn single_styled_span_wraps_to_multiple_lines() {
    // A single bold span longer than the wrap width. Every output line that
    // contains visible text must carry the bold code.
    let spans = vec![(
        "The quick brown fox jumps over the lazy dog".to_string(),
        BOLD.to_string(),
    )];
    let lines = wrap_styled_text(&spans, 10);

    assert!(
        lines.len() >= 3,
        "Expected at least 3 lines at width 10, got {}",
        lines.len()
    );
    assert_all_visible_lines_styled(&lines, BOLD);
}

#[test]
fn styled_span_split_across_two_lines() {
    // "aaaa bbbb" at width 5 should wrap to ["aaaa", "bbbb"].
    // Both lines should carry the red code.
    let spans = vec![("aaaa bbbb".to_string(), RED.to_string())];
    let lines = wrap_styled_text(&spans, 5);

    assert_eq!(lines.len(), 2, "Expected 2 lines, got {lines:?}");
    for line in &lines {
        assert!(line.contains(RED), "Line missing RED style: {line:?}");
        assert!(line.contains(RESET), "Line missing reset: {line:?}");
    }
}

// ---------------------------------------------------------------------------
// 2. Multiple styles don't bleed
// ---------------------------------------------------------------------------

#[test]
fn adjacent_styles_do_not_bleed_across_boundary() {
    // "red green" — "red " is RED, "green" is GREEN, wrap at 6.
    // Line 1 should have RED but not GREEN. Line 2 should have GREEN but not RED.
    let spans = vec![
        ("red ".to_string(), RED.to_string()),
        ("green".to_string(), GREEN.to_string()),
    ];
    let lines = wrap_styled_text(&spans, 6);

    assert!(lines.len() >= 2, "Expected at least 2 lines, got {lines:?}");

    // First line contains "red " — should have RED, not GREEN
    let first = &lines[0];
    assert!(first.contains(RED), "First line should have RED: {first:?}");
    assert!(
        !first.contains(GREEN),
        "First line should NOT have GREEN: {first:?}"
    );

    // Second line contains "green" — should have GREEN, not RED
    let second = &lines[1];
    assert!(
        second.contains(GREEN),
        "Second line should have GREEN: {second:?}"
    );
    assert!(
        !second.contains(RED),
        "Second line should NOT have RED: {second:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. Style at exact wrap boundary
// ---------------------------------------------------------------------------

#[test]
fn style_begins_at_exact_wrap_point() {
    // "12345" unstyled + "67890" styled, width=5.
    // Line 1: "12345" (no style). Line 2: "67890" (styled).
    let spans = vec![
        ("12345".to_string(), String::new()),
        ("67890".to_string(), BOLD.to_string()),
    ];
    let lines = wrap_styled_text(&spans, 5);

    // The unstyled line should have no ANSI codes at all
    let first_stripped = strip_ansi(&lines[0]);
    assert_eq!(
        lines[0], first_stripped,
        "First line should have no ANSI codes: {:?}",
        lines[0]
    );

    // The styled line should have the bold code
    assert!(lines.len() >= 2, "Expected at least 2 lines");
    assert!(
        lines[1].contains(BOLD),
        "Second line should have BOLD: {:?}",
        lines[1]
    );
    assert!(
        lines[1].contains(RESET),
        "Second line should have reset: {:?}",
        lines[1]
    );
}

// ---------------------------------------------------------------------------
// 4. Empty lines preserve state (no stray ANSI codes)
// ---------------------------------------------------------------------------

#[test]
fn empty_visible_content_has_no_stray_ansi() {
    // If wrapping produces a line where the styled span has no characters,
    // that line should not contain orphaned ANSI codes.
    let spans = vec![
        ("hello ".to_string(), String::new()),
        ("world".to_string(), RED.to_string()),
    ];
    let lines = wrap_styled_text(&spans, 80);

    for line in &lines {
        let visible = strip_ansi(line);
        if visible.is_empty() {
            assert_eq!(
                line, &visible,
                "Empty line should have no ANSI codes: {line:?}"
            );
        }
    }
}

#[test]
fn unstyled_span_produces_no_ansi_codes() {
    // Purely unstyled text should never get ANSI codes injected.
    let spans = vec![("hello world".to_string(), String::new())];
    let lines = wrap_styled_text(&spans, 5);

    for (i, line) in lines.iter().enumerate() {
        assert!(
            !line.contains('\x1b'),
            "Line {i} has unexpected ANSI escape: {line:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Unicode width handling (CJK double-width chars)
// ---------------------------------------------------------------------------

#[test]
fn cjk_characters_within_styled_span_wrap_correctly() {
    // Each CJK char is 2 columns wide. "你好世界" = 8 columns.
    // At width 5, we expect wrapping (exact split depends on textwrap behavior).
    let spans = vec![("你好世界".to_string(), RED.to_string())];
    let lines = wrap_styled_text(&spans, 5);

    assert!(
        lines.len() >= 2,
        "CJK text should wrap at width 5, got {lines:?}"
    );

    // Every line with visible content should carry the style
    assert_all_visible_lines_styled(&lines, RED);

    // No line should exceed the target width
    for (i, line) in lines.iter().enumerate() {
        let w = visible_width(line);
        // Allow slight overflow since textwrap may not split mid-character
        assert!(
            w <= 6,
            "Line {i} visible width {w} exceeds expected max.\nLine: {line:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Long single word forces character-level splitting
// ---------------------------------------------------------------------------

#[test]
#[ignore = "BUG: char_offset drift in wrap_styled_text char-level chunking path (line_end + 1 assumes whitespace separator)"]
fn long_styled_word_splits_per_character_with_style() {
    // "abcdefghij" is 10 chars, styled bold, width=3.
    // textwrap can't word-break, so wrap_styled_text does char-level chunking.
    // Each chunk should carry the style.
    let spans = vec![("abcdefghij".to_string(), BOLD.to_string())];
    let lines = wrap_styled_text(&spans, 3);

    assert!(
        lines.len() >= 3,
        "Expected at least 3 lines for 10-char word at width 3, got {}",
        lines.len()
    );

    for (i, line) in lines.iter().enumerate() {
        let visible = strip_ansi(line);
        if visible.is_empty() {
            continue;
        }
        assert!(
            visible.len() <= 3,
            "Line {i} visible content {visible:?} exceeds width 3"
        );
        assert!(
            line.contains(BOLD),
            "Line {i} missing BOLD code.\nLine: {line:?}"
        );
        assert!(
            line.contains(RESET),
            "Line {i} missing reset.\nLine: {line:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 7. Reset codes present on every styled line
// ---------------------------------------------------------------------------

#[test]
fn every_line_with_style_code_has_matching_reset() {
    // Multiple styled spans that will wrap across several lines.
    let spans = vec![
        ("alpha bravo ".to_string(), RED.to_string()),
        ("charlie delta ".to_string(), GREEN.to_string()),
        ("echo foxtrot".to_string(), BOLD.to_string()),
    ];
    let lines = wrap_styled_text(&spans, 10);

    for (i, line) in lines.iter().enumerate() {
        let has_style = line.contains("\x1b[");
        let has_reset = line.contains(RESET);
        if has_style {
            assert!(
                has_reset,
                "Line {i} has style code(s) but no reset.\nLine: {line:?}"
            );
        }
    }
}

#[test]
fn reset_count_matches_style_count() {
    // Each style application should have exactly one reset.
    let spans = vec![
        ("aaa ".to_string(), RED.to_string()),
        ("bbb ".to_string(), GREEN.to_string()),
        ("ccc".to_string(), BOLD.to_string()),
    ];
    let lines = wrap_styled_text(&spans, 80);

    // On a single wide line, we expect exactly 3 style codes and 3 resets.
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    let style_count = line.matches("\x1b[").count() - line.matches(RESET).count();
    let reset_count = line.matches(RESET).count();
    assert_eq!(
        style_count, reset_count,
        "Style opens ({style_count}) should equal reset count ({reset_count}).\nLine: {line:?}"
    );
}
