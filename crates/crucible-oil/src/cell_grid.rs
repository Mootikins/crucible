use std::ops::Range;
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::ansi::extract_bg;

/// One terminal cell: a grapheme and the SGR escapes that style it.
#[derive(Debug, Clone, Default)]
pub struct StyledCell {
    /// The first code point of the grapheme. `'\0'` marks a cell that the
    /// wide grapheme to its left covers.
    pub ch: char,
    /// The rest of the grapheme when it has more than one code point: a ZWJ
    /// sequence, a combining mark, a variation selector. `None` for most
    /// cells, so a plain cell allocates nothing.
    pub tail: Option<Box<str>>,
    pub style: String,
}

impl StyledCell {
    pub fn space() -> Self {
        Self {
            ch: ' ',
            tail: None,
            style: String::new(),
        }
    }

    pub fn new(ch: char, style: String) -> Self {
        Self {
            ch,
            tail: None,
            style,
        }
    }

    /// A transparent cell lets the base layer show through when an overlay composites.
    pub fn is_transparent(&self) -> bool {
        self.ch == ' ' && self.tail.is_none() && self.style.is_empty()
    }

    /// Whether the wide grapheme to the left covers this cell.
    pub fn is_continuation(&self) -> bool {
        self.ch == '\0'
    }

    fn push_grapheme(&self, out: &mut String) {
        if self.ch != '\0' {
            out.push(self.ch);
            if let Some(tail) = &self.tail {
                out.push_str(tail);
            }
        }
    }
}

/// How a row continues the logical line of the row above it.
///
/// A wrap splits one source line over rows and drops the text at the break,
/// usually one space. A copy that crosses the break puts that text back
/// instead of a line break. A row without a join starts a new line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowJoin {
    /// The source text between the row above and this row: `" "` for a
    /// break at a space, `""` for a break inside a long word.
    pub gap: String,
    /// The first column of this row's own text. The cells before it are an
    /// indent or a prefix that the wrap added, so a copy skips them.
    pub content_col: usize,
}

/// One row of the grid.
///
/// A long transcript has thousands of rows, and most of them are either
/// copied whole from an earlier frame or never drawn into. So a row gets its
/// cells only when something draws into it.
#[derive(Debug, Clone, Default)]
struct Row {
    /// Empty until something draws into the row. An empty row reads as spaces.
    cells: Vec<StyledCell>,
    /// A finished row that [`CellGrid::put_row`] placed, as `rows[index]`.
    /// It is the row's output as long as nothing draws over it.
    verbatim: Option<(Arc<[String]>, usize)>,
    /// How the row continues the row above. See [`RowJoin`].
    join: Option<RowJoin>,
}

#[derive(Debug, Clone)]
pub struct CellGrid {
    rows: Vec<Row>,
    width: usize,
    height: usize,
}

