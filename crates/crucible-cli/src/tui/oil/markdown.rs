//! Markdown to Oil Node renderer
//!
//! Parses markdown using markdown-it and converts to oil Node trees
//! for styled terminal rendering.
//!
//! # Render Styles
//!
//! Two render styles are supported:
//!
//! - **Viewport**: Content is pre-wrapped to fit the terminal width. Use for content
//!   that will be rendered in-place and redrawn (streaming, popups).
//!
//! - **Natural**: Text uses large width (terminal wraps), but tables use terminal width
//!   for correct column sizing. Use for graduated/scrollback content that won't be redrawn.

use crate::tui::oil::ansi::{visible_width, wrap_styled_text};
use crate::tui::oil::node::*;
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::ThemeTokens;
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
use once_cell::sync::Lazy;
use regex::Regex;

const NATURAL_TEXT_WIDTH: usize = 10000;

/// Regex to match HTML <br> tags in various forms: <br>, <br/>, <br />, <BR>, etc.
static BR_TAG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<br\s*/?\s*>").expect("valid regex"));

pub const ASSISTANT_BULLET: &str = " ● ";
pub const ASSISTANT_BULLET_WIDTH: usize = 3;
pub const CONTENT_PADDING: usize = ASSISTANT_BULLET_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margins {
    pub left: usize,
    pub right: usize,
    pub show_bullet: bool,
}

impl Margins {
    pub fn assistant() -> Self {
        Self {
            left: CONTENT_PADDING,
            right: CONTENT_PADDING,
            show_bullet: true,
        }
    }

