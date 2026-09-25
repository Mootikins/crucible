//! Text selection over a buffer of rows: drag, double-click word,
//! triple-click line, and the text a copy gets.
//!
//! Points are buffer coordinates (a buffer row and a cell column), not
//! screen coordinates, so a selection stays on its text while the view
//! scrolls. The rows come from a closure, so the transcript and a plugin
//! buffer share this code.
//!
//! A selection covers only source text. Each row says where its text
//! starts ([`RowText`]); the cells before that are a gutter, and the spaces
//! after the last visible grapheme are padding. The highlight and the copy
//! both read the text through [`text_span`], so they cannot disagree.

use crucible_oil::cell_grid::{CellGrid, RowText};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// A row as a selection reads it: the styled text and where its source
/// text is.
#[derive(Debug, Clone, Copy)]
pub struct RowRef<'a> {
    pub ansi: &'a str,
    /// `None` for a row that a renderer did not describe: its text starts
    /// in the first column, and it starts a new line.
    pub text: Option<&'a RowText>,
}

impl<'a> RowRef<'a> {
    /// The source text between the row above and this row, when a wrap
    /// split one line over both.
    fn join(self) -> Option<&'a str> {
        self.text.and_then(|t| t.join.as_deref())
    }
}

/// A cell of the buffer. Ordered by row, then column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}

/// A run of cells from `start` up to `end`, with `end.col` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Point,
    pub end: Point,
}

/// What one press selects: a cell, a word, or a logical line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Cell,
    Word,
    Line,
}

impl Unit {
    /// One click selects cells, two a word, three a line.
    pub fn from_clicks(clicks: u8) -> Self {
        match clicks {
            0 | 1 => Self::Cell,
            2 => Self::Word,
            _ => Self::Line,
        }
    }
}

/// A selection: the span under the press and the span under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: Span,
    head: Span,
    unit: Unit,
}

impl Selection {
    /// Start a selection with a press at `point`.
    pub fn start<'a>(
        point: Point,
        unit: Unit,
        width: usize,
        rows: impl Fn(usize) -> Option<RowRef<'a>>,
    ) -> Self {
        let span = unit_span(point, unit, width, &rows);
        Self {
            anchor: span,
            head: span,
            unit,
        }
    }

    /// Move the pointer end to `point`. A word or line selection grows by
    /// whole words or lines.
    pub fn extend<'a>(
        &mut self,
        point: Point,
        width: usize,
        rows: impl Fn(usize) -> Option<RowRef<'a>>,
    ) {
        self.head = unit_span(point, self.unit, width, &rows);
    }

    pub fn unit(&self) -> Unit {
        self.unit
    }

    /// Move each end to the row that `f` gives, as when a layout changed
    /// the height of rows above it.
    pub fn map_rows(&mut self, mut f: impl FnMut(usize) -> usize) {
        for point in [
            &mut self.anchor.start,
            &mut self.anchor.end,
            &mut self.head.start,
            &mut self.head.end,
        ] {
            point.row = f(point.row);
        }
    }

    /// The span between the press and the pointer, in buffer order. It can
    /// start or end in a gutter; [`text_span`] gives the text in it.
    pub fn bounds(&self) -> Span {
        Span {
            start: self.anchor.start.min(self.head.start),
            end: self.anchor.end.max(self.head.end),
        }
    }
}

/// Row `row` parsed into cells, and the columns of its source text.
fn text_row(row: Option<RowRef<'_>>, width: usize) -> (CellGrid, Range<usize>) {
    let grid = row_grid(row, width);
    let start = row.and_then(|r| r.text).map_or(0, |t| t.start).min(width);
    let end = grid.text_end(0).max(start);
    (grid, start..end)
}

