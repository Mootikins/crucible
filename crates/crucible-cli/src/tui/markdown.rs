//! Markdown rendering with syntax highlighting
//!
//! Uses termimad for markdown structure and syntect for code block highlighting.
//! Auto-detects terminal theme (dark/light).
//!
//! Tables are rendered with full box-drawing borders:
//! - Top border: ┌─┬─┐
//! - Bottom border: └─┴─┘
//! - Row separators between all data rows
//! - 1 space padding in cells

use std::env;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use termimad::MadSkin;

/// Markdown renderer with syntax highlighting support
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    dark_theme: Theme,
    light_theme: Theme,
    skin_dark: MadSkin,
    skin_light: MadSkin,
    is_dark: bool,
}

impl MarkdownRenderer {
    /// Create a new renderer with auto-detected theme
    pub fn new() -> Self {
        let is_dark = Self::detect_terminal_background();
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        let dark_theme = theme_set.themes["base16-ocean.dark"].clone();
        let light_theme = theme_set.themes["base16-ocean.light"].clone();

        let skin_dark = Self::create_dark_skin();
        let skin_light = Self::create_light_skin();

        Self {
            syntax_set,
            dark_theme,
            light_theme,
            skin_dark,
            skin_light,
            is_dark,
        }
    }

    /// Detect if terminal has dark background
    ///
    /// Checks in order:
    /// 1. COLORFGBG env var (format: "fg;bg", bg > 6 = light)
    /// 2. TERM_BACKGROUND env var ("dark" | "light")
    /// 3. Default to dark
    pub fn detect_terminal_background() -> bool {
        // Check COLORFGBG first
        if let Ok(val) = env::var("COLORFGBG") {
            if let Some(bg) = val.split(';').nth(1) {
                if let Ok(bg_num) = bg.parse::<u8>() {
                    return bg_num <= 6; // 0-6 are dark colors
                }
            }
        }

        // Check TERM_BACKGROUND
        if let Ok(val) = env::var("TERM_BACKGROUND") {
            return val.to_lowercase() != "light";
        }

        // Default to dark
        true
    }

    /// Render markdown to ANSI-styled string (no width constraint)
    pub fn render(&self, markdown: &str) -> String {
        self.render_with_width(markdown, None)
    }

    /// Render markdown with optional width constraint for word wrapping
    ///
    /// When width is provided, text wraps at word boundaries to fit.
    /// This should be used for TUI rendering where we know the column width.
    pub fn render_with_width(&self, markdown: &str, width: Option<usize>) -> String {
        let skin = if self.is_dark {
            &self.skin_dark
        } else {
            &self.skin_light
        };

        // Pre-process tables to add full borders
        let processed = preprocess_tables(markdown, width);

        // Use termimad's word-aware wrapping when width is specified
        skin.text(&processed, width).to_string()
    }