    pub fn assistant_continuation() -> Self {
        Self {
            left: CONTENT_PADDING,
            right: CONTENT_PADDING,
            show_bullet: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    /// Pre-wrap all content to terminal width. For viewport/streaming content.
    Viewport { width: usize, margins: Margins },
    /// Pre-wrap to terminal width for consistent left/right alignment.
    /// For graduated/scrollback content.
    Natural {
        terminal_width: usize,
        margins: Margins,
    },
}

impl RenderStyle {
    pub fn viewport(width: usize) -> Self {
        RenderStyle::Viewport {
            width,
            margins: Margins::default(),
        }
    }

    pub fn viewport_with_margins(width: usize, margins: Margins) -> Self {
        RenderStyle::Viewport { width, margins }
    }

    pub fn natural(terminal_width: usize) -> Self {
        RenderStyle::Natural {
            terminal_width,
            margins: Margins::default(),
        }
    }

    pub fn natural_with_margins(terminal_width: usize, margins: Margins) -> Self {
        RenderStyle::Natural {
            terminal_width,
            margins,
        }
    }

    fn text_width(&self) -> usize {
        match self {
            RenderStyle::Viewport { width, margins }
            | RenderStyle::Natural {
                terminal_width: width,
                margins,
            } => width.saturating_sub(margins.left + margins.right),
        }
    }

    fn table_width(&self) -> usize {
        match self {
            RenderStyle::Viewport { width, margins } => {
                width.saturating_sub(margins.left + margins.right)
            }
            RenderStyle::Natural {
                terminal_width,
                margins,
            } => terminal_width.saturating_sub(margins.left + margins.right),
        }
    }

    fn margins(&self) -> Margins {
        match self {
            RenderStyle::Viewport { margins, .. } | RenderStyle::Natural { margins, .. } => {
                *margins
            }
        }
    }

    fn blockquote_width(&self) -> usize {
        match self {
            RenderStyle::Viewport { width, margins } => {
                width.saturating_sub(margins.left + margins.right)
            }
            RenderStyle::Natural {
                terminal_width,
                margins,
            } => terminal_width.saturating_sub(margins.left + margins.right),
        }
    }
}

/// Convert markdown text to an oil Node tree
pub fn markdown_to_node(markdown: &str) -> Node {
    let width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    markdown_to_node_with_width(markdown, width)
}

/// Convert markdown text to an oil Node tree with explicit width (viewport style)
pub fn markdown_to_node_with_width(markdown: &str, width: usize) -> Node {
    markdown_to_node_styled(markdown, RenderStyle::viewport(width))
}

/// Convert markdown with explicit render style
pub fn markdown_to_node_styled(markdown: &str, style: RenderStyle) -> Node {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let result = catch_unwind(AssertUnwindSafe(|| {
        let md = create_parser();
        let ast = md.parse(markdown);

        let mut ctx = RenderContext::new(
            style.text_width(),
            style.table_width(),
            style.blockquote_width(),
            style.margins(),
        );
        render_node(&ast, &mut ctx);
        ctx.into_node()
    }));

    result.unwrap_or_else(|_| text(markdown))
}

/// Convert markdown text to an oil Node tree with separate widths for text and tables.
/// Prefer `markdown_to_node_styled` for clearer intent.
pub fn markdown_to_node_with_widths(markdown: &str, text_width: usize, table_width: usize) -> Node {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let result = catch_unwind(AssertUnwindSafe(|| {
        let md = create_parser();
        let ast = md.parse(markdown);

        let mut ctx = RenderContext::new(text_width, table_width, table_width, Margins::default());
        render_node(&ast, &mut ctx);
        ctx.into_node()
    }));

    result.unwrap_or_else(|_| text(markdown))
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
    table_width: usize,
    blockquote_width: usize,
    margins: Margins,
    is_first_paragraph: bool,
}

impl RenderContext {
    fn new(width: usize, table_width: usize, blockquote_width: usize, margins: Margins) -> Self {
        Self {
            blocks: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![Style::default()],
            list_depth: 0,
            list_counter: None,
            needs_blank_line: false,
            width,
            table_width,
            blockquote_width,
            margins,
            is_first_paragraph: true,
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

    fn ensure_block_spacing(&mut self) {
        if self.needs_blank_line && !self.blocks.is_empty() && self.list_depth == 0 {
            self.blocks.push(text(""));
        }
        self.needs_blank_line = false;
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
        render_paragraph(node, ctx);
        return;
    }

    if let Some(heading) = node.cast::<ATXHeading>() {
        ctx.ensure_block_spacing();
        let style = heading_style(heading.level);
        let margins = ctx.margins;
        let has_margins = margins.left > 0 || margins.show_bullet;

        if !has_margins {
            ctx.push_style(style);
            render_children(node, ctx);
            ctx.pop_style();
            ctx.flush_line();
        } else {
            let heading_text = extract_all_text(node);
            let show_bullet = margins.show_bullet && ctx.is_first_paragraph;

            let prefix = if show_bullet {
                styled(
                    ASSISTANT_BULLET,
                    ThemeTokens::default_ref().bullet_prefix_style(),
                )
            } else {
                text(" ".repeat(margins.left))
            };
            ctx.blocks.push(row([prefix, styled(&heading_text, style)]));
            ctx.is_first_paragraph = false;
        }
        ctx.mark_block_end();
        return;
    }

    if node.cast::<CodeFence>().is_some() || node.cast::<MdCodeBlock>().is_some() {
        render_code_block(node, ctx);
        return;
    }

    if node.cast::<BulletList>().is_some() {
        let is_nested = ctx.list_depth > 0;
        ctx.ensure_block_spacing();
        ctx.list_depth += 1;
        ctx.list_counter = None;
        render_children(node, ctx);
        ctx.list_depth -= 1;
        if !is_nested {
            ctx.mark_block_end();
        }
        return;
    }

    if node.cast::<OrderedList>().is_some() {
        let is_nested = ctx.list_depth > 0;
        ctx.ensure_block_spacing();
        ctx.list_depth += 1;
        ctx.list_counter = Some(1);
        render_children(node, ctx);
        ctx.list_depth -= 1;
        ctx.list_counter = None;
        if !is_nested {
            ctx.mark_block_end();
        }
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

    if let Some(text_node) = node.cast::<Text>() {
        // Handle <br> tags in text content by splitting and flushing lines
        let content = &text_node.content;
        let content_lower = content.to_lowercase();
        if content_lower.contains("<br") {
            let parts: Vec<&str> = BR_TAG_REGEX.split(content).collect();
            for (i, part) in parts.iter().enumerate() {
                ctx.push_text(part);
                if i < parts.len() - 1 {
                    ctx.flush_line();
                }
            }
        } else {
            ctx.push_text(content);
        }
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
        ctx.current_spans.push((
            format!("`{}`", code_text),
            ThemeTokens::default_ref().inline_code(),
        ));
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

fn render_paragraph(node: &markdown_it::Node, ctx: &mut RenderContext) {
    ctx.ensure_block_spacing();

    let margins = ctx.margins;
    let has_margins = margins.left > 0 || margins.show_bullet;

    if !has_margins {
        render_children(node, ctx);
        ctx.flush_line();
        ctx.mark_block_end();
        return;
    }

    let para_text = extract_all_text(node);
    let wrapped = wrap_text(&para_text, ctx.width);

    let show_bullet = margins.show_bullet && ctx.is_first_paragraph;
    let indent = " ".repeat(margins.left);

    for (i, line) in wrapped.iter().enumerate() {
        let prefix = if i == 0 && show_bullet {
            styled(
                ASSISTANT_BULLET,
                ThemeTokens::default_ref().bullet_prefix_style(),
            )
        } else {
            text(&indent)
        };
        ctx.blocks.push(row([prefix, text_node(line)]));
    }

    ctx.is_first_paragraph = false;
    ctx.mark_block_end();
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

    ctx.ensure_block_spacing();

    let lang_str = lang.as_deref().unwrap_or("");
    let margins = ctx.margins;
    let indent = " ".repeat(margins.left);

    let fence_marker = if !lang_str.is_empty() {
        format!("```{}", lang_str)
    } else {
        "```".to_string()
    };

    let theme = ThemeTokens::default_ref();
    push_indented_block(
        ctx,
        styled(&fence_marker, theme.fence_marker_style()),
        &indent,
    );
    render_highlighted_code(&content, lang_str, ctx, &indent);
    push_indented_block(ctx, styled("```", theme.fence_marker_style()), &indent);

    ctx.mark_block_end();
}

fn push_indented_block(ctx: &mut RenderContext, node: Node, indent: &str) {
    ctx.flush_line();
    if indent.is_empty() {
        ctx.blocks.push(node);
    } else {
        ctx.blocks.push(row([text(indent), node]));
    }
}

fn render_highlighted_code(content: &str, lang: &str, ctx: &mut RenderContext, indent: &str) {
    use crate::formatting::SyntaxHighlighter;
    use crate::tui::oil::ansi::wrap_styled_text;

    if lang.is_empty() || !SyntaxHighlighter::supports_language(lang) {
        let fallback = ThemeTokens::default_ref().code_fallback_style();
        for line in content.lines() {
            let spans = vec![(line.to_string(), fallback.to_ansi_codes())];
            for wrapped in wrap_styled_text(&spans, ctx.width) {
                push_indented_block(ctx, text_node(&wrapped), indent);
            }
        }
        return;
    }

    let highlighter = SyntaxHighlighter::new();
    let highlighted_lines = highlighter.highlight(content, lang);

    for highlighted_line in highlighted_lines {
        if highlighted_line.spans.is_empty() {
            push_indented_block(ctx, text(""), indent);
            continue;
        }

        let spans: Vec<(String, String)> = highlighted_line
            .spans
            .iter()
            .map(|span| (span.text.clone(), span.style.to_ansi_codes()))
            .collect();

        for wrapped in wrap_styled_text(&spans, ctx.width) {
            push_indented_block(ctx, text_node(&wrapped), indent);
        }
    }
}

fn render_list_item(node: &markdown_it::Node, ctx: &mut RenderContext) {
    let margins = ctx.margins;
    let margin_indent = " ".repeat(margins.left);
    let list_indent = "  ".repeat(ctx.list_depth.saturating_sub(1));

    let (bullet, bullet_width) = if let Some(counter) = ctx.list_counter.as_mut() {
        let n = *counter;
        *counter += 1;
        let b = format!("{}. ", n);
        let w = b.len();
        (b, w)
    } else {
        ("• ".to_string(), 2)
    };

    let item_text = extract_list_item_text(node);
    let content_width = ctx.width.saturating_sub(bullet_width);
    let wrapped = wrap_text(&item_text, content_width);

    for (i, line) in wrapped.iter().enumerate() {
        if i == 0 {
            if margins.left > 0 {
                ctx.blocks.push(row([
                    text(&margin_indent),
                    text(&list_indent),
                    text(&bullet),
                    text_node(line),
                ]));
            } else {
                ctx.blocks
                    .push(row([text(&list_indent), text(&bullet), text_node(line)]));
            }
        } else {
            let continuation_indent = " ".repeat(bullet_width);
            if margins.left > 0 {
                ctx.blocks.push(row([
                    text(&margin_indent),
                    text(&list_indent),
                    text(&continuation_indent),
                    text_node(line),
                ]));
            } else {
                ctx.blocks.push(row([
                    text(&list_indent),
                    text(&continuation_indent),
                    text_node(line),
                ]));
            }
        }
    }

    for child in node.children.iter() {
        if child.cast::<BulletList>().is_some() || child.cast::<OrderedList>().is_some() {
            render_node(child, ctx);
        }
    }
}

fn extract_list_item_text(node: &markdown_it::Node) -> String {
    let mut result = String::new();
    extract_list_item_text_recursive(node, &mut result);
    result
}

fn extract_list_item_text_recursive(node: &markdown_it::Node, result: &mut String) {
    if node.cast::<BulletList>().is_some() || node.cast::<OrderedList>().is_some() {
        return;
    }
    if let Some(text) = node.cast::<Text>() {
        let content = BR_TAG_REGEX.replace_all(&text.content, "\n");
        result.push_str(&content);
    }
    if node.cast::<Softbreak>().is_some() || node.cast::<Hardbreak>().is_some() {
        result.push('\n');
    }
    for child in node.children.iter() {
        extract_list_item_text_recursive(child, result);
    }
}

fn render_blockquote(node: &markdown_it::Node, ctx: &mut RenderContext) {
    ctx.flush_line();
    ctx.ensure_block_spacing();

    let margins = ctx.margins;
    let margin_indent = " ".repeat(margins.left);
    let prefix = "│ ";
    let prefix_width = 2;
    let content_width = ctx
        .blockquote_width
        .saturating_sub(prefix_width + margins.left);

    for child in node.children.iter() {
        let child_text = extract_all_text(child);
        let wrapped = wrap_text(&child_text, content_width);
        for line in wrapped {
            let quote_row = row([
                styled(prefix, ThemeTokens::default_ref().blockquote_prefix_style()),
                styled(line, ThemeTokens::default_ref().blockquote_text_style()),
            ]);
            if margins.left > 0 {
                ctx.push_block(row([text(&margin_indent), quote_row]));
            } else {
                ctx.push_block(quote_row);
            }
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
    ctx.ensure_block_spacing();

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
    let max_content_width = ctx.table_width.saturating_sub(fixed_overhead);

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

    let margins = ctx.margins;
    if margins.left > 0 || margins.right > 0 {
        let left_pad = " ".repeat(margins.left);
        let right_pad = " ".repeat(margins.right);
        ctx.push_block(row([
            text(&left_pad),
            styled(line, style),
            text(&right_pad),
        ]));
    } else {
        ctx.push_block(styled(line, style));
    }
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
    let margins = ctx.margins;

    for line_idx in 0..max_lines {
        let mut nodes: Vec<Node> = Vec::new();

        if margins.left > 0 {
            nodes.push(text(" ".repeat(margins.left)));
        }

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

        if margins.right > 0 {
            nodes.push(text(" ".repeat(margins.right)));
        }

        ctx.push_block(row(nodes));
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    use textwrap::{wrap, Options, WordSplitter};

    fn split_at_break_chars(word: &str) -> Vec<usize> {
        const BREAK_AFTER: &[char] = &['-', '.', ',', ';', ':', '!', '?', ')', ']', '}', '/', '\\'];

        word.char_indices()
            .filter_map(|(idx, c)| {
                if BREAK_AFTER.contains(&c) {
                    Some(idx + c.len_utf8())
                } else {
                    None
                }
            })
            .filter(|&idx| idx < word.len())
            .collect()
    }

    let options = Options::new(width).word_splitter(WordSplitter::Custom(split_at_break_chars));

    // Handle explicit line breaks (from <br> tags) by splitting on newlines first,
    // wrapping each segment, then combining the results. This preserves intentional
    // line breaks within table cells.
    let mut result = Vec::new();
    for segment in text.split('\n') {
        let wrapped = wrap(segment, &options);
        for line in wrapped {
            result.push(line.into_owned());
        }
    }
    result
}

fn render_link(node: &markdown_it::Node, link: &Link, ctx: &mut RenderContext) {
    let link_text = extract_all_text(node);
    let display = if link_text.is_empty() {
        link.url.clone()
    } else {
        link_text
    };
    ctx.current_spans
        .push((display, ThemeTokens::default_ref().link_style()));
}

fn heading_style(level: u8) -> Style {
    let theme = ThemeTokens::default_ref();
    match level {
        1 => theme.heading_1_style(),
        2 => theme.heading_2_style(),
        3 => theme.heading_3_style(),
        _ => Style::new().bold(),
    }
}

fn extract_all_text(node: &markdown_it::Node) -> String {
    let mut result = String::new();
    if let Some(text) = node.cast::<Text>() {
        // Convert <br> tags to newlines within text content (for table cells)
        let content = BR_TAG_REGEX.replace_all(&text.content, "\n");
        result.push_str(&content);
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
    // Replace with "  \n" (two trailing spaces = markdown Hardbreak)
    BR_TAG_REGEX.replace_all(input, "  \n").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_string;

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

    #[test]
    fn test_nested_list_not_duplicated() {
        let md = "- Parent item\n  - Nested item one\n  - Nested item two";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        let count = output.matches("Nested item one").count();
        assert_eq!(
            count, 1,
            "Nested item should appear exactly once, but appeared {} times.\nOutput:\n{}",
            count, output
        );
    }

    #[test]
    fn test_nested_list_no_blank_line_after_parent() {
        let md = "- Parent item\n  - Nested item";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();

        let parent_idx = lines.iter().position(|l| l.contains("Parent")).unwrap();
        let nested_idx = lines.iter().position(|l| l.contains("Nested")).unwrap();

        assert_eq!(
            nested_idx,
            parent_idx + 1,
            "Nested list should immediately follow parent item (no blank line).\nLines: {:?}",
            lines
        );
    }

    #[test]
    fn test_paragraph_then_heading_has_blank_line() {
        let md = "Some paragraph.\n\n## A Heading";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();

        let para_idx = lines.iter().position(|l| l.contains("paragraph")).unwrap();
        let heading_idx = lines.iter().position(|l| l.contains("Heading")).unwrap();

        assert!(
            heading_idx > para_idx + 1,
            "Should have blank line between paragraph and heading.\nLines: {:?}",
            lines
        );
    }

    #[test]
    fn test_paragraph_then_heading_with_margins() {
        let md = "Some paragraph.\n\n## A Heading";
        let style = RenderStyle::natural_with_margins(80, Margins::assistant());
        let node = markdown_to_node_styled(md, style);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();

        let para_idx = lines.iter().position(|l| l.contains("paragraph")).unwrap();
        let heading_idx = lines.iter().position(|l| l.contains("Heading")).unwrap();

        assert!(
            heading_idx > para_idx + 1,
            "With margins: should have blank line between paragraph and heading.\nLines: {:?}",
            lines
        );
    }

    #[test]
    fn ordered_list_then_paragraph_has_blank_line() {
        let md = "1. Item one\n2. Item two\n\nFinal paragraph.";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();

        let last_item = lines.iter().rposition(|l| l.contains("Item two")).unwrap();
        let para = lines
            .iter()
            .position(|l| l.contains("Final paragraph"))
            .unwrap();

        assert!(
            para > last_item + 1,
            "Should have blank line between ordered list and paragraph.\nLines: {:?}",
            lines
        );
    }

    #[test]
    fn ordered_list_then_paragraph_has_blank_line_with_margins() {
        let md = "1. Item one\n2. Item two\n\nFinal paragraph.";
        let style = RenderStyle::natural_with_margins(80, Margins::assistant());
        let node = markdown_to_node_styled(md, style);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();

        let last_item = lines.iter().rposition(|l| l.contains("Item two")).unwrap();
        let para = lines
            .iter()
            .position(|l| l.contains("Final paragraph"))
            .unwrap();

        assert!(
            para > last_item + 1,
            "With margins: should have blank line between ordered list and paragraph.\nLines: {:?}",
            lines
        );
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
        use crate::tui::oil::node::{row, styled};
        use crate::tui::oil::style::{Color, Style};

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

    #[test]
    fn table_uses_table_width_not_text_width() {
        use super::markdown_to_node_with_widths;

        let table = "| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |";

        let node = markdown_to_node_with_widths(table, 10000, 50);
        let output = render_to_string(&node, 50);

        assert_lines_fit_width(&output, 50);
        assert!(output.contains("┌"), "Should have table border");
    }

    #[test]
    fn text_ignores_table_width() {
        use super::markdown_to_node_with_widths;

        let long_text = "This is text that should use the large text_width and not wrap early.";

        let node = markdown_to_node_with_widths(long_text, 10000, 20);
        let output = render_to_string(&node, 200);

        let lines: Vec<&str> = output.split("\r\n").collect();
        assert_eq!(lines.len(), 1, "Text should not wrap (text_width=10000)");
    }

    mod render_style_tests {
        use super::*;
        use crate::tui::oil::markdown::{markdown_to_node_styled, RenderStyle, NATURAL_TEXT_WIDTH};

        #[test]
        fn render_style_viewport_widths() {
            let style = RenderStyle::viewport(80);
            assert_eq!(style.text_width(), 80);
            assert_eq!(style.table_width(), 80);
        }

        #[test]
        fn render_style_natural_widths() {
            let style = RenderStyle::natural(80);
            assert_eq!(style.text_width(), 80);
            assert_eq!(style.table_width(), 80);
        }

        #[test]
        fn viewport_style_wraps_text_at_width() {
            let long_text = "This is a long paragraph that should wrap at the viewport width boundary when using viewport style rendering.";
            let style = RenderStyle::viewport(40);
            let node = markdown_to_node_styled(long_text, style);
            let output = render_to_string(&node, 40);

            assert_lines_fit_width(&output, 40);
            let lines: Vec<&str> = output.split("\r\n").collect();
            assert!(lines.len() > 1, "Viewport style should wrap text");
        }

        #[test]
        fn natural_style_wraps_text_at_terminal_width() {
            let long_text = "This is a long paragraph that should wrap when using natural style rendering for consistent left edge alignment.";
            let style = RenderStyle::natural(40);
            let node = markdown_to_node_styled(long_text, style);
            let output = render_to_string(&node, 40);

            let lines: Vec<&str> = output.split("\r\n").collect();
            assert!(lines.len() > 1, "Natural style should now wrap text");
        }

        #[test]
        fn viewport_style_table_fits_width() {
            let table = "| Header A | Header B | Header C |\n|----------|----------|----------|\n| Cell 1   | Cell 2   | Cell 3   |";
            let style = RenderStyle::viewport(50);
            let node = markdown_to_node_styled(table, style);
            let output = render_to_string(&node, 50);

            assert_lines_fit_width(&output, 50);
        }

        #[test]
        fn natural_style_table_fits_terminal_width() {
            let table = "| Header A | Header B | Header C |\n|----------|----------|----------|\n| Cell 1   | Cell 2   | Cell 3   |";
            let style = RenderStyle::natural(50);
            let node = markdown_to_node_styled(table, style);
            let output = render_to_string(&node, 50);

            assert_lines_fit_width(&output, 50);
        }

        #[test]
        fn natural_style_mixed_content_table_fits() {
            let md = "This paragraph uses natural text width.\n\n| A | B |\n|---|---|\n| 1 | 2 |";
            let style = RenderStyle::natural(40);
            let node = markdown_to_node_styled(md, style);
            let output = render_to_string(&node, 200);

            for line in output.lines() {
                if line.contains('┌') || line.contains('│') || line.contains('└') {
                    let width = visible_width(line);
                    assert!(
                        width <= 40,
                        "Table line should fit terminal width: {} > 40",
                        width
                    );
                }
            }

            assert!(
                output.contains("natural text width"),
                "Paragraph content should be present"
            );
        }

        #[test]
        fn constructor_helpers_work() {
            let v = RenderStyle::viewport(100);
            let n = RenderStyle::natural(100);

            assert!(matches!(v, RenderStyle::Viewport { width: 100, .. }));
            assert!(matches!(
                n,
                RenderStyle::Natural {
                    terminal_width: 100,
                    ..
                }
            ));
        }

        #[test]
        fn render_style_equality() {
            assert_eq!(RenderStyle::viewport(80), RenderStyle::viewport(80));
            assert_ne!(RenderStyle::viewport(80), RenderStyle::viewport(100));
            assert_ne!(RenderStyle::viewport(80), RenderStyle::natural(80));
            assert_eq!(RenderStyle::natural(80), RenderStyle::natural(80));
        }
    }

    mod syntax_highlighting {
        use super::*;

        #[test]
        fn rust_code_block_has_multiple_colors() {
            let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            // Syntax highlighting should produce multiple distinct RGB color codes
            // RGB escape sequence pattern: \x1b[38;2;R;G;B;m
            let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
            let matches: Vec<_> = rgb_pattern.find_iter(&output).collect();

            assert!(
                matches.len() >= 3,
                "Rust code should have at least 3 different color codes for keywords, \
                 strings, and identifiers. Got {} matches in output:\n{}",
                matches.len(),
                output.escape_debug()
            );
        }

        #[test]
        fn python_code_block_has_highlighting() {
            let md = "```python\ndef hello():\n    print(\"world\")\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
            let has_rgb_colors = rgb_pattern.is_match(&output);

            assert!(
                has_rgb_colors,
                "Python code should have RGB syntax colors. Output:\n{}",
                output.escape_debug()
            );
        }

        #[test]
        fn unknown_language_still_renders() {
            let md = "```unknownlang\nsome code here\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            assert!(output.contains("some code here"));
        }

        #[test]
        fn code_block_without_language_renders() {
            let md = "```\nplain code\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            assert!(output.contains("plain code"));
        }

        #[test]
        fn javascript_code_block_has_highlighting() {
            let md = "```js\nconst x = 42;\nfunction test() { return x; }\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
            assert!(
                rgb_pattern.is_match(&output),
                "JavaScript code should have syntax highlighting"
            );
        }

        #[test]
        fn highlighted_code_preserves_content() {
            let md = "```rust\nlet answer = 42;\n```";
            let node = markdown_to_node(md);
            let output = render_to_string(&node, 80);

            assert!(output.contains("let"), "Should contain 'let' keyword");
            assert!(output.contains("answer"), "Should contain variable name");
            assert!(output.contains("42"), "Should contain number literal");
        }
    }

    #[test]
    fn markdown_with_bullet_and_list_does_not_panic() {
        let md = "● I don't have the ability to look directly into your local file system, but I can definitely help you understand the structure and contents of a repo if you share them with me. Here's what you can do:\n\n   1. Copy the folder tree";

        let result = std::panic::catch_unwind(|| markdown_to_node(md));

        assert!(
            result.is_ok(),
            "Should not panic on bullet with apostrophe and list"
        );
    }
}
