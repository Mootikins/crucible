//! Terminal markdown renderer using markdown-it
//!
//! Renders markdown text with ANSI escape codes for terminal display.
//! Uses the markdown-it crate for parsing and crossterm for styling.

use crossterm::style::{Attribute, Color, Stylize};
use markdown_it::plugins::cmark::block::blockquote::Blockquote;
use markdown_it::plugins::cmark::block::code::CodeBlock as MdCodeBlock;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::hr::ThematicBreak;
use markdown_it::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::backticks::CodeInline;
use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};
use markdown_it::plugins::cmark::inline::link::Link;
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::plugins::extra::tables::{Table, TableBody, TableCell, TableHead, TableRow};
use markdown_it::parser::inline::Text;
use markdown_it::{MarkdownIt, Node};

/// Render markdown text to ANSI-styled terminal output
///
/// Parses the input markdown and applies terminal styling:
/// - Headers: bold + cyan coloring (H1 brightest, H6 dimmest)
/// - Code blocks: dimmed with language indicator
/// - Inline code: dimmed
/// - Bold: bold styling
/// - Italic: italic styling
/// - Lists: indented with bullet points
/// - Blockquotes: cyan "│" prefix
/// - Links: underlined with URL
pub fn render_markdown(text: &str) -> String {
    let md = create_parser();
    let ast = md.parse(text);
    render_node(&ast, &RenderContext::default())
}

/// Create a markdown-it parser with CommonMark and GFM table plugins
fn create_parser() -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    // Add GFM table support
    markdown_it::plugins::extra::tables::add(&mut md);
    // Note: Don't add syntect plugin - it overrides code blocks
    md
}

/// Context for rendering, tracks nesting and list state
#[derive(Default, Clone)]
struct RenderContext {
    /// Current indentation level
    indent: usize,
    /// Whether we're inside a blockquote
    in_blockquote: bool,
}

impl RenderContext {
    fn indented(&self) -> Self {
        Self {
            indent: self.indent + 1,
            ..self.clone()
        }
    }

    fn in_blockquote(&self) -> Self {
        Self {
            in_blockquote: true,
            ..self.clone()
        }
    }

    fn indent_prefix(&self) -> String {
        "  ".repeat(self.indent)
    }
}

/// Render a node and its children to styled text
fn render_node(node: &Node, ctx: &RenderContext) -> String {
    let mut output = String::new();

    // Handle different node types
    if node.is::<markdown_it::parser::core::Root>() {
        // Root node - just render children
        for child in node.children.iter() {
            output.push_str(&render_node(child, ctx));
        }
    } else if let Some(heading) = node.cast::<ATXHeading>() {
        // Headings with level-based styling
        let text = collect_text(node);
        let styled = match heading.level {
            1 => format!(
                "\n{}\n",
                text.with(Color::Cyan)
                    .attribute(Attribute::Bold)
                    .attribute(Attribute::Underlined)
            ),
            2 => format!("\n{}\n", text.with(Color::Cyan).attribute(Attribute::Bold)),
            3 => format!("\n{}\n", text.with(Color::Cyan)),
            _ => format!("\n{}\n", text.attribute(Attribute::Bold)),
        };
        output.push_str(&styled);
    } else if let Some(fence) = node.cast::<CodeFence>() {
        // Fenced code block
        let lang_display = if fence.info.is_empty() {
            String::new()
        } else {
            format!(
                " [{}]",
                fence.info.split_whitespace().next().unwrap_or("")
            )
        };
        if !lang_display.is_empty() {
            output.push_str(&format!("\n{}\n", lang_display.with(Color::DarkGrey)));
        } else {
            output.push('\n');
        }
        // Indent and dim each line of code
        for line in fence.content.lines() {
            output.push_str(&format!("  {}\n", line.with(Color::DarkGrey)));
        }
    } else if let Some(code) = node.cast::<MdCodeBlock>() {
        // Indented code block
        output.push('\n');
        for line in code.content.lines() {
            output.push_str(&format!("  {}\n", line.with(Color::DarkGrey)));
        }
    } else if node.cast::<Blockquote>().is_some() {
        // Blockquote with "│" prefix
        let inner_ctx = ctx.in_blockquote();
        for child in node.children.iter() {
            let child_text = render_node(child, &inner_ctx);
            for line in child_text.lines() {
                output.push_str(&format!("{} {}\n", "│".with(Color::Cyan), line));
            }
        }
    } else if node.cast::<BulletList>().is_some() {
        // Unordered list
        let inner_ctx = ctx.indented();
        for child in node.children.iter() {
            if child.cast::<ListItem>().is_some() {
                let item_text = collect_inline_text(child);
                output.push_str(&format!("{}• {}\n", inner_ctx.indent_prefix(), item_text));
            }
        }
    } else if node.cast::<OrderedList>().is_some() {
        // Ordered list
        let inner_ctx = ctx.indented();
        let mut num = 1;
        for child in node.children.iter() {
            if child.cast::<ListItem>().is_some() {
                let item_text = collect_inline_text(child);
                output.push_str(&format!(
                    "{}{}. {}\n",
                    inner_ctx.indent_prefix(),
                    num,
                    item_text
                ));
                num += 1;
            }
        }
    } else if node.cast::<Paragraph>().is_some() {
        // Paragraph - render inline content
        let text = render_inline_children(node);
        if ctx.in_blockquote {
            output.push_str(&text);
        } else {
            output.push_str(&format!("{}\n", text));
        }
    } else if node.cast::<ThematicBreak>().is_some() {
        // Horizontal rule
        output.push_str(&format!(
            "\n{}\n",
            "─".repeat(40).with(Color::DarkGrey)
        ));
    } else if node.cast::<Table>().is_some() {
        // GFM Table - render as formatted table
        output.push_str(&render_table(node));
    } else {
        // Default: render children
        for child in node.children.iter() {
            output.push_str(&render_node(child, ctx));
        }
    }

    output
}