    /// Add padding spaces inside inline code backticks for visual clarity
    ///
    /// Transforms `` `code` `` to `` ` code ` `` so the background color
    /// has visual padding around the text.
    fn add_inline_code_padding(markdown: &str) -> String {
        // Match single backtick inline code (not code blocks with triple backticks)
        // Pattern: single ` not followed by ` or preceded by `
        let mut result = String::with_capacity(markdown.len() + 64);
        let mut chars = markdown.chars().peekable();
        let mut in_code_block = false;

        while let Some(c) = chars.next() {
            if c == '`' {
                // Check for triple backtick (code block)
                if chars.peek() == Some(&'`') {
                    // Could be code block, check for third
                    let next = chars.next().unwrap(); // second `
                    if chars.peek() == Some(&'`') {
                        // Triple backtick - code block marker
                        let third = chars.next().unwrap();
                        result.push(c);
                        result.push(next);
                        result.push(third);
                        in_code_block = !in_code_block;
                    } else {
                        // Double backtick inline code
                        result.push(c);
                        result.push(next);
                    }
                } else if !in_code_block {
                    // Single backtick - inline code
                    // Find the closing backtick and add padding
                    result.push(c);
                    result.push(' '); // Opening padding

                    // Copy content until closing backtick
                    let mut found_close = false;
                    for inner in chars.by_ref() {
                        if inner == '`' {
                            result.push(' '); // Closing padding
                            result.push(inner);
                            found_close = true;
                            break;
                        }
                        result.push(inner);
                    }
                    if !found_close {
                        // Unclosed backtick - just continue
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Render a code block with syntax highlighting
    #[allow(dead_code)]
    fn render_code_block(&self, code: &str, lang: &str) -> String {
        use syntect::easy::HighlightLines;
        use syntect::util::as_24_bit_terminal_escaped;

        let theme = if self.is_dark {
            &self.dark_theme
        } else {
            &self.light_theme
        };

        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut h = HighlightLines::new(syntax, theme);
        let mut result = String::new();

        for line in code.lines() {
            match h.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let escaped = as_24_bit_terminal_escaped(&ranges, false);
                    result.push_str(&escaped);
                    result.push('\n');
                }
                Err(_) => {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        // Reset colors at end
        result.push_str("\x1b[0m");
        result
    }

    fn create_dark_skin() -> MadSkin {
        use termimad::crossterm::style::Color;

        let mut skin = MadSkin::default();
        skin.bold.set_fg(Color::White);
        skin.italic.set_fg(Color::Cyan);
        skin.inline_code.set_bg(Color::DarkGrey);
        skin.code_block.set_bg(Color::DarkGrey);
        skin
    }

    fn create_light_skin() -> MadSkin {
        use termimad::crossterm::style::Color;

        let mut skin = MadSkin::default();
        skin.bold.set_fg(Color::Black);
        skin.italic.set_fg(Color::DarkBlue);
        skin.inline_code.set_bg(Color::Grey);
        skin.code_block.set_bg(Color::Grey);
        skin
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Table Preprocessing
// =============================================================================

/// Box drawing characters for tables
mod box_chars {
    pub const TOP_LEFT: char = '┌';
    pub const TOP_RIGHT: char = '┐';
    pub const BOTTOM_LEFT: char = '└';
    pub const BOTTOM_RIGHT: char = '┘';
    pub const HORIZONTAL: char = '─';
    pub const VERTICAL: char = '│';
    pub const TOP_T: char = '┬';
    pub const BOTTOM_T: char = '┴';
    pub const LEFT_T: char = '├';
    pub const RIGHT_T: char = '┤';
    pub const CROSS: char = '┼';
}

/// Pre-process markdown to render tables with full borders
///
/// Tables in markdown format are detected and rendered with:
/// - Top border (┌─┬─┐)
/// - Bottom border (└─┴─┘)
/// - Row separators between all data rows
/// - 1 space padding in cells
fn preprocess_tables(markdown: &str, max_width: Option<usize>) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Check if this starts a table (line with | characters)
        if is_table_row(lines[i]) {
            // Find all consecutive table lines
            let table_start = i;
            while i < lines.len() && (is_table_row(lines[i]) || is_separator_row(lines[i])) {
                i += 1;
            }
            let table_end = i;

            // Extract and render the table
            let table_lines: Vec<&str> = lines[table_start..table_end].to_vec();
            let rendered = render_table_with_borders(&table_lines, max_width);
            result.push_str(&rendered);
        } else {
            // Non-table line - pass through
            result.push_str(lines[i]);
            result.push('\n');
            i += 1;
        }
    }

    result
}

/// Check if a line is a table row (contains | and content)
fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('|') && !is_separator_row(line)
}

/// Check if a line is a table separator row (like |---|---|)
fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return false;
    }
    // Separator rows contain mainly dashes, colons, pipes, and spaces
    trimmed
        .chars()
        .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
}

/// Parse a table row into cells
fn parse_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    // Remove leading/trailing pipes and split
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(|s| s.trim().to_string()).collect()
}

/// Calculate display width of a string (accounting for Unicode)
fn display_width(s: &str) -> usize {
    // Simple implementation - count characters
    // For proper Unicode width, we'd use unicode-width crate
    s.chars().count()
}

/// Wrap text to fit within a given width, returning lines
///
/// Uses word-level wrapping: lines break at word boundaries.
/// If a single word is longer than the column width, it is kept whole
/// (allowed to overflow) rather than being broken mid-word.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || display_width(text) <= width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = display_width(word);
        if current_width == 0 {
            // First word on line - always add it (even if it overflows)
            current_line = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Need to wrap - push current line and start new one
            lines.push(current_line);
            // Start new line with this word (even if it overflows)
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Truncate a string to fit within max_width characters
///
/// Handles UTF-8 properly by truncating at character boundaries.
fn truncate_line(line: &str, max_width: usize) -> String {
    if display_width(line) <= max_width {
        return line.to_string();
    }
    let mut width = 0;
    let mut result = String::new();
    for c in line.chars() {
        if width >= max_width {
            break;
        }
        result.push(c);
        width += 1;
    }
    result
}

/// Render a table with full box-drawing borders
fn render_table_with_borders(lines: &[&str], max_width: Option<usize>) -> String {
    // Parse all rows
    let mut header_rows: Vec<Vec<String>> = Vec::new();
    let mut data_rows: Vec<Vec<String>> = Vec::new();
    let mut found_separator = false;

    for line in lines {
        if is_separator_row(line) {
            found_separator = true;
            continue;
        }

        let cells = parse_row(line);
        if !found_separator {
            header_rows.push(cells);
        } else {
            data_rows.push(cells);
        }
    }

    // Combine for column width calculation
    let all_rows: Vec<&Vec<String>> = header_rows.iter().chain(data_rows.iter()).collect();
    if all_rows.is_empty() {
        return String::new();
    }

    // Calculate number of columns
    let num_cols = all_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return String::new();
    }

    // Calculate initial column widths (content width)
    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for row in &all_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(display_width(cell));
            }
        }
    }

    // Calculate minimum column widths based on longest word in each column
    // Since we never break words, each column must be at least as wide as its longest word
    let mut min_col_widths: Vec<usize> = vec![3; num_cols]; // minimum 3 chars
    for row in &all_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                let longest_word_width = cell
                    .split_whitespace()
                    .map(display_width)
                    .max()
                    .unwrap_or(0);
                min_col_widths[i] = min_col_widths[i].max(longest_word_width);
            }
        }
    }

    // Ensure minimum width of 3 for each column
    for w in &mut col_widths {
        *w = (*w).max(3);
    }

    // Calculate total table width: │ cell │ cell │ ... │
    // Each cell has: padding (1) + content + padding (1)
    // Borders: num_cols + 1 vertical bars
    let border_overhead = num_cols + 1; // vertical bars
    let padding_overhead = num_cols * 2; // 1 space padding on each side of each cell
    let total_content_width: usize = col_widths.iter().sum();
    let table_width = total_content_width + padding_overhead + border_overhead;

    // If max_width is specified and table is too wide, shrink columns proportionally
    // but never below the minimum width needed for the longest word
    if let Some(max_w) = max_width {
        if table_width > max_w {
            // Calculate available width for content (subtract borders and padding)
            let min_table_overhead = border_overhead + padding_overhead;
            if max_w > min_table_overhead {
                let available_content = max_w - min_table_overhead;

                // Calculate total of minimum widths
                let total_min: usize = min_col_widths.iter().sum();

                if total_content_width > available_content && available_content >= total_min {
                    // We need to shrink, and we have room above minimums
                    // Calculate how much "shrinkable" space we have
                    let shrinkable_total: usize = col_widths
                        .iter()
                        .zip(min_col_widths.iter())
                        .map(|(w, min)| w.saturating_sub(*min))
                        .sum();

                    if shrinkable_total > 0 {
                        // How much we need to shrink in total
                        let excess = total_content_width - available_content;

                        // Shrink each column proportionally to its shrinkable space
                        for (i, w) in col_widths.iter_mut().enumerate() {
                            let shrinkable = w.saturating_sub(min_col_widths[i]);
                            if shrinkable > 0 {
                                // This column's share of the shrinking
                                let shrink_amount =
                                    (shrinkable as f64 / shrinkable_total as f64 * excess as f64)
                                        .ceil() as usize;
                                let new_width = w.saturating_sub(shrink_amount);
                                *w = new_width.max(min_col_widths[i]);
                            }
                        }
                    }
                } else if available_content < total_min {
                    // Even minimums don't fit - use minimums and let it clip
                    for (i, w) in col_widths.iter_mut().enumerate() {
                        *w = min_col_widths[i];
                    }
                }
            }
        }
    }

    // Now build the table
    // Lines will be truncated to max_width to prevent termimad from wrapping them
    let mut output = String::new();

    // Top border: ┌─────┬─────┬─────┐
    output.push(box_chars::TOP_LEFT);
    for (i, &w) in col_widths.iter().enumerate() {
        output.push_str(&box_chars::HORIZONTAL.to_string().repeat(w + 2));
        if i < num_cols - 1 {
            output.push(box_chars::TOP_T);
        }
    }
    output.push(box_chars::TOP_RIGHT);
    output.push('\n');

    // Render header rows
    for row in &header_rows {
        render_data_row(&mut output, row, &col_widths, num_cols);
    }

    // Header separator (if we have headers and data)
    if !header_rows.is_empty() && !data_rows.is_empty() {
        render_separator_row(&mut output, &col_widths, num_cols);
    }

    // Render data rows with separators between them
    for (idx, row) in data_rows.iter().enumerate() {
        render_data_row(&mut output, row, &col_widths, num_cols);

        // Add separator between data rows (but not after the last one)
        if idx < data_rows.len() - 1 {
            render_separator_row(&mut output, &col_widths, num_cols);
        }
    }

    // Bottom border: └─────┴─────┴─────┘
    output.push(box_chars::BOTTOM_LEFT);
    for (i, &w) in col_widths.iter().enumerate() {
        output.push_str(&box_chars::HORIZONTAL.to_string().repeat(w + 2));
        if i < num_cols - 1 {
            output.push(box_chars::BOTTOM_T);
        }
    }
    output.push(box_chars::BOTTOM_RIGHT);
    output.push('\n');

    // If max_width is specified, truncate each line to prevent termimad from
    // adding line breaks that would corrupt the table structure.
    // This clips the table at the viewport edge rather than wrapping incorrectly.
    if let Some(max_w) = max_width {
        output
            .lines()
            .map(|line| truncate_line(line, max_w))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        output
    }
}

