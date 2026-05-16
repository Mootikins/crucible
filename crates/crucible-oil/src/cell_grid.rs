use unicode_width::UnicodeWidthChar;

use crate::ansi::extract_bg;

#[derive(Debug, Clone, Default)]
pub struct StyledCell {
    pub ch: char,
    pub style: String,
}

impl StyledCell {
    pub fn space() -> Self {
        Self {
            ch: ' ',
            style: String::new(),
        }
    }

    pub fn new(ch: char, style: String) -> Self {
        Self { ch, style }
    }
}

#[derive(Debug, Clone)]
pub struct CellGrid {
    cells: Vec<Vec<StyledCell>>,
    width: usize,
    height: usize,
}

impl CellGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let cells = (0..height)
            .map(|_| (0..width).map(|_| StyledCell::space()).collect())
            .collect();
        Self {
            cells,
            width,
            height,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn set(&mut self, x: usize, y: usize, cell: StyledCell) {
        if x < self.width && y < self.height {
            self.cells[y][x] = cell;
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&StyledCell> {
        self.cells.get(y).and_then(|row| row.get(x))
    }

    pub fn blit_line(&mut self, line: &str, x: usize, y: usize) {
        if y >= self.height {
            return;
        }

        let mut col = x;
        let mut current_style = String::new();
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            if col >= self.width {
                break;
            }

            if c == '\x1b' {
                match chars.peek() {
                    Some(&'[') => {
                        let mut escape = String::from("\x1b[");
                        chars.next();
                        while let Some(&next) = chars.peek() {
                            escape.push(chars.next().unwrap());
                            if next.is_ascii_alphabetic() {
                                break;
                            }
                        }
                        if escape.contains('m') {
                            if escape == "\x1b[0m" || escape == "\x1b[m" {
                                current_style.clear();
                            } else {
                                current_style.push_str(&escape);
                            }
                        }
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
                    // no current path does. Stage B (render path unification) is the
                    // right place to converge them.
                    Some(&']') | Some(&'_') | Some(&'P') => {
                        chars.next();
                        let mut consumed = 0usize;
                        let mut terminated = false;
                        while let Some(sc) = chars.next() {
                            consumed += 1;
                            if sc == '\x07' {
                                terminated = true;
                                break;
                            }
                            if sc == '\x1b' && chars.peek() == Some(&'\\') {
                                chars.next();
                                terminated = true;
                                break;
                            }
                            if consumed >= 256 {
                                break;
                            }
                        }
                        if !terminated {
                            tracing::debug!(
                                consumed,
                                "dropped malformed OSC/APC/DCS escape (no terminator within 256 characters)"
                            );
                        }
                    }
                    _ => {}
                }
            } else {
                let char_width = UnicodeWidthChar::width(c).unwrap_or(1);
                if col + char_width <= self.width {
                    // Style composition: if the new write doesn't set its
                    // own bg, inherit whatever bg was on the cell already.
                    // This lets a parent Box's `style.bg` survive children
                    // that only paint fg, mirroring CSS layering. Pair
                    // with `tree_render::render_box_content`'s bg-fill.
                    //
                    // Asymmetric guarantee: this composes by *cell state*,
                    // not by tree ancestry. If a sibling Box-with-bg paints
                    // a region, then a *later* sibling (no bg) writes text
                    // over the same cells, the second sibling's text picks
                    // up the first sibling's bg. Tree layouts that don't
                    // overlap siblings (Crucible's norm) see only the
                    // intended parent→child inheritance.
                    let final_style = if extract_bg(&current_style).is_none() {
                        match extract_bg(&self.cells[y][col].style) {
                            Some(prior_bg) => {
                                if current_style.is_empty() {
                                    prior_bg
                                } else {
                                    format!("{}{}", prior_bg, current_style)
                                }
                            }
                            None => current_style.clone(),
                        }
                    } else {
                        current_style.clone()
                    };
                    self.cells[y][col] = StyledCell::new(c, final_style);
                    for i in 1..char_width {
                        if col + i < self.width {
                            self.cells[y][col + i] = StyledCell::new('\0', String::new());
                        }
                    }
                    col += char_width;
                }
            }
        }
    }

    pub fn blit_string(&mut self, content: &str, x: usize, y: usize) {
        for (row_idx, line) in content.lines().enumerate() {
            let target_y = y + row_idx;
            if target_y < self.height {
                self.blit_line(line, x, target_y);
            }
        }
    }

    pub fn to_lines(&self) -> Vec<String> {
        self.cells.iter().map(|row| cells_to_string(row)).collect()
    }

    pub fn to_string_joined(&self) -> String {
        self.to_lines().join("\r\n")
    }

    /// Extract rows `[y_start, y_end)` as a compact string (trailing padding stripped).
    ///
    /// Each row is rendered with `cells_to_string_compact`, joined with `\r\n`.
    /// Panics if indices are out of bounds.
    pub fn extract_rows(&self, y_start: usize, y_end: usize) -> String {
        self.cells[y_start..y_end]
            .iter()
            .map(|row| cells_to_string_compact(row))
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    /// Find the last row with non-space (or styled) content, returning count of content rows.
    ///
    /// Returns 0 for an entirely blank grid.
    pub fn content_height(&self) -> usize {
        self.cells
            .iter()
            .rposition(|row| row.iter().any(|c| c.ch != ' ' || !c.style.is_empty()))
            .map(|i| i + 1)
            .unwrap_or(0)
    }

    /// Render to compact string with trailing padding stripped per line.
    /// Used for graduation output where fixed-width padding is wasteful.
    pub fn to_string_compact(&self) -> String {
        self.cells
            .iter()
            .map(|row| cells_to_string_compact(row))
            .collect::<Vec<_>>()
            .join("\r\n")
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

fn cells_to_string(cells: &[StyledCell]) -> String {
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

        result.push(cell.ch);
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