/// Render inline children with styling
fn render_inline_children(node: &Node) -> String {
    let mut output = String::new();

    for child in node.children.iter() {
        output.push_str(&render_inline(child));
    }

    output
}

/// Render inline elements with styling
fn render_inline(node: &Node) -> String {
    if let Some(text) = node.cast::<Text>() {
        return text.content.clone();
    }

    if node.cast::<Strong>().is_some() {
        let inner = collect_inline_text(node);
        return inner.attribute(Attribute::Bold).to_string();
    }

    if node.cast::<Em>().is_some() {
        let inner = collect_inline_text(node);
        return inner.attribute(Attribute::Italic).to_string();
    }

    if node.cast::<CodeInline>().is_some() {
        // CodeInline content is in child Text nodes
        let inner = collect_text(node);
        return inner.with(Color::Yellow).to_string();
    }

    if let Some(link) = node.cast::<Link>() {
        let text = collect_inline_text(node);
        let url = &link.url;
        if text == *url {
            return text.attribute(Attribute::Underlined).to_string();
        } else {
            return format!(
                "{} ({})",
                text.attribute(Attribute::Underlined),
                url.clone().with(Color::DarkGrey)
            );
        }
    }

    if node.cast::<Hardbreak>().is_some() {
        return "\n".to_string();
    }

    if node.cast::<Softbreak>().is_some() {
        return " ".to_string();
    }

    // Default: recursively render children
    let mut output = String::new();
    for child in node.children.iter() {
        output.push_str(&render_inline(child));
    }
    output
}

/// Collect plain text from a node tree (no styling)
fn collect_text(node: &Node) -> String {
    let mut text = String::new();

    if let Some(t) = node.cast::<Text>() {
        text.push_str(&t.content);
    }

    for child in node.children.iter() {
        text.push_str(&collect_text(child));
    }

    text
}

/// Collect inline text with styling applied
fn collect_inline_text(node: &Node) -> String {
    render_inline_children(node)
}

