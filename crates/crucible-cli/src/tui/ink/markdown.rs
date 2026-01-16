//! Markdown to Ink Node renderer
//!
//! Parses markdown using markdown-it and converts to ink Node trees
//! for styled terminal rendering.

use crate::tui::ink::ansi::{visible_width, wrap_styled_text};
use crate::tui::ink::node::*;
use crate::tui::ink::style::{Color, Style};
use markdown_it::parser::inline::Text;
use markdown_it::plugins::cmark::block::blockquote::Blockquote;
use markdown_it::plugins::cmark::block::code::CodeBlock as MdCodeBlock;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::backticks::CodeInline;
use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};
use markdown_it::plugins::cmark::inline::link::Link;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::plugins::extra::tables::{Table, TableBody, TableCell, TableHead, TableRow};
use markdown_it::MarkdownIt;

/// Convert markdown text to an ink Node tree
pub fn markdown_to_node(markdown: &str) -> Node {
    let width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    markdown_to_node_with_width(markdown, width)
}

/// Convert markdown text to an ink Node tree with explicit width
pub fn markdown_to_node_with_width(markdown: &str, width: usize) -> Node {
    // Normalize <br> tags to newlines before parsing
    // This handles HTML line breaks that markdown-it doesn't process by default
    let markdown = normalize_br_tags(markdown);

    let md = create_parser();
    let ast = md.parse(&markdown);

    let mut ctx = RenderContext::new(width);
    render_node(&ast, &mut ctx);
    ctx.into_node()
}

fn create_parser() -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::tables::add(&mut md);
    md
}

struct RenderContext {
    blocks: Vec<Node>,
    current_spans: Vec<(String, Style)>,
    style_stack: Vec<Style>,
    list_depth: usize,
    list_counter: Option<usize>,
    needs_blank_line: bool,
    width: usize,
}