/// The source text in `span`: an end in a gutter, in padding or on a row
/// without text moves to the nearest text inside the span. The start moves
/// forward and the end moves back. `None` when the span holds no text.
pub fn text_span<'a>(
    span: Span,
    width: usize,
    rows: impl Fn(usize) -> Option<RowRef<'a>>,
) -> Option<Span> {
    let start = (span.start.row..=span.end.row).find_map(|r| {
        let (_, text) = text_row(rows(r), width);
        let from = if r == span.start.row {
            span.start.col.max(text.start)
        } else {
            text.start
        };
        let to = if r == span.end.row {
            span.end.col.min(text.end)
        } else {
            text.end
        };
        (from < to).then_some(Point { row: r, col: from })
    })?;
    let end = (start.row..=span.end.row).rev().find_map(|r| {
        let (_, text) = text_row(rows(r), width);
        let from = if r == start.row {
            start.col
        } else {
            text.start
        };
        let to = if r == span.end.row {
            span.end.col.min(text.end)
        } else {
            text.end
        };
        (from < to).then_some(Point { row: r, col: to })
    })?;
    Some(Span { start, end })
}

/// The columns of `row` that the highlight inverts: the source text of the
/// row inside `span`, which must come from [`text_span`].
pub fn highlight_cols<'a>(
    span: Span,
    row: usize,
    width: usize,
    rows: impl Fn(usize) -> Option<RowRef<'a>>,
) -> Option<Range<usize>> {
    if row < span.start.row || row > span.end.row {
        return None;
    }
    let (_, text) = text_row(rows(row), width);
    let from = if row == span.start.row {
        span.start.col
    } else {
        text.start
    };
    let to = if row == span.end.row {
        span.end.col
    } else {
        text.end
    };
    (from < to).then_some(from..to)
}

/// The span of `unit` at `point`.
fn unit_span<'a>(
    point: Point,
    unit: Unit,
    width: usize,
    rows: &impl Fn(usize) -> Option<RowRef<'a>>,
) -> Span {
    let grid = row_grid(rows(point.row), width);
    let cols = match unit {
        Unit::Cell => grid.grapheme_span(0, point.col),
        Unit::Word => word_cols(&grid, point.col),
        Unit::Line => return line_span(point.row, width, rows),
    };
    Span {
        start: Point {
            row: point.row,
            col: cols.start,
        },
        end: Point {
            row: point.row,
            col: cols.end,
        },
    }
}

fn row_grid(row: Option<RowRef<'_>>, width: usize) -> CellGrid {
    CellGrid::from_line(row.map_or("", |r| r.ansi), width)
}

/// The columns of the word at `col`, by Unicode word boundaries. A run of
/// spaces or a punctuation mark is a word of its own.
fn word_cols(grid: &CellGrid, col: usize) -> Range<usize> {
    // Each grapheme's text and the columns it covers.
    let mut text = String::new();
    let mut spans: Vec<(usize, Range<usize>)> = Vec::new();
    let mut x = 0;
    while x < grid.width() {
        let span = grid.grapheme_span(0, x);
        spans.push((text.len(), span.clone()));
        text.push_str(&grid.text(0, span.start..span.start + 1));
        x = span.end.max(x + 1);
    }
    let Some(byte) = spans
        .iter()
        .find(|(_, span)| span.contains(&col))
        .map(|(byte, _)| *byte)
    else {
        return col..col;
    };
    let Some((start, word)) = text
        .split_word_bound_indices()
        .find(|(start, word)| (*start..start + word.len()).contains(&byte))
    else {
        return col..col;
    };
    let end = start + word.len();
    let first = spans
        .iter()
        .find(|(b, _)| *b >= start)
        .map(|(_, s)| s.start);
    let last = spans
        .iter()
        .rev()
        .find(|(b, _)| *b < end)
        .map(|(_, s)| s.end);
    match (first, last) {
        (Some(first), Some(last)) => first..last,
        _ => col..col,
    }
}

/// The rows of the logical line that holds `row`: up while a row continues
/// the one above it, down while the next row continues this one.
fn line_span<'a>(row: usize, width: usize, rows: &impl Fn(usize) -> Option<RowRef<'a>>) -> Span {
    let continues = |r: usize| rows(r).and_then(|row| row.join()).is_some();
    let mut first = row;
    while first > 0 && continues(first) {
        first -= 1;
    }
    let mut last = row;
    while continues(last + 1) {
        last += 1;
    }
    Span {
        start: Point { row: first, col: 0 },
        end: Point {
            row: last,
            col: width,
        },
    }
}