/// Render a GFM table with proper formatting (fully outlined box)
fn render_table(node: &Node) -> String {
    let mut header_rows: Vec<Vec<String>> = Vec::new();
    let mut body_rows: Vec<Vec<String>> = Vec::new();

    // Collect rows from TableHead and TableBody sections
    for section in node.children.iter() {
        let is_header = section.cast::<TableHead>().is_some();
        let is_body = section.cast::<TableBody>().is_some();

        if is_header || is_body {
            for row_node in section.children.iter() {
                if row_node.cast::<TableRow>().is_some() {
                    let mut row_cells: Vec<String> = Vec::new();
                    for cell_node in row_node.children.iter() {
                        if cell_node.cast::<TableCell>().is_some() {
                            let cell_text = collect_inline_text(cell_node);
                            row_cells.push(cell_text);
                        }
                    }
                    if is_header {
                        header_rows.push(row_cells);
                    } else {
                        body_rows.push(row_cells);
                    }
                }
            }
        }
    }

    if header_rows.is_empty() && body_rows.is_empty() {
        return String::new();
    }

    // Combine all rows for column width calculation
    let all_rows: Vec<&Vec<String>> = header_rows.iter().chain(body_rows.iter()).collect();

    // Calculate column widths (max width per column)
    let num_cols = all_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for row in &all_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(cell.chars().count());
            }
        }
    }

    // Ensure minimum column width
    for w in &mut col_widths {
        *w = (*w).max(3);
    }

    let mut output = String::new();
    output.push('\n');

    // Build top border: ┌───┬───┐
    let top_border = format!(
        "┌{}┐",
        col_widths
            .iter()
            .map(|w| "─".repeat(*w + 2))
            .collect::<Vec<_>>()
            .join("┬")
    );
    output.push_str(&format!("{}\n", top_border));

    // Render header rows with bold content
    for row in &header_rows {
        let mut cells: Vec<String> = Vec::new();
        for (i, cell) in row.iter().enumerate() {
            let width = col_widths.get(i).copied().unwrap_or(3);
            let padded = format!("{:width$}", cell, width = width);
            // Bold header cell content
            cells.push(padded.attribute(Attribute::Bold).to_string());
        }
        // Pad missing cells
        for i in row.len()..num_cols {
            let width = col_widths.get(i).copied().unwrap_or(3);
            cells.push(" ".repeat(width));
        }

        let row_content = cells.join(" │ ");
        output.push_str(&format!("│ {} │\n", row_content));
    }

    // Separator line after header: ├───┼───┤
    if !header_rows.is_empty() && !body_rows.is_empty() {
        let mid_border = format!(
            "├{}┤",
            col_widths
                .iter()
                .map(|w| "─".repeat(*w + 2))
                .collect::<Vec<_>>()
                .join("┼")
        );
        output.push_str(&format!("{}\n", mid_border));
    }

    // Render body rows
    for row in &body_rows {
        let mut cells: Vec<String> = Vec::new();
        for (i, cell) in row.iter().enumerate() {
            let width = col_widths.get(i).copied().unwrap_or(3);
            let padded = format!("{:width$}", cell, width = width);
            cells.push(padded);
        }
        // Pad missing cells
        for i in row.len()..num_cols {
            let width = col_widths.get(i).copied().unwrap_or(3);
            cells.push(" ".repeat(width));
        }

        let row_content = cells.join(" │ ");
        output.push_str(&format!("│ {} │\n", row_content));
    }

    // Build bottom border: └───┴───┘
    let bottom_border = format!(
        "└{}┘",
        col_widths
            .iter()
            .map(|w| "─".repeat(*w + 2))
            .collect::<Vec<_>>()
            .join("┴")
    );
    output.push_str(&format!("{}\n", bottom_border));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_heading() {
        let result = render_markdown("# Hello World");
        assert!(result.contains("Hello World"));
    }

    #[test]
    fn test_render_code_block() {
        let result = render_markdown("```rust\nfn main() {}\n```");
        // Code block content will have ANSI escapes, check for key parts
        assert!(result.contains("main"), "Should contain 'main'");
        assert!(result.contains("rust"), "Should contain 'rust'");
    }

    #[test]
    fn test_render_bullet_list() {
        let result = render_markdown("- Item 1\n- Item 2");
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
        assert!(result.contains("•"));
    }

    #[test]
    fn test_render_ordered_list() {
        let result = render_markdown("1. First\n2. Second");
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_render_bold() {
        let result = render_markdown("This is **bold** text");
        assert!(result.contains("bold"));
    }

    #[test]
    fn test_render_inline_code() {
        let result = render_markdown("Use `code` here");
        assert!(result.contains("code"));
    }

    #[test]
    fn test_render_blockquote() {
        let result = render_markdown("> Quote text");
        assert!(result.contains("Quote text"));
        assert!(result.contains("│"));
    }

    #[test]
    fn test_render_link() {
        let result = render_markdown("[Click here](https://example.com)");
        assert!(result.contains("Click here"));
        assert!(result.contains("example.com"));
    }

    #[test]
    fn test_render_plain_text() {
        let result = render_markdown("Just plain text");
        assert!(result.contains("Just plain text"));
    }

    #[test]
    fn test_render_horizontal_rule() {
        let result = render_markdown("---");
        assert!(result.contains("─"));
    }

    #[test]
    fn test_render_table() {
        let input = r#"| Name | Age |
|------|-----|
| Alice | 30 |
| Bob | 25 |"#;
        let result = render_markdown(input);
        // Table should contain headers and data
        assert!(result.contains("Name"), "Table should contain 'Name' header");
        assert!(result.contains("Age"), "Table should contain 'Age' header");
        assert!(result.contains("Alice"), "Table should contain 'Alice' data");
        assert!(result.contains("Bob"), "Table should contain 'Bob' data");
        assert!(result.contains("30"), "Table should contain '30' data");
        assert!(result.contains("25"), "Table should contain '25' data");
        // Should have separator line with box-drawing chars
        assert!(result.contains("─"), "Table should have horizontal separator");
        assert!(result.contains("│"), "Table should have vertical separator");
    }

    #[test]
    fn test_render_table_single_row() {
        let input = r#"| Header1 | Header2 |
|---------|---------|
| Data1   | Data2   |"#;
        let result = render_markdown(input);
        assert!(result.contains("Header1"));
        assert!(result.contains("Header2"));
        assert!(result.contains("Data1"));
        assert!(result.contains("Data2"));
    }
}