impl CellGrid {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            rows: vec![Row::default(); height],
            width,
            height,
        }
    }

    /// A one-row grid holding `line`, for reading the cells of a stored row.
    pub fn from_line(line: &str, width: usize) -> Self {
        let mut grid = Self::new(width, 1);
        grid.blit_line(line, 0, 0);
        grid
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    #[cfg(test)]
    pub fn get(&self, x: usize, y: usize) -> Option<&StyledCell> {
        static SPACE: StyledCell = StyledCell {
            ch: ' ',
            tail: None,
            style: String::new(),
        };
        let row = self.rows.get(y)?;
        if x >= self.width {
            return None;
        }
        Some(row.cells.get(x).unwrap_or(&SPACE))
    }

    /// The cells of row `y`, made on the first draw. A verbatim row is drawn
    /// into its cells first, so a later draw composes over it.
    fn cells_mut(&mut self, y: usize) -> &mut [StyledCell] {
        let width = self.width;
        let row = &mut self.rows[y];
        if row.cells.is_empty() {
            row.cells = vec![StyledCell::space(); width];
            if let Some((rows, index)) = row.verbatim.take() {
                blit_into(&mut row.cells, &rows[index], 0);
            }
        }
        &mut row.cells
    }

    /// Draw every verbatim row into its cells, so that [`CellGrid::row`]
    /// reads its content. The full-screen mode reads cells; the native mode
    /// copies verbatim rows as they are.
    pub fn draw_verbatim_rows(&mut self) {
        for y in 0..self.height {
            if self.rows[y].verbatim.is_some() {
                self.cells_mut(y);
            }
        }
    }

    /// How row `y` continues the row above, if a wrap split them.
    pub fn join(&self, y: usize) -> Option<&RowJoin> {
        self.rows.get(y).and_then(|row| row.join.as_ref())
    }

    /// Mark row `y` as the continuation of the row above.
    pub fn set_join(&mut self, y: usize, join: RowJoin) {
        if let Some(row) = self.rows.get_mut(y) {
            row.join = Some(join);
        }
    }

    /// Invert the cells of row `y` in `cols`, as a selection highlight. A
    /// range that starts inside a wide grapheme widens to its first cell.
    pub fn invert(&mut self, y: usize, cols: Range<usize>) {
        if y >= self.height {
            return;
        }
        let row = self.cells_mut(y);
        let mut start = cols.start.min(row.len());
        while start > 0 && row[start].is_continuation() {
            start -= 1;
        }
        let end = cols.end.min(row.len());
        for cell in &mut row[start..end] {
            cell.style.push_str("\x1b[7m");
        }
    }

    /// The text of row `y` in `cols`, one grapheme per covered cell. A
    /// wide grapheme counts when its first cell is in the range, or when the
    /// range starts inside it.
    pub fn text(&self, y: usize, cols: Range<usize>) -> String {
        let row = self.row(y);
        let mut start = cols.start.min(row.len());
        while start > 0 && start < row.len() && row[start].is_continuation() {
            start -= 1;
        }
        let mut out = String::new();
        for cell in &row[start..cols.end.min(row.len())] {
            cell.push_grapheme(&mut out);
        }
        out
    }

    /// The column span of the grapheme at `col`: its first cell and its
    /// width. A continuation cell answers for the grapheme that covers it.
    pub fn grapheme_span(&self, y: usize, col: usize) -> Range<usize> {
        let row = self.row(y);
        if row.is_empty() {
            return col..col;
        }
        let mut start = col.min(row.len() - 1);
        while start > 0 && row[start].is_continuation() {
            start -= 1;
        }
        let mut end = start + 1;
        while end < row.len() && row[end].is_continuation() {
            end += 1;
        }
        start..end
    }

    pub fn blit_line(&mut self, line: &str, x: usize, y: usize) {
        if y >= self.height || x >= self.width {
            return;
        }
        blit_into(self.cells_mut(y), line, x);
    }

    /// Place `rows[index]` as row `y`: a finished row as
    /// [`CellGrid::to_string_compact`] emits it, rendered earlier at the
    /// width of this grid.
    ///
    /// A row that nothing drew into keeps the string as its output, so a
    /// frame copies it instead of drawing its cells again. Otherwise the
    /// string is drawn over the cells, as [`CellGrid::blit_line`] does.
    pub fn put_row(&mut self, rows: &Arc<[String]>, index: usize, x: usize, y: usize) {
        let Some(row) = self.rows.get_mut(y) else {
            return;
        };
        if x == 0 && row.cells.is_empty() && row.verbatim.is_none() {
            row.verbatim = Some((Arc::clone(rows), index));
        } else {
            self.blit_line(&rows[index], x, y);
        }
    }

    #[cfg(test)]
    pub fn blit_string(&mut self, content: &str, x: usize, y: usize) {
        for (row_idx, line) in content.lines().enumerate() {
            let target_y = y + row_idx;
            if target_y < self.height {
                self.blit_line(line, x, target_y);
            }
        }
    }

    #[cfg(test)]
    pub fn to_lines(&self) -> Vec<String> {
        (0..self.height)
            .map(|y| {
                let row = &self.rows[y];
                match &row.verbatim {
                    Some((rows, index)) => rows[*index].clone(),
                    None if row.cells.is_empty() => " ".repeat(self.width),
                    None => cells_to_string(&row.cells),
                }
            })
            .collect()
    }

    /// Find the last row with non-space (or styled) content, returning count of content rows.
    ///
    /// Returns 0 for an entirely blank grid.
    pub fn content_height(&self) -> usize {
        self.rows
            .iter()
            .rposition(|row| match &row.verbatim {
                Some((rows, index)) => !rows[*index].is_empty(),
                None => row.cells.iter().any(|c| c.ch != ' ' || !c.style.is_empty()),
            })
            .map(|i| i + 1)
            .unwrap_or(0)
    }

    /// Render to compact string with trailing padding stripped per line.
    /// Styled cells (non-empty `style`) preserved even if their glyph is space.
    ///
    /// Callers counting rendered lines must use `str::lines()`, not
    /// `split("\r\n").count()` — trailing empty rows produce no terminator,
    /// so `lines()` drops them but `split` would yield one extra entry. The
    /// unified cursor-info `row_from_end` math (`tree_render.rs`) relies on
    /// this and would silently desync if a caller picked the wrong API.
    pub fn to_string_compact(&self) -> String {
        let mut out = String::new();
        for (y, row) in self.rows.iter().enumerate() {
            if y > 0 {
                out.push_str("\r\n");
            }
            push_row_compact(row, &mut out);
        }
        out
    }

    /// Each row as [`CellGrid::to_string_compact`] emits it, without the
    /// line breaks between them.
    pub fn rows_compact(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| {
                let mut out = String::new();
                push_row_compact(row, &mut out);
                out
            })
            .collect()
    }

    /// The cells of row `y`: empty outside the grid, and empty for a row
    /// that nothing drew into, which reads as spaces. A verbatim row has
    /// cells only after [`CellGrid::draw_verbatim_rows`].
    pub fn row(&self, y: usize) -> &[StyledCell] {
        self.rows.get(y).map_or(&[], |row| row.cells.as_slice())
    }

    /// Row `y` as the bytes that paint it over any old content in place.
    ///
    /// Trailing unstyled padding is dropped. When the row ends before the
    /// last column, an erase to the end of the line follows, so a longer old
    /// row cannot leave a tail. A full row gets no erase: the cursor waits in
    /// the last column, and an erase there would delete the last cell.
    pub fn row_ansi(&self, y: usize) -> String {
        let row = self.row(y);
        let content_end = row
            .iter()
            .rposition(|c| c.ch != ' ' || !c.style.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut out = cells_to_string(&row[..content_end]);
        if content_end < self.width {
            out.push_str("\x1b[K");
        }
        out
    }

    /// Copy row `src_y` of `src` into row `dst_y`. Cells past either width
    /// are dropped or stay blank.
    pub fn copy_row_from(&mut self, dst_y: usize, src: &CellGrid, src_y: usize) {
        if dst_y >= self.height {
            return;
        }
        let src_row = src.row(src_y);
        let dst = self.cells_mut(dst_y);
        for (x, cell) in dst.iter_mut().enumerate() {
            *cell = src_row.get(x).cloned().unwrap_or_else(StyledCell::space);
        }
    }

    /// Draw row `src_y` of `src` over row `dst_y`, keeping the old cell
    /// wherever the source cell is transparent. This is how an overlay lands
    /// on the full-screen frame.
    pub fn overlay_row_from(&mut self, dst_y: usize, src: &CellGrid, src_y: usize) {
        let src_row = src.row(src_y);
        if dst_y >= self.height || src_row.is_empty() {
            return;
        }
        let dst = self.cells_mut(dst_y);
        for (x, cell) in src_row.iter().enumerate() {
            if x < dst.len() && !cell.is_transparent() {
                dst[x] = cell.clone();
            }
        }
    }
}

