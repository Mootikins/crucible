//! Row-layout sizing contract tests.
//!
//! The `Size` doc-comment promises two-column layouts: a Content-sized child
//! shrinks to its natural width and a Flex sibling fills the remaining space.
//! These tests pin that contract, plus graceful degradation when a row's
//! content overflows the available width (children shrink and the renderer
//! ellipsizes instead of pushing siblings off-grid).

use crucible_oil::node::{col, fixed, flex, row, text};
use crucible_oil::render_to_plain_text;

/// A plain text sibling of a fixed-height box must still get row space.
#[test]
fn row_fixed_box_does_not_claim_full_width() {
    let node = row([fixed(1, text("Label:")), text("content after label")]);
    let plain = render_to_plain_text(&node, 80);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        first_line.contains("Label:") && first_line.contains("content after label"),
        "both columns should render on one line, got: {plain:?}"
    );
}

/// The doc-comment example on `Size`: Content col + flex(1) col in a row.
#[test]
fn row_flex_child_renders_its_content() {
    let node = row([
        col([text("Label:")]),
        flex(1, col([text("description fills the remaining space")])),
    ]);
    let plain = render_to_plain_text(&node, 80);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        first_line.contains("Label:")
            && first_line.contains("description fills the remaining space"),
        "flex child content must render next to the label, got: {plain:?}"
    );
}

/// When row content exceeds the width, children shrink and the renderer
/// ellipsizes — nothing is silently dropped and nothing lands off-grid.
#[test]
fn row_overflow_shrinks_and_ellipsizes() {
    let node = row([text("aaaaaaaaaaaaaaaaaaaa"), text("bbbbbbbbbbbbbbbbbbbb")]);
    let plain = render_to_plain_text(&node, 20);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        first_line.chars().count() <= 20,
        "row must not exceed the available width, got: {first_line:?}"
    );
    assert!(
        first_line.contains('a') && first_line.contains('b'),
        "both children must remain visible when the row overflows, got: {plain:?}"
    );
    assert!(
        first_line.contains('…'),
        "shrunk text should signal truncation with an ellipsis, got: {plain:?}"
    );
    assert_eq!(
        plain.lines().count(),
        1,
        "shrunk single-line text must not bleed onto extra rows, got: {plain:?}"
    );
}

/// A `no_shrink` text span (badges, decorations) keeps its full width when
/// the row overflows — shrinkable siblings give up the space instead.
#[test]
fn no_shrink_span_keeps_width_when_row_overflows() {
    let node = row([
        text(" BADGE ").no_shrink(),
        text("a long shrinkable span of content"),
    ]);
    let plain = render_to_plain_text(&node, 20);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with(" BADGE "),
        "no_shrink span must keep its full width, got: {plain:?}"
    );
    assert!(
        first_line.contains('…'),
        "the shrinkable sibling absorbs the overflow, got: {plain:?}"
    );
}

/// Shrink truncation must be terminal-column aware, not char-count aware:
/// double-width glyphs (CJK) count two columns when ellipsizing.
#[test]
fn row_shrink_is_width_aware_for_wide_glyphs() {
    let node = row([text("日本語のテキストです"), text("second column here")]);
    let plain = render_to_plain_text(&node, 20);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        crucible_oil::ansi::visible_width(first_line) <= 20,
        "wide glyphs must not push the row past 20 columns: {first_line:?}"
    );
    assert!(
        first_line.contains("日本語") && first_line.contains("second"),
        "both children stay visible under width-aware shrink: {plain:?}"
    );
    assert!(first_line.contains('…'));
}

/// Documents the degenerate case: a no_shrink span wider than the whole row
/// wins over its siblings and hard-clips at the grid edge. no_shrink is for
/// SHORT decorations — oversized rigid spans are the caller's bug, and the
/// engine guarantees only that nothing renders past the grid.
#[test]
fn no_shrink_wider_than_row_clips_at_grid_edge() {
    let node = row([text(" A VERY LONG RIGID BADGE ").no_shrink(), text("x")]);
    let plain = render_to_plain_text(&node, 10);
    let first_line = plain.lines().next().unwrap_or("");
    assert_eq!(
        first_line, " A VERY LO",
        "rigid overflow clips at the grid edge; the shrinkable sibling yields fully"
    );
}

/// Column gaps survive shrink: the gap is layout, not content, so it is never
/// absorbed by overflowing children.
#[test]
fn row_gap_preserved_under_shrink() {
    use crucible_oil::style::Gap;
    let node = row([text("aaaaaaaaaa"), text("bbbbbbbbbb")]).gap(Gap::column(2));
    let plain = render_to_plain_text(&node, 15);
    assert_eq!(
        plain.lines().next().unwrap_or(""),
        "aaaaaa…  bbbbb…",
        "2-cell gap intact, both sides ellipsized to fit 15"
    );
}

/// A row that fits keeps its exact content — shrink support must not disturb
/// the non-overflowing case.
#[test]
fn row_that_fits_renders_verbatim() {
    let node = row([text(" ok "), text("Read File → [208 lines]")]);
    let plain = render_to_plain_text(&node, 80);
    let first_line = plain.lines().next().unwrap_or("");
    assert!(
        first_line.contains(" ok ") && first_line.contains("Read File → [208 lines]"),
        "fitting rows render verbatim, got: {plain:?}"
    );
    assert!(!first_line.contains('…'));
}