impl RenderContext {
    fn new(width: usize) -> Self {
        Self {
            blocks: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![Style::default()],
            list_depth: 0,
            list_counter: None,
            needs_blank_line: false,
            width,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn push_text(&mut self, text: &str) {
        if !text.is_empty() {
            self.current_spans
                .push((text.to_string(), self.current_style()));
        }
    }

    fn flush_line(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }

        let spans = std::mem::take(&mut self.current_spans);

        if spans.len() == 1 {
            let (content, style) = spans.into_iter().next().unwrap();
            if style == Style::default() {
                self.blocks.push(text_node(&content));
            } else {
                self.blocks.push(styled(&content, style));
            }
        } else {
            let styled_spans: Vec<(String, String)> = spans
                .into_iter()
                .map(|(content, style)| {
                    let codes = if style == Style::default() {
                        String::new()
                    } else {
                        style.to_ansi_codes()
                    };
                    (content, codes)
                })
                .collect();

            let wrapped = wrap_styled_text(&styled_spans, self.width);
            for line in wrapped {
                self.blocks.push(text_node(&line));
            }
        }
    }

    fn push_block(&mut self, node: Node) {
        self.flush_line();
        self.blocks.push(node);
    }

    fn push_block_with_spacing(&mut self, node: Node) {
        self.flush_line();
        if self.needs_blank_line && !self.blocks.is_empty() {
            self.blocks.push(text(""));
        }
        self.blocks.push(node);
        self.needs_blank_line = true;
    }

    fn mark_block_end(&mut self) {
        self.needs_blank_line = true;
    }

    fn into_node(mut self) -> Node {
        self.flush_line();
        if self.blocks.is_empty() {
            Node::Empty
        } else if self.blocks.len() == 1 {
            self.blocks.pop().unwrap()
        } else {
            col(self.blocks)
        }
    }
}

fn render_node(node: &markdown_it::Node, ctx: &mut RenderContext) {
    if node.cast::<Paragraph>().is_some() {
        if ctx.needs_blank_line && !ctx.blocks.is_empty() {
            ctx.blocks.push(text(""));
        }
        render_children(node, ctx);
        ctx.flush_line();
        ctx.mark_block_end();
        return;
    }

    if let Some(heading) = node.cast::<ATXHeading>() {
        if ctx.needs_blank_line && !ctx.blocks.is_empty() {
            ctx.blocks.push(text(""));
        }
        let style = heading_style(heading.level);
        ctx.push_style(style);
        render_children(node, ctx);
        ctx.pop_style();
        ctx.flush_line();
        ctx.mark_block_end();
        return;
    }

    if node.cast::<CodeFence>().is_some() || node.cast::<MdCodeBlock>().is_some() {
        render_code_block(node, ctx);
        return;
    }

    if node.cast::<BulletList>().is_some() {
        if ctx.needs_blank_line && !ctx.blocks.is_empty() {
            ctx.blocks.push(text(""));
        }
        ctx.list_depth += 1;
        ctx.list_counter = None;
        render_children(node, ctx);
        ctx.list_depth -= 1;
        ctx.mark_block_end();
        return;
    }

    if node.cast::<OrderedList>().is_some() {
        if ctx.needs_blank_line && !ctx.blocks.is_empty() {
            ctx.blocks.push(text(""));
        }
        ctx.list_depth += 1;
        ctx.list_counter = Some(1);
        render_children(node, ctx);
        ctx.list_depth -= 1;
        ctx.list_counter = None;
        ctx.mark_block_end();
        return;
    }

    if node.cast::<ListItem>().is_some() {
        render_list_item(node, ctx);
        return;
    }

    if node.cast::<Blockquote>().is_some() {
        render_blockquote(node, ctx);
        return;
    }

    if node.cast::<Table>().is_some() {
        render_table(node, ctx);
        return;
    }

    if let Some(link) = node.cast::<Link>() {
        render_link(node, link, ctx);
        return;
    }

    if let Some(text) = node.cast::<Text>() {
        ctx.push_text(&text.content);
        return;
    }

    if node.cast::<Strong>().is_some() {
        let style = ctx.current_style().bold();
        ctx.push_style(style);
        render_children(node, ctx);
        ctx.pop_style();
        return;
    }

    if node.cast::<Em>().is_some() {
        let style = ctx.current_style().italic();
        ctx.push_style(style);
        render_children(node, ctx);
        ctx.pop_style();
        return;
    }

    if node.cast::<CodeInline>().is_some() {
        let code_text = extract_all_text(node);
        let style = Style::new().fg(Color::Yellow);
        ctx.current_spans.push((format!("`{}`", code_text), style));
        return;
    }

    if node.cast::<Softbreak>().is_some() {
        ctx.push_text(" ");
        return;
    }

    if node.cast::<Hardbreak>().is_some() {
        ctx.flush_line();
        return;
    }

    render_children(node, ctx);
}

fn render_children(node: &markdown_it::Node, ctx: &mut RenderContext) {
    for child in node.children.iter() {
        render_node(child, ctx);
    }
}

fn render_code_block(node: &markdown_it::Node, ctx: &mut RenderContext) {
    let (content, lang) = if let Some(fence) = node.cast::<CodeFence>() {
        let lang = fence.info.split_whitespace().next().map(|s| s.to_string());
        (fence.content.clone(), lang)
    } else if let Some(code) = node.cast::<MdCodeBlock>() {
        (code.content.clone(), None)
    } else {
        (extract_all_text(node), None)
    };

    if ctx.needs_blank_line && !ctx.blocks.is_empty() {
        ctx.blocks.push(text(""));
    }

    let lang_str = lang.as_deref().unwrap_or("");

    if !lang_str.is_empty() {
        ctx.push_block(styled(
            format!("```{}", lang_str),
            Style::new().fg(Color::DarkGray),
        ));
    } else {
        ctx.push_block(styled("```", Style::new().fg(Color::DarkGray)));
    }

    let code_style = Style::new().fg(Color::Green);
    for line in content.lines() {
        ctx.push_block(styled(line, code_style));
    }

    ctx.push_block(styled("```", Style::new().fg(Color::DarkGray)));
    ctx.mark_block_end();
}

fn render_list_item(node: &markdown_it::Node, ctx: &mut RenderContext) {
    let indent = "  ".repeat(ctx.list_depth.saturating_sub(1));

    let bullet = if let Some(counter) = ctx.list_counter.as_mut() {
        let n = *counter;
        *counter += 1;
        format!("{}{}. ", indent, n)
    } else {
        format!("{}• ", indent)
    };

    ctx.current_spans.push((bullet, Style::default()));

    for child in node.children.iter() {
        if child.cast::<BulletList>().is_some() || child.cast::<OrderedList>().is_some() {
            ctx.flush_line();
            render_node(child, ctx);
        } else {
            render_node(child, ctx);
        }
    }
    ctx.flush_line();
}

fn render_blockquote(node: &markdown_it::Node, ctx: &mut RenderContext) {
    ctx.flush_line();

    if ctx.needs_blank_line && !ctx.blocks.is_empty() {
        ctx.blocks.push(text(""));
    }

    let prefix = "│ ";
    let prefix_width = 2;
    let content_width = ctx.width.saturating_sub(prefix_width);

    for child in node.children.iter() {
        let child_text = extract_all_text(child);
        let wrapped = wrap_text(&child_text, content_width);
        for line in wrapped {
            ctx.push_block(row([
                styled(prefix, Style::new().fg(Color::DarkGray)),
                styled(line, Style::new().fg(Color::Gray).italic()),
            ]));
        }
    }
    ctx.mark_block_end();
}

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

fn render_table(node: &markdown_it::Node, ctx: &mut RenderContext) {
    ctx.flush_line();

    if ctx.needs_blank_line && !ctx.blocks.is_empty() {
        ctx.blocks.push(text(""));
    }

    let mut header_rows: Vec<Vec<String>> = Vec::new();
    let mut body_rows: Vec<Vec<String>> = Vec::new();

    for child in node.children.iter() {
        let is_header = child.cast::<TableHead>().is_some();
        let is_body = child.cast::<TableBody>().is_some();

        if is_header || is_body {
            for row_node in child.children.iter() {
                if row_node.cast::<TableRow>().is_some() {
                    let mut cells: Vec<String> = Vec::new();
                    for cell_node in row_node.children.iter() {
                        if cell_node.cast::<TableCell>().is_some() {
                            cells.push(extract_all_text(cell_node).trim().to_string());
                        }
                    }
                    if is_header {
                        header_rows.push(cells);
                    } else {
                        body_rows.push(cells);
                    }
                }
            }
        }
    }

    if header_rows.is_empty() && body_rows.is_empty() {
        return;
    }

    let all_rows: Vec<&Vec<String>> = header_rows.iter().chain(body_rows.iter()).collect();
    let num_cols = all_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return;
    }