fn push_row_compact(row: &Row, out: &mut String) {
    match &row.verbatim {
        Some((rows, index)) => out.push_str(&rows[*index]),
        None => out.push_str(&cells_to_string_compact(&row.cells)),
    }
}

/// Draw `line` into `cells` from column `x`, reading SGR escapes as the
/// style of the cells after them.
fn blit_into(cells: &mut [StyledCell], line: &str, x: usize) {
    let mut col = x;
    let mut current_style = String::new();
    let mut rest = line;
    while !rest.is_empty() && col < cells.len() {
        match rest.find('\x1b') {
            Some(0) => rest = consume_escape(rest, &mut current_style),
            Some(esc) => {
                col = blit_text(cells, &rest[..esc], x, col, &current_style);
                rest = &rest[esc..];
            }
            None => {
                blit_text(cells, rest, x, col, &current_style);
                break;
            }
        }
    }
}

/// Write the graphemes of `text` into `cells` from column `col`, and
/// return the column after them. `x` is where the blit started, so a
/// zero-width grapheme never attaches to a cell left of it.
fn blit_text(cells: &mut [StyledCell], text: &str, x: usize, mut col: usize, style: &str) -> usize {
    for grapheme in text.graphemes(true) {
        if col >= cells.len() {
            break;
        }
        let mut chars = grapheme.chars();
        let first = chars.next().unwrap_or(' ');
        let tail = chars.as_str();
        // A control character keeps the old per-character rule: one
        // cell each. CR LF is one grapheme, and a zero-width answer for
        // it would move every cell after it.
        if tail.is_empty() || first.is_control() {
            for c in grapheme.chars() {
                let width = UnicodeWidthChar::width(c).unwrap_or(1);
                col = put_grapheme(cells, c, None, width, x, col, style);
            }
        } else {
            let width = UnicodeWidthStr::width(grapheme);
            col = put_grapheme(cells, first, Some(tail), width, x, col, style);
        }
    }
    col
}