/// The source text in `span` (see [`text_span`]), as the source had it.
///
/// A row that a wrap split from the row above joins it with the text the
/// wrap dropped. Every other row starts a new line. A gutter and the
/// padding at the end of a row are not text.
pub fn selected_text<'a>(
    span: Span,
    width: usize,
    rows: impl Fn(usize) -> Option<RowRef<'a>>,
) -> String {
    let mut out = String::new();
    let Some(span) = text_span(span, width, &rows) else {
        return out;
    };
    for r in span.start.row..=span.end.row {
        let row = rows(r);
        if r > span.start.row {
            match row.and_then(|row| row.join()) {
                Some(gap) => out.push_str(gap),
                None => out.push('\n'),
            }
        }
        if let Some(cols) = highlight_cols(span, r, width, &rows) {
            out.push_str(&row_grid(row, width).text(0, cols));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer of rows. A row with `Some((gap, start))` continues the row
    /// above with `gap`, and its text starts at column `start`.
    struct Rows {
        rows: Vec<(String, RowText)>,
    }

    impl Rows {
        fn new(spec: &[(&str, Option<(&str, usize)>)]) -> Self {
            Self {
                rows: spec
                    .iter()
                    .map(|(text, join)| {
                        (
                            text.to_string(),
                            join.map_or_else(RowText::default, |(gap, start)| RowText {
                                start,
                                join: Some(gap.to_string()),
                            }),
                        )
                    })
                    .collect(),
            }
        }

        fn get(&self, r: usize) -> Option<RowRef<'_>> {
            self.rows.get(r).map(|(ansi, text)| RowRef {
                ansi,
                text: Some(text),
            })
        }
    }

    fn span(r0: usize, c0: usize, r1: usize, c1: usize) -> Span {
        Span {
            start: Point { row: r0, col: c0 },
            end: Point { row: r1, col: c1 },
        }
    }

    #[test]
    fn a_copy_across_a_wrap_joins_the_rows_with_the_dropped_space() {
        let rows = Rows::new(&[
            ("  The quick brown", None),
            ("  fox jumps over", Some((" ", 2))),
            ("  the lazy dog.", Some((" ", 2))),
        ]);
        let text = selected_text(span(0, 2, 2, 20), 20, |r| rows.get(r));
        assert_eq!(text, "The quick brown fox jumps over the lazy dog.");
    }

    #[test]
    fn a_copy_keeps_hard_line_breaks() {
        let rows = Rows::new(&[("first line", None), ("second line", None)]);
        let text = selected_text(span(0, 0, 1, 20), 20, |r| rows.get(r));
        assert_eq!(text, "first line\nsecond line");
    }

    #[test]
    fn a_break_inside_a_long_word_joins_without_a_space() {
        let rows = Rows::new(&[("abcdefghij", None), ("klmno", Some(("", 0)))]);
        let text = selected_text(span(0, 0, 1, 10), 10, |r| rows.get(r));
        assert_eq!(text, "abcdefghijklmno");
    }

    #[test]
    fn a_wide_character_copies_once_whole() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let line = format!("日本 {family} end");
        let rows = Rows::new(&[(line.as_str(), None)]);
        // Columns 1..6 start inside 日 and end inside the emoji.
        let text = selected_text(span(0, 1, 0, 6), 20, |r| rows.get(r));
        assert_eq!(text, format!("日本 {family}"));
        let whole = selected_text(span(0, 0, 0, 20), 20, |r| rows.get(r));
        assert_eq!(whole, line);
    }

    #[test]
    fn a_wrap_between_wide_characters_joins_them_exactly() {
        let rows = Rows::new(&[("日本語の", None), ("テキスト", Some(("", 0)))]);
        let text = selected_text(span(0, 0, 1, 8), 8, |r| rows.get(r));
        assert_eq!(text, "日本語のテキスト");
    }

    #[test]
    fn styled_rows_copy_as_plain_text() {
        let rows = Rows::new(&[("\x1b[1mbold\x1b[0m and \x1b[7mplain\x1b[0m", None)]);
        let text = selected_text(span(0, 0, 0, 30), 30, |r| rows.get(r));
        assert_eq!(text, "bold and plain");
    }

    #[test]
    fn a_double_click_selects_the_word_under_the_pointer() {
        let rows = Rows::new(&[("  hello brave world", None)]);
        let sel = Selection::start(Point { row: 0, col: 10 }, Unit::Word, 30, |r| rows.get(r));
        assert_eq!(selected_text(sel.bounds(), 30, |r| rows.get(r)), "brave");
    }

    #[test]
    fn a_double_click_on_cjk_selects_the_cjk_run() {
        let rows = Rows::new(&[("see 日本語 here", None)]);
        let sel = Selection::start(Point { row: 0, col: 6 }, Unit::Word, 30, |r| rows.get(r));
        let text = selected_text(sel.bounds(), 30, |r| rows.get(r));
        assert!(text.starts_with('日') || text == "本", "{text:?}");
        assert!(!text.contains(' '), "{text:?}");
    }

    #[test]
    fn a_word_drag_grows_by_whole_words() {
        let rows = Rows::new(&[("one two three four", None)]);
        let mut sel = Selection::start(Point { row: 0, col: 5 }, Unit::Word, 30, |r| rows.get(r));
        sel.extend(Point { row: 0, col: 9 }, 30, |r| rows.get(r));
        assert_eq!(
            selected_text(sel.bounds(), 30, |r| rows.get(r)),
            "two three"
        );
    }

    #[test]
    fn a_triple_click_selects_the_whole_wrapped_line() {
        let rows = Rows::new(&[
            ("before", None),
            ("  The quick brown", None),
            ("  fox jumps", Some((" ", 2))),
            ("after", None),
        ]);
        let sel = Selection::start(Point { row: 2, col: 4 }, Unit::Line, 20, |r| rows.get(r));
        assert_eq!(
            selected_text(sel.bounds(), 20, |r| rows.get(r)),
            "  The quick brown fox jumps"
        );
    }

    #[test]
    fn a_backward_drag_selects_the_same_text() {
        let rows = Rows::new(&[("abc def", None), ("ghi", None)]);
        let mut sel = Selection::start(Point { row: 1, col: 2 }, Unit::Cell, 10, |r| rows.get(r));
        sel.extend(Point { row: 0, col: 4 }, 10, |r| rows.get(r));
        assert_eq!(selected_text(sel.bounds(), 10, |r| rows.get(r)), "def\nghi");
    }

    #[test]
    fn the_highlight_covers_only_the_selected_text_of_each_row() {
        let rows = Rows::new(&[("abc", None), ("def", None), ("ghi", None)]);
        let mut sel = Selection::start(Point { row: 0, col: 1 }, Unit::Cell, 10, |r| rows.get(r));
        sel.extend(Point { row: 2, col: 1 }, 10, |r| rows.get(r));
        let span = text_span(sel.bounds(), 10, |r| rows.get(r)).unwrap();
        let cols = |r| highlight_cols(span, r, 10, |r| rows.get(r));
        assert_eq!(cols(0), Some(1..3));
        assert_eq!(cols(1), Some(0..3));
        assert_eq!(cols(2), Some(0..2));
        assert_eq!(cols(3), None);
    }

    #[test]
    fn ends_in_a_gutter_snap_to_the_nearest_text() {
        let gutter = |start| RowText { start, join: None };
        let texts = [gutter(3), RowText::default(), gutter(3)];
        let ansi = [" * alpha", "", "   gamma"];
        let rows = |r: usize| {
            texts.get(r).map(|text| RowRef {
                ansi: ansi[r],
                text: Some(text),
            })
        };
        // From the bullet of row 0 to the gutter of row 2.
        let span = span(0, 1, 2, 2);
        assert_eq!(text_span(span, 10, rows), Some(self::span(0, 3, 0, 8)));
        assert_eq!(selected_text(span, 10, rows), "alpha");
        // A span in a gutter or on a blank row holds no text.
        assert_eq!(text_span(self::span(0, 0, 0, 3), 10, rows), None);
        assert_eq!(text_span(self::span(1, 0, 2, 3), 10, rows), None);
    }
}