    let mut col_widths: Vec<usize> = vec![3; num_cols];
    let mut min_col_widths: Vec<usize> = vec![3; num_cols];

    for row in &all_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(visible_width(cell));
                let longest_word = cell
                    .split_whitespace()
                    .map(visible_width)
                    .max()
                    .unwrap_or(0);
                min_col_widths[i] = min_col_widths[i].max(longest_word);
            }
        }
    }

    let fixed_overhead = num_cols + 1 + num_cols * 2;
    let max_content_width = ctx.width.saturating_sub(fixed_overhead);

    let total_content: usize = col_widths.iter().sum();

    if total_content > max_content_width && max_content_width > 0 {
        let total_min: usize = min_col_widths.iter().sum();

        if max_content_width >= total_min {
            let shrinkable: usize = col_widths
                .iter()
                .zip(min_col_widths.iter())
                .map(|(w, min)| w.saturating_sub(*min))
                .sum();

            if shrinkable > 0 {
                let excess = total_content - max_content_width;
                for (i, w) in col_widths.iter_mut().enumerate() {
                    let shrink_space = w.saturating_sub(min_col_widths[i]);
                    if shrink_space > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        let shrink = (shrink_space as f64 / shrinkable as f64 * excess as f64)
                            .ceil() as usize;
                        *w = (*w).saturating_sub(shrink).max(min_col_widths[i]);
                    }
                }
            }
        } else {
            let per_col = max_content_width / num_cols;
            for w in col_widths.iter_mut() {
                *w = per_col.max(1);
            }
        }

        let final_content: usize = col_widths.iter().sum();
        if final_content > max_content_width {
            let mut excess = final_content - max_content_width;
            for w in col_widths.iter_mut().rev() {
                if excess == 0 {
                    break;
                }
                let reduce = excess.min(w.saturating_sub(1));
                *w = w.saturating_sub(reduce);
                excess -= reduce;
            }
        }
    }

    let border_style = Style::default();
    let header_style = Style::new().bold();

    render_table_border(
        ctx,
        &col_widths,
        border_style,
        box_chars::TOP_LEFT,
        box_chars::TOP_T,
        box_chars::TOP_RIGHT,
    );

    for row in &header_rows {
        render_table_data_row(ctx, row, &col_widths, header_style, border_style);
    }

    if !header_rows.is_empty() && !body_rows.is_empty() {
        render_table_border(
            ctx,
            &col_widths,
            border_style,
            box_chars::LEFT_T,
            box_chars::CROSS,
            box_chars::RIGHT_T,
        );
    }

    for (idx, row) in body_rows.iter().enumerate() {
        render_table_data_row(ctx, row, &col_widths, Style::default(), border_style);
        if idx < body_rows.len() - 1 {
            render_table_border(
                ctx,
                &col_widths,
                border_style,
                box_chars::LEFT_T,
                box_chars::CROSS,
                box_chars::RIGHT_T,
            );
        }
    }

    render_table_border(
        ctx,
        &col_widths,
        border_style,
        box_chars::BOTTOM_LEFT,
        box_chars::BOTTOM_T,
        box_chars::BOTTOM_RIGHT,
    );
    ctx.mark_block_end();
}