/// Write one grapheme at `col` and return the next column. A grapheme
/// that does not fit is dropped; a zero-width one joins the cell to its
/// left.
#[allow(clippy::too_many_arguments)]
fn put_grapheme(
    cells: &mut [StyledCell],
    first: char,
    tail: Option<&str>,
    width: usize,
    x: usize,
    col: usize,
    current_style: &str,
) -> usize {
    if width == 0 {
        if col > x {
            let mut lead = col - 1;
            while lead > x && cells[lead].is_continuation() {
                lead -= 1;
            }
            let cell = &mut cells[lead];
            let mut joined = cell.tail.take().map(String::from).unwrap_or_default();
            joined.push(first);
            joined.push_str(tail.unwrap_or(""));
            cell.tail = Some(joined.into_boxed_str());
        }
        return col;
    }
    if col + width > cells.len() {
        return col;
    }
    // Style composition: if the new write doesn't set its own bg,
    // inherit whatever bg was on the cell already. This lets a parent
    // Box's `style.bg` survive children that only paint fg, mirroring
    // CSS layering. Pair with `tree_render::render_box_content`'s
    // bg-fill.
    //
    // Asymmetric guarantee: this composes by *cell state*, not by tree
    // ancestry. If a sibling Box-with-bg paints a region, then a *later*
    // sibling (no bg) writes text over the same cells, the second
    // sibling's text picks up the first sibling's bg. Tree layouts that
    // don't overlap siblings (Crucible's norm) see only the intended
    // parent→child inheritance.
    let final_style = if extract_bg(current_style).is_none() {
        match extract_bg(&cells[col].style) {
            Some(prior_bg) => {
                if current_style.is_empty() {
                    prior_bg
                } else {
                    format!("{}{}", prior_bg, current_style)
                }
            }
            None => current_style.to_string(),
        }
    } else {
        current_style.to_string()
    };
    cells[col] = StyledCell {
        ch: first,
        tail: tail.map(Box::from),
        style: final_style,
    };
    for i in 1..width {
        cells[col + i] = StyledCell::new('\0', String::new());
    }
    col + width
}

/// Consume the escape sequence at the start of `rest` and return what
/// follows it. An SGR sequence updates `current_style`; every other escape
/// is dropped.
fn consume_escape<'a>(rest: &'a str, current_style: &mut String) -> &'a str {
    let mut chars = rest.char_indices().skip(1).peekable();
    match chars.peek().map(|&(_, c)| c) {
        Some('[') => {
            chars.next();
            let end = chars
                .find(|&(_, c)| c.is_ascii_alphabetic())
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(rest.len());
            let escape = &rest[..end];
            if escape.contains('m') {
                if escape == "\x1b[0m" || escape == "\x1b[m" {
                    current_style.clear();
                } else {
                    current_style.push_str(escape);
                }
            }
            &rest[end..]
        }
        // OSC / APC / DCS: skip entirely without interpreting as visible chars.
        // Bounded to 256 characters — a malformed unterminated sequence in
        // user content must not consume the rest of the line.
        //
        // NOTE: the parallel skip in `ansi::strip_ansi` / `visible_width`
        // is unbounded today (see `ansi.rs::skip_until_st_or_bel`). For
        // legitimate well-terminated escapes the two agree by reaching the
        // terminator first; for malformed input this asymmetry would only
        // matter if width and blit ran on the same malformed payload, which
        // no current path does.
        Some(']') | Some('_') | Some('P') => {
            chars.next();
            let mut consumed = 0usize;
            while let Some((i, c)) = chars.next() {
                consumed += 1;
                if c == '\x07' {
                    return &rest[i + 1..];
                }
                if c == '\x1b' && chars.peek().map(|&(_, c)| c) == Some('\\') {
                    return &rest[i + 2..];
                }
                if consumed >= 256 {
                    tracing::debug!(
                        consumed,
                        "dropped malformed OSC/APC/DCS escape (no terminator within 256 characters)"
                    );
                    return &rest[i + c.len_utf8()..];
                }
            }
            ""
        }
        // A lone ESC is dropped; what follows it is text.
        _ => &rest[1..],
    }
}

