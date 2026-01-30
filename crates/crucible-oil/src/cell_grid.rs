use unicode_width::UnicodeWidthChar;

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
                                current_style = escape;
                            }
                        }
                    }
                    // OSC / APC / DCS: skip entirely without interpreting as visible chars
                    Some(&']') | Some(&'_') | Some(&'P') => {
                        chars.next();
                        while let Some(sc) = chars.next() {
                            if sc == '\x07' {
                                break;
                            }
                            if sc == '\x1b' && chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                let char_width = UnicodeWidthChar::width(c).unwrap_or(1);
                if col + char_width <= self.width {
                    self.cells[y][col] = StyledCell::new(c, current_style.clone());
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
}