fn render_table_border(
    ctx: &mut RenderContext,
    col_widths: &[usize],
    style: Style,
    left: char,
    middle: char,
    right: char,
) {
    let mut line = String::new();
    line.push(left);
    for (i, &w) in col_widths.iter().enumerate() {
        line.push_str(&box_chars::HORIZONTAL.to_string().repeat(w + 2));
        if i < col_widths.len() - 1 {
            line.push(middle);
        }
    }
    line.push(right);
    ctx.push_block(styled(line, style));
}

fn render_table_data_row(
    ctx: &mut RenderContext,
    cells: &[String],
    col_widths: &[usize],
    content_style: Style,
    border_style: Style,
) {
    let num_cols = col_widths.len();

    let mut wrapped_cells: Vec<Vec<String>> = Vec::new();
    for (i, cell) in cells.iter().enumerate() {
        let w = col_widths.get(i).copied().unwrap_or(3);
        wrapped_cells.push(wrap_text(cell, w));
    }
    while wrapped_cells.len() < num_cols {
        wrapped_cells.push(vec![String::new()]);
    }

    let max_lines = wrapped_cells.iter().map(|c| c.len()).max().unwrap_or(1);

    for line_idx in 0..max_lines {
        let mut nodes: Vec<Node> = Vec::new();
        nodes.push(styled(box_chars::VERTICAL.to_string(), border_style));

        for (col_idx, wrapped) in wrapped_cells.iter().enumerate() {
            let w = col_widths.get(col_idx).copied().unwrap_or(3);
            let content = wrapped.get(line_idx).map(String::as_str).unwrap_or("");
            let content_width = visible_width(content);
            let padding_right = w.saturating_sub(content_width);

            nodes.push(styled(" ".to_string(), border_style));
            nodes.push(styled(content.to_string(), content_style));
            nodes.push(styled(" ".repeat(padding_right + 1), border_style));
            nodes.push(styled(box_chars::VERTICAL.to_string(), border_style));
        }

        ctx.push_block(row(nodes));
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    use textwrap::{wrap, Options, WordSplitter};
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    wrap(text, options)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect()
}

fn render_link(node: &markdown_it::Node, link: &Link, ctx: &mut RenderContext) {
    let link_text = extract_all_text(node);
    let display = if link_text.is_empty() {
        link.url.clone()
    } else {
        link_text
    };
    ctx.current_spans
        .push((display, Style::new().fg(Color::Blue).underline()));
}

fn heading_style(level: u8) -> Style {
    match level {
        1 => Style::new().fg(Color::Cyan).bold(),
        2 => Style::new().fg(Color::Blue).bold(),
        3 => Style::new().fg(Color::Magenta).bold(),
        _ => Style::new().bold(),
    }
}

fn extract_all_text(node: &markdown_it::Node) -> String {
    let mut result = String::new();
    if let Some(text) = node.cast::<Text>() {
        result.push_str(&text.content);
    }
    if node.cast::<Softbreak>().is_some() || node.cast::<Hardbreak>().is_some() {
        result.push('\n');
    }
    for child in node.children.iter() {
        result.push_str(&extract_all_text(child));
    }
    result
}

fn text_node(content: &str) -> Node {
    text(content)
}

fn normalize_br_tags(input: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    // (?i)<br\s*/?\s*> matches <br>, <br/>, <br />, <BR>, etc.
    // Replace with "  \n" (two trailing spaces = markdown Hardbreak)
    static BR_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<br\s*/?\s*>").expect("valid regex"));

    BR_REGEX.replace_all(input, "  \n").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::ink::render::render_to_string;

    #[test]
    fn test_plain_text() {
        let node = markdown_to_node("Hello world");
        let output = render_to_string(&node, 80);
        assert!(output.contains("Hello world"));
    }

    #[test]
    fn test_bold_text() {
        let node = markdown_to_node("This is **bold** text");
        let output = render_to_string(&node, 80);
        assert!(output.contains("bold"));
    }

    #[test]
    fn test_heading() {
        let node = markdown_to_node("# Heading 1\n\nSome text");
        let output = render_to_string(&node, 80);
        assert!(output.contains("Heading 1"));
    }

    #[test]
    fn test_code_block() {
        let node = markdown_to_node("```rust\nfn main() {}\n```");
        let output = render_to_string(&node, 80);
        assert!(output.contains("fn") && output.contains("main"));
    }

    #[test]
    fn test_bullet_list() {
        let node = markdown_to_node("- Item 1\n- Item 2\n- Item 3");
        let output = render_to_string(&node, 80);
        assert!(output.contains("Item 1"));
        assert!(output.contains("•"));
    }

    #[test]
    fn test_blockquote() {
        let node = markdown_to_node("> This is a quote");
        let output = render_to_string(&node, 80);
        assert!(output.contains("quote"));
        assert!(output.contains("│"));
    }

    #[test]
    fn test_table() {
        let node = markdown_to_node("| A | B |\n|---|---|\n| 1 | 2 |");
        let output = render_to_string(&node, 80);
        assert!(output.contains("A"));
        assert!(output.contains("B"));
        assert!(output.contains("1"));
        assert!(output.contains("2"));
        assert!(output.contains("┌"), "Should have top-left corner");
        assert!(output.contains("┐"), "Should have top-right corner");
        assert!(output.contains("└"), "Should have bottom-left corner");
        assert!(output.contains("┘"), "Should have bottom-right corner");
    }

    #[test]
    fn test_blank_lines_between_blocks() {
        let md = "# Heading\n\nParagraph one.\n\nParagraph two.";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(
            lines.len() >= 4,
            "Should have blank lines between blocks: {:?}",
            lines
        );
        assert!(
            lines.iter().any(|l| l.is_empty()),
            "Should have empty lines: {:?}",
            lines
        );
    }

    #[test]
    fn test_table_followed_by_paragraph() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n\nSome text after table.";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        assert!(output.contains("Some text after table"));
        let lines: Vec<&str> = output.split("\r\n").collect();
        let table_end = lines.iter().position(|l| l.contains("└")).unwrap();
        let text_pos = lines.iter().position(|l| l.contains("Some text")).unwrap();
        assert!(
            text_pos > table_end + 1,
            "Should have blank line between table and paragraph"
        );
    }

    #[test]
    fn test_link() {
        let node = markdown_to_node("[click here](https://example.com)");
        let output = render_to_string(&node, 80);
        assert!(output.contains("click here"));
    }

    fn assert_lines_fit_width(output: &str, max_width: usize) {
        for (i, line) in output.split("\r\n").enumerate() {
            let width = visible_width(line);
            assert!(
                width <= max_width,
                "Line {} exceeds width {}: {} chars\n{:?}",
                i + 1,
                max_width,
                width,
                line
            );
        }
    }

    #[test]
    fn table_respects_width_constraint() {
        let wide_table = r#"| Very Long Header Column | Another Long Header Column |
|---|---|
| This cell has lots of content | More content in this cell too |"#;

        let node = markdown_to_node_with_width(wide_table, 50);
        let output = render_to_string(&node, 50);

        assert_lines_fit_width(&output, 50);
        assert!(output.contains("┌"), "Should have table border");
        assert!(output.contains("┘"), "Should have table border");
    }

    #[test]
    fn table_with_very_narrow_width() {
        let table = "| Header | Data |\n|---|---|\n| Content | Value |";
        let node = markdown_to_node_with_width(table, 25);
        let output = render_to_string(&node, 25);

        assert_lines_fit_width(&output, 25);
    }

    #[test]
    fn table_multicolumn_fits_width() {
        let table = r#"| Col1 | Col2 | Col3 | Col4 |
|---|---|---|---|
| A | B | C | D |
| Long content here | More long content | Even more | Final |"#;

        let node = markdown_to_node_with_width(table, 60);
        let output = render_to_string(&node, 60);

        assert_lines_fit_width(&output, 60);
    }

    #[test]
    fn text_wraps_at_width_boundary() {
        let long_text = "This is a very long sentence that should wrap at the specified width boundary without breaking words unnecessarily.";
        let node = markdown_to_node_with_width(long_text, 40);
        let output = render_to_string(&node, 40);

        assert_lines_fit_width(&output, 40);

        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(lines.len() > 1, "Long text should wrap to multiple lines");
    }

    #[test]
    fn text_does_not_wrap_when_fits() {
        let short_text = "Short text that fits.";
        let node = markdown_to_node_with_width(short_text, 80);
        let output = render_to_string(&node, 80);

        let lines: Vec<&str> = output.split("\r\n").collect();
        assert_eq!(lines.len(), 1, "Short text should not wrap");
        assert!(output.contains("Short text that fits."));
    }

    #[test]
    fn styled_text_wraps_correctly() {
        let md = "This has **bold text** and *italic text* mixed together in a long sentence that needs to wrap.";
        let node = markdown_to_node_with_width(md, 40);
        let output = render_to_string(&node, 40);

        assert_lines_fit_width(&output, 40);
        assert!(output.contains("bold text"));
        assert!(output.contains("italic text"));
        assert!(output.contains("\x1b[1m"), "Should have bold ANSI code");
        assert!(output.contains("\x1b[3m"), "Should have italic ANSI code");
    }

    #[test]
    fn list_items_wrap_within_width() {
        let md = r#"- This is a very long list item that should wrap properly within the specified width constraint
- Another long item with lots of text that also needs to wrap correctly"#;

        let node = markdown_to_node_with_width(md, 50);
        let output = render_to_string(&node, 50);

        assert_lines_fit_width(&output, 50);
        assert!(output.contains("•"));
    }

    #[test]
    fn blockquote_wraps_within_width() {
        let md = "> This is a long blockquote that should wrap properly within the width constraint while maintaining the quote prefix.";
        let node = markdown_to_node_with_width(md, 40);
        let output = render_to_string(&node, 40);

        assert_lines_fit_width(&output, 40);
        assert!(output.contains("│"));
    }

    #[test]
    fn words_not_broken_unnecessarily() {
        let md = "Pneumonoultramicroscopicsilicovolcanoconiosis is a long word.";
        let node = markdown_to_node_with_width(md, 60);
        let output = render_to_string(&node, 60);

        assert!(
            output.contains("Pneumonoultramicroscopicsilicovolcanoconiosis"),
            "Long word should not be broken when it fits on a line"
        );
    }

    #[test]
    fn table_at_67_columns() {
        let table = r#"| Feature | Rust | Go |
|---------|------|-----|
| Memory | Safe | GC |
| Speed | Fast | Fast |"#;

        let node = markdown_to_node_with_width(table, 67);
        let output = render_to_string(&node, 67);
        assert_lines_fit_width(&output, 67);
    }

    #[test]
    fn table_forces_cell_wrap_at_67_columns() {
        let table = r#"| Command | Description | Example Usage |
|---------|-------------|---------------|
| search | Search through notes semantically | crucible search "query" |
| semantic | Semantic search with embeddings enabled | crucible semantic "concept" |
| note create | Create a new note in kiln | crucible note create path.md |"#;

        let node = markdown_to_node_with_width(table, 67);
        let output = render_to_string(&node, 67);
        assert_lines_fit_width(&output, 67);
    }

    #[test]
    fn table_with_bullet_prefix_at_67_columns() {
        use crate::tui::ink::node::{row, styled};
        use crate::tui::ink::style::{Color, Style};

        let table = r#"| Command | Description | Example Usage |
|---------|-------------|---------------|
| search | Search through notes semantically | crucible search "query" |
| semantic | Semantic search with embeddings enabled | crucible semantic "concept" |"#;

        let prefix_width = 2;
        let content_width = 67 - prefix_width;
        let md_node = markdown_to_node_with_width(table, content_width);
        let with_prefix = row([
            styled("● ".to_string(), Style::new().fg(Color::DarkGray)),
            md_node,
        ]);
        let output = render_to_string(&with_prefix, 67);
        assert_lines_fit_width(&output, 67);
    }

    #[test]
    fn br_tag_converts_to_newline() {
        let md = "Line one<br>Line two";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(lines.len() >= 2, "Should have multiple lines: {:?}", lines);
        assert!(output.contains("Line one"));
        assert!(output.contains("Line two"));
    }

    #[test]
    fn br_tag_variants_all_work() {
        for variant in ["<br>", "<br/>", "<br />", "<BR>", "<BR/>", "<Br />"] {
            let md = format!("A{}B", variant);
            let node = markdown_to_node(&md);
            let output = render_to_string(&node, 80);
            let lines: Vec<&str> = output.split("\r\n").collect();
            assert!(
                lines.len() >= 2,
                "Variant {} should create newline: {:?}",
                variant,
                lines
            );
        }
    }

    #[test]
    fn br_tag_in_table_cell() {
        let table = "| Header |\n|---|\n| First<br>Second |";
        let node = markdown_to_node(table);
        let output = render_to_string(&node, 80);
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
    }

    #[test]
    fn normalize_br_tags_function() {
        assert_eq!(super::normalize_br_tags("a<br>b"), "a  \nb");
        assert_eq!(super::normalize_br_tags("a<br/>b"), "a  \nb");
        assert_eq!(super::normalize_br_tags("a<br />b"), "a  \nb");
        assert_eq!(super::normalize_br_tags("a<BR>b"), "a  \nb");
        assert_eq!(super::normalize_br_tags("no tags here"), "no tags here");
        assert_eq!(
            super::normalize_br_tags("multi<br>line<br>text"),
            "multi  \nline  \ntext"
        );
    }
}