/// Like `cells_to_string` but strips trailing unstyled space cells (CellGrid padding).
fn cells_to_string_compact(cells: &[StyledCell]) -> String {
    // Find last non-padding cell (non-space or styled)
    let last_content = cells
        .iter()
        .rposition(|c| c.ch != ' ' || !c.style.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    cells_to_string(&cells[..last_content])
}

pub(crate) fn cells_to_string(cells: &[StyledCell]) -> String {
    let mut result = String::new();
    let mut current_style = String::new();

    for cell in cells {
        if cell.ch == '\0' {
            continue;
        }

        if cell.style != current_style {
            if !current_style.is_empty() {
                result.push_str("\x1b[0m");
            }
            if !cell.style.is_empty() {
                result.push_str(&cell.style);
            }
            current_style = cell.style.clone();
        }

        cell.push_grapheme(&mut result);
    }

    if !current_style.is_empty() {
        result.push_str("\x1b[0m");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_filled_with_spaces() {
        let grid = CellGrid::new(5, 3);
        assert_eq!(grid.width(), 5);
        assert_eq!(grid.height(), 3);

        for y in 0..3 {
            for x in 0..5 {
                let cell = grid.get(x, y).unwrap();
                assert_eq!(cell.ch, ' ');
            }
        }
    }

    #[test]
    fn blit_line_places_chars_at_position() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("ABC", 3, 0);

        let line = &grid.to_lines()[0];
        assert_eq!(line, "   ABC    ");
    }

    #[test]
    fn blit_string_handles_multiple_lines() {
        let mut grid = CellGrid::new(10, 3);
        grid.blit_string("AB\nCD\nEF", 2, 0);

        let lines = grid.to_lines();
        assert_eq!(lines[0], "  AB      ");
        assert_eq!(lines[1], "  CD      ");
        assert_eq!(lines[2], "  EF      ");
    }

    #[test]
    fn blit_respects_grid_bounds() {
        let mut grid = CellGrid::new(5, 2);
        grid.blit_line("ABCDEFGHIJ", 0, 0);
        grid.blit_line("XYZ", 0, 5);

        let lines = grid.to_lines();
        assert_eq!(lines[0], "ABCDE");
        assert_eq!(lines[1], "     ");
    }

    #[test]
    fn styled_content_preserved() {
        let mut grid = CellGrid::new(20, 1);
        grid.blit_line("\x1b[31mRED\x1b[0m", 0, 0);

        let line = &grid.to_lines()[0];
        assert!(line.contains("\x1b[31m"));
        assert!(line.contains("RED"));
    }

    #[test]
    fn sequential_style_escapes_are_accumulated() {
        let mut grid = CellGrid::new(20, 1);
        grid.blit_line("\x1b[48;5;12m\x1b[38;5;0m\x1b[1m PLAN \x1b[0m", 0, 0);

        let line = &grid.to_lines()[0];
        assert!(line.contains("\x1b[48;5;12m"));
        assert!(line.contains("\x1b[38;5;0m"));
        assert!(line.contains("\x1b[1m"));
        assert!(line.contains(" PLAN "));
    }

    #[test]
    fn multiple_blits_overwrite() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("AAAAAAAAAA", 0, 0);
        grid.blit_line("BBB", 3, 0);

        let line = &grid.to_lines()[0];
        assert_eq!(line, "AAABBBAAAA");
    }

    #[test]
    fn multi_line_row_rendering() {
        let mut grid = CellGrid::new(40, 2);
        grid.blit_string("Line1\nLine2", 0, 0);
        grid.blit_string("Short", 20, 0);

        let lines = grid.to_lines();
        assert!(lines[0].starts_with("Line1"));
        assert!(lines[0].contains("Short"));
        assert!(lines[1].starts_with("Line2"));
    }

    #[test]
    fn unterminated_osc_does_not_consume_visible_content() {
        // Malformed OSC (set-title) with no BEL/ST terminator, followed by
        // visible content. The parser must drop the escape and resume.
        let mut grid = CellGrid::new(20, 1);
        let malformed = format!("\x1b]52;c;{}TAIL", "A".repeat(400));
        grid.blit_line(&malformed, 0, 0);
        // After the 256-byte cap kicks in, subsequent characters keep blitting.
        // We don't pin which suffix bytes survive (depends on where the cap
        // landed inside "AAAA...TAIL"), only that we don't lock up and the
        // grid retains its allocated width.
        let line = &grid.to_lines()[0];
        assert_eq!(line.chars().count(), 20);
    }

    #[test]
    fn a_zwj_emoji_takes_two_cells_and_keeps_every_code_point() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line(&format!("a{family}b"), 0, 0);

        assert_eq!(
            grid.get(3, 0).unwrap().ch,
            'b',
            "the emoji covers columns 1 and 2"
        );
        assert_eq!(grid.text(0, 0..4), format!("a{family}b"));
        assert_eq!(grid.row_ansi(0), format!("a{family}b\x1b[K"));
    }

    #[test]
    fn cjk_takes_two_cells_per_character() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("日本x", 0, 0);
        assert_eq!(grid.get(4, 0).unwrap().ch, 'x');
        assert_eq!(
            grid.grapheme_span(0, 3),
            2..4,
            "a continuation cell answers for its lead"
        );
    }

    #[test]
    fn a_combining_mark_stays_with_its_base() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("e\u{301}x", 0, 0);
        assert_eq!(grid.get(1, 0).unwrap().ch, 'x');
        assert_eq!(grid.text(0, 0..1), "e\u{301}");
    }

    #[test]
    fn text_from_inside_a_wide_grapheme_takes_the_whole_grapheme() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("a日b", 0, 0);
        assert_eq!(grid.text(0, 2..4), "日b");
        assert_eq!(
            grid.text(0, 0..2),
            "a日",
            "a range that ends inside it keeps it"
        );
    }

    #[test]
    fn invert_marks_the_cells_and_widens_to_a_wide_lead() {
        let mut grid = CellGrid::new(10, 1);
        grid.blit_line("a日b", 0, 0);
        grid.invert(0, 2..3);
        assert!(grid.get(1, 0).unwrap().style.contains("\x1b[7m"));
        assert!(!grid.get(0, 0).unwrap().style.contains("\x1b[7m"));
        assert!(!grid.get(3, 0).unwrap().style.contains("\x1b[7m"));
    }

    /// Locks in the asymmetric composition rule documented above
    /// `final_style`: when an earlier blit established a bg, a later blit
    /// with no bg of its own picks up that bg. Tree layouts that don't
    /// overlap siblings never observe this; the test exists to make the
    /// trade-off explicit if anyone changes the composition logic.
    #[test]
    fn fg_only_write_inherits_prior_bg_from_cell() {
        let mut grid = CellGrid::new(10, 1);
        // First blit paints bg.
        grid.blit_line("\x1b[48;2;40;44;52m     \x1b[0m", 0, 0);
        // Second blit writes only fg over the same cells.
        grid.blit_line("\x1b[38;2;255;0;0mABC\x1b[0m", 0, 0);

        let line = &grid.to_lines()[0];
        // The bg escape from the first blit should still be present in the
        // composed output for the cells the second blit wrote to.
        assert!(
            line.contains("\x1b[48;2;40;44;52m"),
            "expected prior bg to be preserved through fg-only write: {:?}",
            line
        );
        assert!(line.contains("\x1b[38;2;255;0;0m"));
        assert!(line.contains("ABC"));
    }
}