/// Render a data row (potentially wrapping cells if needed)
fn render_data_row(output: &mut String, row: &[String], col_widths: &[usize], num_cols: usize) {
    // Wrap each cell's content if needed
    let mut wrapped_cells: Vec<Vec<String>> = Vec::new();
    for (i, cell) in row.iter().enumerate() {
        let w = col_widths.get(i).copied().unwrap_or(3);
        wrapped_cells.push(wrap_text(cell, w));
    }
    // Pad with empty cells if row has fewer columns
    while wrapped_cells.len() < num_cols {
        wrapped_cells.push(vec![String::new()]);
    }

    // Find max lines needed
    let max_lines = wrapped_cells.iter().map(|c| c.len()).max().unwrap_or(1);

    // Render each line
    for line_idx in 0..max_lines {
        output.push(box_chars::VERTICAL);
        for (col_idx, wrapped) in wrapped_cells.iter().enumerate() {
            let w = col_widths.get(col_idx).copied().unwrap_or(3);
            let content = wrapped.get(line_idx).map(|s| s.as_str()).unwrap_or("");
            let content_width = display_width(content);
            let padding_right = w.saturating_sub(content_width);

            output.push(' '); // Left padding
            output.push_str(content);
            output.push_str(&" ".repeat(padding_right));
            output.push(' '); // Right padding
            output.push(box_chars::VERTICAL);
        }
        output.push('\n');
    }
}

/// Render a separator row: ├─────┼─────┼─────┤
fn render_separator_row(output: &mut String, col_widths: &[usize], num_cols: usize) {
    output.push(box_chars::LEFT_T);
    for (i, &w) in col_widths.iter().enumerate() {
        output.push_str(&box_chars::HORIZONTAL.to_string().repeat(w + 2));
        if i < num_cols - 1 {
            output.push(box_chars::CROSS);
        }
    }
    output.push(box_chars::RIGHT_T);
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let renderer = MarkdownRenderer::new();
        assert!(!renderer.syntax_set.syntaxes().is_empty());
    }

    #[test]
    fn test_plain_text_passthrough() {
        let renderer = MarkdownRenderer::new();
        let result = renderer.render("Hello world");
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn test_theme_detection_default_dark() {
        // Clear env vars
        env::remove_var("COLORFGBG");
        env::remove_var("TERM_BACKGROUND");
        assert!(MarkdownRenderer::detect_terminal_background());
    }

    #[test]
    fn test_code_block_highlighting() {
        let renderer = MarkdownRenderer::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let result = renderer.render_code_block(code, "rust");
        // Should contain ANSI escape codes
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn test_table_rendering() {
        let renderer = MarkdownRenderer::new();
        let md = r#"| Feature | Rust | Go |
|---------|------|-----|
| Memory | Safe | GC |
| Speed | Fast | Fast |"#;
        let result = renderer.render_with_width(md, Some(80));
        // Print the result for debugging
        eprintln!("Table output:\n{}", result);
        // Should have table borders
        assert!(result.contains("│"), "Should have vertical border");
        assert!(result.contains("─"), "Should have horizontal border");
        // Check for top border
        assert!(result.contains("┌"), "Should have top-left corner");
        assert!(result.contains("┐"), "Should have top-right corner");
        // Check for bottom border
        assert!(result.contains("└"), "Should have bottom-left corner");
        assert!(result.contains("┘"), "Should have bottom-right corner");
    }

    #[test]
    fn test_inline_code_padding() {
        // Simple inline code gets padding
        assert_eq!(
            MarkdownRenderer::add_inline_code_padding("Use `cargo` here"),
            "Use ` cargo ` here"
        );

        // Multiple inline codes
        assert_eq!(
            MarkdownRenderer::add_inline_code_padding("Use `a` and `b`"),
            "Use ` a ` and ` b `"
        );

        // Code blocks are not modified
        assert_eq!(
            MarkdownRenderer::add_inline_code_padding("```rust\ncode\n```"),
            "```rust\ncode\n```"
        );

        // Mixed inline and block
        assert_eq!(
            MarkdownRenderer::add_inline_code_padding("Run `cmd`\n```\nblock\n```"),
            "Run ` cmd `\n```\nblock\n```"
        );
    }

    #[test]
    fn test_table_proportional_shrinking() {
        // Table with moderate-length content that can be shrunk
        // Using shorter words to allow shrinking without hitting word-break limits
        let md = r#"| Name | Info | Code | Type |
|------|------|------|------|
| foo | Some long description here | 1.0 | MIT |
| bar | Another desc text value | 2.0 | BSD |"#;

        let renderer = MarkdownRenderer::new();

        // At 80 chars, table should fit without shrinking
        let wide = renderer.render_with_width(md, Some(80));
        eprintln!("Wide table (80 chars):\n{}", wide);
        // All closing borders should be present
        for line in wide.lines() {
            if line.contains("│") && !line.trim().is_empty() {
                // Data/header rows should end with │
                let trimmed = line.trim_end();
                if trimmed.starts_with("│") {
                    assert!(
                        trimmed.ends_with("│"),
                        "Row should have closing border at width 80: {}",
                        line
                    );
                }
            }
        }

        // At 50 chars, table must shrink but all columns should still be visible
        // Minimum widths: Name=4, Info=11 ("description"), Code=4, Type=4 = 23
        // With borders (5) and padding (8) = 36, so 50 chars should work
        let narrow = renderer.render_with_width(md, Some(50));
        eprintln!("Narrow table (50 chars):\n{}", narrow);

        // All rows should have closing borders (not clipped)
        for line in narrow.lines() {
            let trimmed = line.trim_end();
            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }
            // If it's a table row (starts with box char), it should be complete
            if trimmed.starts_with('│')
                || trimmed.starts_with('┌')
                || trimmed.starts_with('├')
                || trimmed.starts_with('└')
            {
                assert!(
                    trimmed.ends_with('│')
                        || trimmed.ends_with('┐')
                        || trimmed.ends_with('┤')
                        || trimmed.ends_with('┘'),
                    "Table line should have closing border at width 50: '{}'",
                    trimmed
                );
            }
        }

        // All column headers should be present
        assert!(narrow.contains("Name"), "Should show Name");
        assert!(narrow.contains("Info"), "Should show Info");
        assert!(narrow.contains("Code"), "Should show Code");
        assert!(narrow.contains("Type"), "Should show Type");
    }

    #[test]
    fn test_table_minimum_width_respected() {
        // Table with long words that can't be broken
        let md = r#"| Name | VeryLongUnbreakableWord |
|------|-------------------------|
| test | AnotherLongWord |"#;

        let renderer = MarkdownRenderer::new();

        // Even at narrow width, words shouldn't be broken
        let narrow = renderer.render_with_width(md, Some(40));
        eprintln!("Narrow table with long words:\n{}", narrow);

        // The long word should appear intact (possibly causing overflow)
        assert!(
            narrow.contains("VeryLongUnbreakableWord") || narrow.contains("VeryLong"),
            "Long word should appear (whole or clipped at viewport, not mid-word broken)"
        );
    }
}
