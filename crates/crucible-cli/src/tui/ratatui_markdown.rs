//! Ratatui-native markdown renderer
//!
//! Renders markdown text to ratatui `Line<'static>` and `Span<'static>` types
//! for direct use with ratatui `Paragraph` widgets. Uses markdown-it for parsing
//! and `MarkdownTheme` for styling.
//!
//! # Example
//!
//! ```no_run
//! use crucible_cli::tui::ratatui_markdown::RatatuiMarkdown;
//! use crucible_cli::tui::theme::MarkdownTheme;
//!
//! let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
//! let lines = renderer.render("# Hello\n\nThis is **bold** text.");
//! // Use `lines` directly with ratatui::widgets::Paragraph
//! ```

use markdown_it::parser::inline::Text;
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
use markdown_it::{MarkdownIt, Node};
use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use regex::Regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::Theme;
use syntect::parsing::SyntaxSet;

use super::theme::{MarkdownElement, MarkdownTheme};

// =============================================================================
// Static Syntax Set (expensive to load, so cache globally)
// =============================================================================

/// Global syntax set for code highlighting (loaded once at startup)
static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

// =============================================================================
// Custom syntax patterns (wikilinks, tags, agent mentions)
// =============================================================================

static WIKILINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap());

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([a-zA-Z][a-zA-Z0-9_/-]*)").unwrap());

static AGENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@([a-zA-Z][a-zA-Z0-9_-]*)").unwrap());

static CALLOUT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\[!(\w+)\][ \t]*([^\n]*)?").unwrap());

#[derive(Debug, Clone)]
enum CustomSyntax {
    Wikilink {
        target: String,
        alias: Option<String>,
    },
    Tag(String),
    AgentMention(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalloutType {
    Note,
    Tip,
    Warning,
    Danger,
}

impl CalloutType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "note" | "info" | "abstract" | "summary" | "tldr" => Some(Self::Note),
            "tip" | "hint" | "success" | "check" | "done" | "example" => Some(Self::Tip),
            "warning" | "caution" | "attention" | "important" | "question" | "help" | "faq" => {
                Some(Self::Warning)
            }
            "danger" | "error" | "failure" | "fail" | "missing" | "bug" => Some(Self::Danger),
            _ => Some(Self::Note),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Note => "ℹ",
            Self::Tip => "✓",
            Self::Warning => "⚠",
            Self::Danger => "✕",
        }
    }

    fn element(self) -> MarkdownElement {
        match self {
            Self::Note => MarkdownElement::CalloutNote,
            Self::Tip => MarkdownElement::CalloutTip,
            Self::Warning => MarkdownElement::CalloutWarning,
            Self::Danger => MarkdownElement::CalloutDanger,
        }
    }
}

fn parse_custom_syntax(text: &str) -> Vec<(usize, usize, CustomSyntax)> {
    let mut matches = Vec::new();

    for cap in WIKILINK_RE.captures_iter(text) {
        let m = cap.get(0).unwrap();
        let target = cap.get(1).unwrap().as_str().to_string();
        let alias = cap.get(2).map(|a| a.as_str().to_string());
        matches.push((m.start(), m.end(), CustomSyntax::Wikilink { target, alias }));
    }

    for cap in TAG_RE.captures_iter(text) {
        let m = cap.get(0).unwrap();
        let tag = cap.get(1).unwrap().as_str().to_string();
        matches.push((m.start(), m.end(), CustomSyntax::Tag(tag)));
    }

    for cap in AGENT_RE.captures_iter(text) {
        let m = cap.get(0).unwrap();
        let agent = cap.get(1).unwrap().as_str().to_string();
        matches.push((m.start(), m.end(), CustomSyntax::AgentMention(agent)));
    }

    matches.sort_by_key(|(start, _, _)| *start);

    let mut filtered = Vec::new();
    let mut last_end = 0;
    for (start, end, syntax) in matches {
        if start >= last_end {
            filtered.push((start, end, syntax));
            last_end = end;
        }
    }

    filtered
}

fn emit_custom_syntax_spans<F>(text: &str, base_style: Style, theme: &MarkdownTheme, mut emit: F)
where
    F: FnMut(String, Style),
{
    let matches = parse_custom_syntax(text);

    if matches.is_empty() {
        emit(text.to_string(), base_style);
        return;
    }

    let mut pos = 0;
    for (start, end, syntax) in matches {
        if pos < start {
            emit(text[pos..start].to_string(), base_style);
        }

        match syntax {
            CustomSyntax::Wikilink { target, alias } => {
                let style = theme.style_for(MarkdownElement::Wikilink);
                let display = alias.as_ref().unwrap_or(&target);
                emit(format!("[[{}]]", display), style);
            }
            CustomSyntax::Tag(tag) => {
                let style = theme.style_for(MarkdownElement::Tag);
                emit(format!("#{}", tag), style);
            }
            CustomSyntax::AgentMention(agent) => {
                let style = theme.style_for(MarkdownElement::AgentMention);
                emit(format!("@{}", agent), style);
            }
        }

        pos = end;
    }

    if pos < text.len() {
        emit(text[pos..].to_string(), base_style);
    }
}

fn extract_first_text(node: &Node) -> Option<String> {
    if let Some(text) = node.cast::<Text>() {
        return Some(text.content.clone());
    }
    for child in node.children.iter() {
        if let Some(text) = extract_first_text(child) {
            return Some(text);
        }
    }
    None
}

fn extract_all_text(node: &Node) -> String {
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

fn detect_callout(node: &Node) -> Option<(CalloutType, Option<String>)> {
    if let Some(first_child) = node.children.first() {
        if first_child.cast::<Paragraph>().is_some() {
            if let Some(text) = extract_first_text(first_child) {
                if let Some(caps) = CALLOUT_RE.captures(&text) {
                    let callout_type = caps
                        .get(1)
                        .and_then(|m| CalloutType::from_str(m.as_str()))?;
                    let title = caps
                        .get(2)
                        .map(|m| m.as_str().trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    return Some((callout_type, title));
                }
            }
        }
    }
    None
}

// =============================================================================
// Box-drawing characters for tables
// =============================================================================

mod box_chars {
    pub const TOP_LEFT: char = '\u{250C}'; // ┌
    pub const TOP_RIGHT: char = '\u{2510}'; // ┐
    pub const BOTTOM_LEFT: char = '\u{2514}'; // └
    pub const BOTTOM_RIGHT: char = '\u{2518}'; // ┘
    pub const HORIZONTAL: char = '\u{2500}'; // ─
    pub const VERTICAL: char = '\u{2502}'; // │
    pub const TOP_T: char = '\u{252C}'; // ┬
    pub const BOTTOM_T: char = '\u{2534}'; // ┴
    pub const LEFT_T: char = '\u{251C}'; // ├
    pub const RIGHT_T: char = '\u{2524}'; // ┤
    pub const CROSS: char = '\u{253C}'; // ┼
}

/// Ratatui-native markdown renderer.
///
/// Parses markdown using markdown-it and produces ratatui `Line<'static>` values
/// that can be used directly with `Paragraph` widgets.
pub struct RatatuiMarkdown {
    /// Theme for styling markdown elements
    theme: MarkdownTheme,
    /// Optional width constraint for word wrapping
    width: Option<usize>,
}

impl RatatuiMarkdown {
    /// Create a new renderer with the given theme.
    pub fn new(theme: MarkdownTheme) -> Self {
        Self { theme, width: None }
    }

    /// Set the width constraint for word wrapping.
    ///
    /// When set, paragraphs will be wrapped at word boundaries to fit within
    /// the specified width.
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = Some(width);
        self
    }

    /// Render markdown text to ratatui lines.
    ///
    /// Returns a vector of `Line<'static>` that can be passed directly to
    /// a ratatui `Paragraph` widget.
    pub fn render(&self, markdown: &str) -> Vec<Line<'static>> {
        let md = create_parser();
        let ast = md.parse(markdown);

        let mut ctx = RenderContext::new(
            &self.theme,
            self.width,
            &SYNTAX_SET,
            self.theme.syntect_theme(),
        );
        render_node(&ast, &mut ctx);
        ctx.into_lines()
    }

    /// Get a reference to the underlying theme.
    pub fn theme(&self) -> &MarkdownTheme {
        &self.theme
    }
}

/// Create a markdown-it parser with CommonMark and GFM table plugins.
fn create_parser() -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::tables::add(&mut md);
    md
}

/// Context for rendering, tracks state during AST traversal.
struct RenderContext<'a> {
    /// Theme for styling
    theme: &'a MarkdownTheme,
    /// Width constraint for wrapping
    width: Option<usize>,
    /// Syntax definitions for code highlighting
    syntax_set: &'a SyntaxSet,
    /// Syntect theme for code highlighting
    syntect_theme: &'a Theme,
    /// Accumulated output lines
    lines: Vec<Line<'static>>,
    /// Current line being built (spans)
    current_spans: Vec<Span<'static>>,
    /// Current indentation level
    indent: usize,
    /// Whether we're inside a blockquote
    in_blockquote: bool,
    /// Ordered list counter (None if not in ordered list)
    list_counter: Option<usize>,
}

impl<'a> RenderContext<'a> {
    fn new(
        theme: &'a MarkdownTheme,
        width: Option<usize>,
        syntax_set: &'a SyntaxSet,
        syntect_theme: &'a Theme,
    ) -> Self {
        Self {
            theme,
            width,
            syntax_set,
            syntect_theme,
            lines: Vec::new(),
            current_spans: Vec::new(),
            indent: 0,
            in_blockquote: false,
            list_counter: None,
        }
    }

    /// Flush the current spans as a completed line.
    fn flush_line(&mut self) {
        if !self.current_spans.is_empty() {
            let spans = std::mem::take(&mut self.current_spans);
            self.lines.push(Line::from(spans));
        }
    }

    /// Add a blank line (for paragraph separation).
    fn add_blank_line(&mut self) {
        self.flush_line();
        self.lines.push(Line::from(""));
    }

    /// Push a span with the given style onto the current line.
    fn push_span(&mut self, text: String, style: Style) {
        if !text.is_empty() {
            self.current_spans.push(Span::styled(text, style));
        }
    }

    fn push_text(&mut self, text: &str) {
        let base_style = self.theme.style_for(MarkdownElement::Text);
        emit_custom_syntax_spans(text, base_style, self.theme, |t, s| self.push_span(t, s));
    }

    /// Get the indent prefix string.
    fn indent_prefix(&self) -> String {
        "  ".repeat(self.indent)
    }

    /// Convert accumulated state into final lines vector.
    fn into_lines(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        self.lines
    }

    /// Render a code block with syntax highlighting.
    ///
    /// Uses syntect to highlight code based on the language tag.
    /// Falls back to plain text if the language is not recognized.
    fn render_code_block(&mut self, code: &str, lang: &str) {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, self.syntect_theme);

        for line in code.lines() {
            match highlighter.highlight_line(line, self.syntax_set) {
                Ok(ranges) => {
                    let spans: Vec<Span<'static>> = ranges
                        .into_iter()
                        .map(|(style, text)| {
                            let fg = Color::Rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            );
                            Span::styled(text.to_string(), Style::default().fg(fg))
                        })
                        .collect();
                    if spans.is_empty() {
                        self.lines.push(Line::from(""));
                    } else {
                        self.lines.push(Line::from(spans));
                    }
                }
                Err(_) => {
                    // Fallback: plain text with code style
                    let style = self.theme.style_for(MarkdownElement::InlineCode);
                    self.lines
                        .push(Line::from(Span::styled(line.to_string(), style)));
                }
            }
        }
    }
}

/// Render a node and its children.
fn render_node(node: &Node, ctx: &mut RenderContext<'_>) {
    // Handle root node
    if node.is::<markdown_it::parser::core::Root>() {
        for child in node.children.iter() {
            render_node(child, ctx);
        }
        return;
    }

    // Headings
    if let Some(heading) = node.cast::<ATXHeading>() {
        ctx.flush_line();
        // Add blank line before heading (unless at start)
        if !ctx.lines.is_empty() {
            ctx.add_blank_line();
        }

        let element = match heading.level {
            1 => MarkdownElement::Heading1,
            2 => MarkdownElement::Heading2,
            3 => MarkdownElement::Heading3,
            4 => MarkdownElement::Heading4,
            5 => MarkdownElement::Heading5,
            _ => MarkdownElement::Heading6,
        };

        let style = ctx.theme.style_for(element);
        let text = collect_text(node);
        ctx.push_span(text, style);
        ctx.flush_line();
        return;
    }

    // Code fence (```code```)
    if let Some(fence) = node.cast::<CodeFence>() {
        ctx.flush_line();

        // Extract language for syntax highlighting (don't display it as a label)
        let lang = if !fence.info.is_empty() {
            fence.info.split_whitespace().next().unwrap_or("")
        } else {
            ""
        };

        // Code content with syntax highlighting
        ctx.render_code_block(&fence.content, lang);
        return;
    }

    // Indented code block
    if let Some(code) = node.cast::<MdCodeBlock>() {
        ctx.flush_line();
        // Indented code blocks have no language, use plain text highlighting
        ctx.render_code_block(&code.content, "");
        return;
    }

    // Blockquote (including callouts)
    if node.cast::<Blockquote>().is_some() {
        ctx.flush_line();
        if !ctx.lines.is_empty() {
            ctx.add_blank_line();
        }
        let old_in_blockquote = ctx.in_blockquote;
        ctx.in_blockquote = true;

        let callout = detect_callout(node);
        let start_line = ctx.lines.len();

        if let Some((callout_type, title)) = &callout {
            let style = ctx.theme.style_for(callout_type.element());
            let header = match title {
                Some(t) => format!("{} {}", callout_type.icon(), t),
                None => format!("{} {:?}", callout_type.icon(), callout_type),
            };
            ctx.push_span(header, style);
            ctx.flush_line();

            let mut is_first_para = true;
            for child in node.children.iter() {
                if is_first_para && child.cast::<Paragraph>().is_some() {
                    is_first_para = false;
                    let full_text = extract_all_text(child);
                    if CALLOUT_RE.is_match(&full_text) {
                        let remaining = CALLOUT_RE.replace(&full_text, "");
                        let remaining = remaining.trim();
                        if !remaining.is_empty() {
                            for line in remaining.lines() {
                                ctx.push_span(
                                    line.to_string(),
                                    ctx.theme.style_for(MarkdownElement::Text),
                                );
                                ctx.flush_line();
                            }
                        }
                        continue;
                    }
                }
                render_node(child, ctx);
            }
        } else {
            for child in node.children.iter() {
                render_node(child, ctx);
            }
        }
        ctx.flush_line();

        let (border_style, content_italic) = if let Some((callout_type, _)) = callout {
            (ctx.theme.style_for(callout_type.element()), false)
        } else {
            (ctx.theme.style_for(MarkdownElement::Blockquote), true)
        };

        for line in ctx.lines[start_line..].iter_mut() {
            let mut new_spans = vec![Span::styled("  │ ".to_owned(), border_style)];
            if content_italic {
                for span in line.spans.iter_mut() {
                    span.style = span.style.add_modifier(Modifier::ITALIC);
                }
            }
            new_spans.append(&mut line.spans.clone());
            *line = Line::from(new_spans);
        }

        ctx.in_blockquote = old_in_blockquote;
        return;
    }

    // Bullet list
    if node.cast::<BulletList>().is_some() {
        ctx.flush_line();
        let old_indent = ctx.indent;

        for child in node.children.iter() {
            if child.cast::<ListItem>().is_some() {
                render_list_item(child, ctx, "-");
            }
        }

        ctx.indent = old_indent;
        return;
    }

    // Ordered list
    if node.cast::<OrderedList>().is_some() {
        ctx.flush_line();
        let old_indent = ctx.indent;
        let old_counter = ctx.list_counter;
        ctx.indent += 1;
        ctx.list_counter = Some(1);

        for child in node.children.iter() {
            if child.cast::<ListItem>().is_some() {
                let num = ctx.list_counter.unwrap_or(1);
                render_list_item(child, ctx, &format!("{}.", num));
                ctx.list_counter = Some(num + 1);
            }
        }

        ctx.indent = old_indent;
        ctx.list_counter = old_counter;
        return;
    }

    // Paragraph
    if node.cast::<Paragraph>().is_some() {
        ctx.flush_line();

        // Add blank line before paragraph (unless at start or in blockquote)
        if !ctx.lines.is_empty() && !ctx.in_blockquote {
            // Check if previous line is not already blank
            if let Some(last) = ctx.lines.last() {
                if !last.spans.is_empty() {
                    ctx.add_blank_line();
                }
            }
        }

        render_inline_children_with_wrapping(node, ctx);
        ctx.flush_line();
        return;
    }

    // Horizontal rule
    if node.cast::<ThematicBreak>().is_some() {
        ctx.flush_line();
        let style = ctx.theme.style_for(MarkdownElement::HorizontalRule);
        let width = ctx.width.unwrap_or(40);
        ctx.push_span("-".repeat(width.min(40)), style);
        ctx.flush_line();
        return;
    }

    // Table rendering
    if node.cast::<Table>().is_some() {
        ctx.flush_line();
        render_table(node, ctx);
        return;
    }

    // Default: render children
    for child in node.children.iter() {
        render_node(child, ctx);
    }
}

/// Render a list item with proper prefix and indentation.
///
/// Handles both inline content (text, bold, etc.) AND block-level elements
/// (code fences, nested lists, paragraphs). The prefix is added to the first
/// line of content, with continuation lines properly indented.
fn render_list_item(node: &Node, ctx: &mut RenderContext<'_>, prefix: &str) {
    let prefix_style = ctx.theme.style_for(MarkdownElement::ListMarker);
    let prefix_width = display_width(prefix);
    let continuation_indent = " ".repeat(prefix_width);

    // Check if all children are inline-only (no block elements)
    let has_block_children = node.children.iter().any(|child| {
        child.cast::<MdCodeBlock>().is_some()
            || child.cast::<CodeFence>().is_some()
            || child.cast::<BulletList>().is_some()
            || child.cast::<OrderedList>().is_some()
            || child.cast::<Blockquote>().is_some()
            || child.cast::<Table>().is_some()
    });

    if has_block_children {
        // Block content: render children normally, then prefix each line
        let item_start = ctx.lines.len();

        for child in node.children.iter() {
            render_node(child, ctx);
        }

        ctx.flush_line();
        let item_end = ctx.lines.len();

        if item_start < item_end {
            for (idx, line) in ctx.lines[item_start..item_end].iter_mut().enumerate() {
                if idx == 0 {
                    let mut new_spans = vec![Span::styled(prefix.to_string(), prefix_style)];
                    new_spans.append(&mut line.spans.clone());
                    *line = Line::from(new_spans);
                } else {
                    let mut new_spans = vec![Span::raw(continuation_indent.clone())];
                    new_spans.append(&mut line.spans.clone());
                    *line = Line::from(new_spans);
                }
            }
        }
    } else {
        // Inline-only content: collect spans and wrap with prefix
        let mut temp_spans: Vec<Span<'static>> = Vec::new();
        {
            let mut temp_ctx = TempSpanCollector::new(&mut temp_spans);
            collect_inline_spans(node, ctx, &mut temp_ctx);
        }

        // Wrap spans with prefix
        wrap_list_item_spans(
            &temp_spans,
            prefix,
            prefix_style,
            ctx.width.unwrap_or(80),
            ctx,
        );
    }
}

/// Render inline children of a node (for paragraphs, list items, etc.)
fn render_inline_children(node: &Node, ctx: &mut RenderContext<'_>) {
    for child in node.children.iter() {
        render_inline(child, ctx);
    }
}

/// Render inline children with word wrapping support.
///
/// Renders inline content (text, bold, italic, etc.) with word-aware line breaking.
/// When a width constraint is set, text will wrap at word boundaries to fit within
/// the specified width. Styling is preserved across line breaks.
fn render_inline_children_with_wrapping(node: &Node, ctx: &mut RenderContext<'_>) {
    let Some(max_width) = ctx.width else {
        // No width constraint, just render inline without wrapping
        render_inline_children(node, ctx);
        return;
    };

    // Collect all inline spans first
    let mut temp_spans: Vec<Span<'static>> = Vec::new();
    {
        let mut temp_ctx = TempSpanCollector::new(&mut temp_spans);
        collect_inline_spans(node, ctx, &mut temp_ctx);
    }

    // Now wrap the collected spans with word-aware line breaking
    wrap_spans_to_lines(&temp_spans, max_width, ctx);
}

/// Trait for collecting styled spans during inline rendering.
///
/// This abstracts over the different ways inline content can be collected:
/// - Directly to the render context (for non-wrapping paths)
/// - To a temporary buffer (for word-wrapping)
trait SpanSink {
    fn push_span(&mut self, text: String, style: Style);
}

/// Temporary collector for spans during inline rendering.
struct TempSpanCollector<'a> {
    spans: &'a mut Vec<Span<'static>>,
}

impl<'a> TempSpanCollector<'a> {
    fn new(spans: &'a mut Vec<Span<'static>>) -> Self {
        Self { spans }
    }
}

impl SpanSink for TempSpanCollector<'_> {
    fn push_span(&mut self, text: String, style: Style) {
        if !text.is_empty() {
            self.spans.push(Span::styled(text, style));
        }
    }
}

/// Collect inline spans without adding them to the context lines.
fn collect_inline_spans(
    node: &Node,
    ctx: &RenderContext<'_>,
    collector: &mut TempSpanCollector<'_>,
) {
    for child in node.children.iter() {
        process_inline_node(child, ctx, collector, true);
    }
}

/// Process a single inline element, pushing spans to the sink.
///
/// This unified function handles all inline markdown elements. The `for_wrapping`
/// parameter controls hard break behavior:
/// - `true`: emit "\n" span (for word-wrapping collection)
/// - `false`: flush line directly (for direct rendering)
fn process_inline_node<S: SpanSink>(
    node: &Node,
    ctx: &RenderContext<'_>,
    sink: &mut S,
    for_wrapping: bool,
) {
    if let Some(text) = node.cast::<Text>() {
        let base_style = ctx.theme.style_for(MarkdownElement::Text);
        emit_custom_syntax_spans(&text.content, base_style, ctx.theme, |t, s| {
            sink.push_span(t, s);
        });
        return;
    }

    // Strong (bold)
    if node.cast::<Strong>().is_some() {
        let element = if has_nested_node::<Em>(node) {
            MarkdownElement::BoldItalic
        } else {
            MarkdownElement::Bold
        };
        let style = ctx.theme.style_for(element);
        let text = collect_text(node);
        sink.push_span(text, style);
        return;
    }

    // Emphasis (italic)
    if node.cast::<Em>().is_some() {
        let element = if has_nested_node::<Strong>(node) {
            MarkdownElement::BoldItalic
        } else {
            MarkdownElement::Italic
        };
        let style = ctx.theme.style_for(element);
        let text = collect_text(node);
        sink.push_span(text, style);
        return;
    }

    // Inline code
    if node.cast::<CodeInline>().is_some() {
        let style = ctx.theme.style_for(MarkdownElement::InlineCode);
        let text = collect_text(node);
        sink.push_span(text, style);
        return;
    }

    // Link
    if let Some(link) = node.cast::<Link>() {
        let style = ctx.theme.style_for(MarkdownElement::Link);
        let text = collect_text(node);

        if text == link.url {
            sink.push_span(text, style);
        } else {
            sink.push_span(text, style);
            let url_style = ctx
                .theme
                .style_for(MarkdownElement::Text)
                .add_modifier(Modifier::DIM);
            sink.push_span(format!(" ({})", link.url), url_style);
        }
        return;
    }

    // Soft break (treat as space)
    if node.cast::<Softbreak>().is_some() {
        let style = ctx.theme.style_for(MarkdownElement::Text);
        sink.push_span(" ".to_string(), style);
        return;
    }

    // Hard break
    if node.cast::<Hardbreak>().is_some() {
        if for_wrapping {
            // Mark with newline that will be processed during wrapping
            let style = ctx.theme.style_for(MarkdownElement::Text);
            sink.push_span("\n".to_string(), style);
        }
        // Note: non-wrapping path handles hard breaks by flushing context directly
        return;
    }

    // Default: recurse into children
    for child in node.children.iter() {
        process_inline_node(child, ctx, sink, for_wrapping);
    }
}

/// Wrap collected spans across lines, respecting word boundaries.
///
/// This function takes a flat list of styled spans and outputs them as wrapped lines,
/// breaking at word boundaries while preserving the styling of each piece of text.
fn wrap_spans_to_lines(spans: &[Span<'static>], max_width: usize, ctx: &mut RenderContext<'_>) {
    if spans.is_empty() {
        return;
    }

    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut current_line_width: usize = 0;

    for span in spans {
        let text = span.content.as_ref();
        let style = span.style;

        // Handle hard breaks (newlines in the content)
        if text == "\n" {
            // Flush current line and start a new one
            if !current_line_spans.is_empty() {
                ctx.lines
                    .push(Line::from(std::mem::take(&mut current_line_spans)));
            } else {
                ctx.lines.push(Line::from(""));
            }
            current_line_width = 0;
            continue;
        }

        // Process text word by word
        let mut remaining = text;
        while !remaining.is_empty() {
            // Find the next word boundary
            let (word, rest, _has_trailing_space) = next_word(remaining);
            remaining = rest;

            if word.is_empty() {
                continue;
            }

            let word_width = display_width(word);

            // Check if word fits on current line
            let fits_on_line =
                current_line_width == 0 || current_line_width + 1 + word_width <= max_width;

            if !fits_on_line && current_line_width > 0 {
                // Word doesn't fit, flush current line
                ctx.lines
                    .push(Line::from(std::mem::take(&mut current_line_spans)));
                current_line_width = 0;
            }

            // Add word to current line
            let word_text = if current_line_width > 0 && !current_line_spans.is_empty() {
                // Add leading space if not at start of line
                format!(" {}", word)
            } else {
                word.to_string()
            };

            let actual_width = display_width(&word_text);
            current_line_spans.push(Span::styled(word_text, style));
            current_line_width += actual_width;
        }
    }

    // Flush remaining spans
    if !current_line_spans.is_empty() {
        ctx.lines.push(Line::from(current_line_spans));
    }
}

/// Wrap spans for a list item with proper indentation.
///
/// First line uses the bullet/number prefix, continuation lines use spaces
/// to align with the content (after the bullet).
fn wrap_list_item_spans(
    spans: &[Span<'static>],
    prefix: &str,
    prefix_style: Style,
    max_width: usize,
    ctx: &mut RenderContext<'_>,
) {
    if spans.is_empty() {
        // Empty list item - just output the prefix
        ctx.lines
            .push(Line::from(Span::styled(prefix.to_string(), prefix_style)));
        return;
    }

    let prefix_width = display_width(prefix) + 1; // +1 for the space after prefix
    let content_width = max_width.saturating_sub(prefix_width);
    let continuation_indent = " ".repeat(prefix_width);

    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut current_line_width: usize = 0;
    let mut is_first_line = true;

    for span in spans {
        let text = span.content.as_ref();
        let style = span.style;

        // Handle hard breaks (newlines in the content)
        if text == "\n" {
            // Flush current line
            let line = if is_first_line {
                is_first_line = false;
                let mut line_spans = vec![Span::styled(format!("{} ", prefix), prefix_style)];
                line_spans.extend(std::mem::take(&mut current_line_spans));
                Line::from(line_spans)
            } else {
                let mut line_spans = vec![Span::raw(continuation_indent.clone())];
                line_spans.extend(std::mem::take(&mut current_line_spans));
                Line::from(line_spans)
            };
            ctx.lines.push(line);
            current_line_width = 0;
            continue;
        }

        // Process text word by word
        let mut remaining = text;
        while !remaining.is_empty() {
            let (word, rest, _) = next_word(remaining);
            remaining = rest;

            if word.is_empty() {
                continue;
            }

            let word_width = display_width(word);

            // Check if word fits on current line
            let fits =
                current_line_width == 0 || current_line_width + 1 + word_width <= content_width;

            if !fits && current_line_width > 0 {
                // Flush current line
                let line = if is_first_line {
                    is_first_line = false;
                    let mut line_spans = vec![Span::styled(format!("{} ", prefix), prefix_style)];
                    line_spans.extend(std::mem::take(&mut current_line_spans));
                    Line::from(line_spans)
                } else {
                    let mut line_spans = vec![Span::raw(continuation_indent.clone())];
                    line_spans.extend(std::mem::take(&mut current_line_spans));
                    Line::from(line_spans)
                };
                ctx.lines.push(line);
                current_line_width = 0;
            }

            // Add word to current line
            let word_text = if current_line_width > 0 && !current_line_spans.is_empty() {
                format!(" {}", word)
            } else {
                word.to_string()
            };

            current_line_spans.push(Span::styled(word_text.clone(), style));
            current_line_width += display_width(&word_text);
        }
    }

    // Flush remaining content
    if !current_line_spans.is_empty() || is_first_line {
        let line = if is_first_line {
            let mut line_spans = vec![Span::styled(format!("{} ", prefix), prefix_style)];
            line_spans.extend(current_line_spans);
            Line::from(line_spans)
        } else {
            let mut line_spans = vec![Span::raw(continuation_indent)];
            line_spans.extend(current_line_spans);
            Line::from(line_spans)
        };
        ctx.lines.push(line);
    }
}

/// Extract the next word from a string, returning (word, rest, has_trailing_space).
fn next_word(s: &str) -> (&str, &str, bool) {
    // Skip leading whitespace
    let s = s.trim_start();
    if s.is_empty() {
        return ("", "", false);
    }

    // Find end of word (next whitespace)
    let word_end = s.find(char::is_whitespace).unwrap_or(s.len());
    let word = &s[..word_end];
    let rest = &s[word_end..];

    // Check for trailing space
    let has_trailing_space = rest.starts_with(char::is_whitespace);
    let rest = rest.trim_start();

    (word, rest, has_trailing_space)
}

/// Render an inline element directly to the context.
///
/// This is the non-wrapping path that renders inline elements directly.
/// Hard breaks trigger immediate line flush (unlike the wrapping path).
fn render_inline(node: &Node, ctx: &mut RenderContext<'_>) {
    // Plain text
    if let Some(text) = node.cast::<Text>() {
        ctx.push_text(&text.content);
        return;
    }

    // Strong (bold)
    if node.cast::<Strong>().is_some() {
        let element = if has_nested_node::<Em>(node) {
            MarkdownElement::BoldItalic
        } else {
            MarkdownElement::Bold
        };
        let style = ctx.theme.style_for(element);
        let text = collect_text(node);
        ctx.push_span(text, style);
        return;
    }

    // Emphasis (italic)
    if node.cast::<Em>().is_some() {
        let element = if has_nested_node::<Strong>(node) {
            MarkdownElement::BoldItalic
        } else {
            MarkdownElement::Italic
        };
        let style = ctx.theme.style_for(element);
        let text = collect_text(node);
        ctx.push_span(text, style);
        return;
    }

    // Inline code
    if node.cast::<CodeInline>().is_some() {
        let style = ctx.theme.style_for(MarkdownElement::InlineCode);
        let text = collect_text(node);
        ctx.push_span(text, style);
        return;
    }

    // Link
    if let Some(link) = node.cast::<Link>() {
        let style = ctx.theme.style_for(MarkdownElement::Link);
        let text = collect_text(node);

        if text == link.url {
            ctx.push_span(text, style);
        } else {
            ctx.push_span(text, style);
            let url_style = ctx
                .theme
                .style_for(MarkdownElement::Text)
                .add_modifier(Modifier::DIM);
            ctx.push_span(format!(" ({})", link.url), url_style);
        }
        return;
    }

    // Hard break
    if node.cast::<Hardbreak>().is_some() {
        ctx.flush_line();
        return;
    }

    // Soft break (treat as space)
    if node.cast::<Softbreak>().is_some() {
        ctx.push_text(" ");
        return;
    }

    // Default: recursively render children
    for child in node.children.iter() {
        render_inline(child, ctx);
    }
}

/// Collect plain text from a node tree (no styling).
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

/// Check if a node has a nested child of type T.
fn has_nested_node<T: markdown_it::NodeValue>(node: &Node) -> bool {
    for child in node.children.iter() {
        if child.cast::<T>().is_some() {
            return true;
        }
        if has_nested_node::<T>(child) {
            return true;
        }
    }
    false
}

// =============================================================================
// Table Rendering
// =============================================================================

/// Calculate display width of a string using Unicode width algorithm.
///
/// Properly handles CJK characters (2 cells), combining characters, and emojis.
pub(crate) fn display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Wrap text to fit within a given width, returning lines.
///
/// Uses word-level wrapping: lines break at word boundaries.
/// If a single word is longer than the column width, it is broken mid-word
/// to ensure the line fits within the width constraint.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    if display_width(text) <= width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = display_width(word);

        if current_width == 0 {
            // First word on line
            if word_width <= width {
                // Word fits
                current_line = word.to_string();
                current_width = word_width;
            } else {
                // Word too long - break it
                let broken = break_word(word, width);
                for (i, part) in broken.into_iter().enumerate() {
                    if i > 0 || !current_line.is_empty() {
                        lines.push(std::mem::take(&mut current_line));
                    }
                    current_line = part.clone();
                    current_width = display_width(&part);
                }
            }
        } else if current_width + 1 + word_width <= width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Need to wrap - push current line and handle this word
            lines.push(std::mem::take(&mut current_line));

            if word_width <= width {
                // Word fits on new line
                current_line = word.to_string();
                current_width = word_width;
            } else {
                // Word too long - break it
                let broken = break_word(word, width);
                for (i, part) in broken.into_iter().enumerate() {
                    if i > 0 {
                        lines.push(std::mem::take(&mut current_line));
                    }
                    current_line = part.clone();
                    current_width = display_width(&part);
                }
            }
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

/// Break a word into chunks that fit within the given width.
fn break_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![word.to_string()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for c in word.chars() {
        let char_width = 1; // Simple char count

        if current_width + char_width > width && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            current_width = 0;
        }

        current.push(c);
        current_width += char_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        parts.push(String::new());
    }

    parts
}

/// Render a GFM table with box-drawing borders.
///
/// The table is parsed from the markdown-it AST and rendered with:
/// - Top border (┌─┬─┐)
/// - Row separators (├─┼─┤)
/// - Bottom border (└─┴─┘)
/// - 1 space padding in cells
/// - Bold styling for header row content
fn render_table(node: &Node, ctx: &mut RenderContext<'_>) {
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
                            let cell_text = collect_text(cell_node);
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
        return;
    }

    // Combine all rows for column width calculation
    let all_rows: Vec<&Vec<String>> = header_rows.iter().chain(body_rows.iter()).collect();

    // Calculate number of columns
    let num_cols = all_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return;
    }

    // Calculate initial column widths (max content width per column)
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

    // If width is specified and table is too wide, shrink columns proportionally
    // but never below the minimum width needed for the longest word
    if let Some(max_w) = ctx.width {
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
                                #[allow(clippy::cast_precision_loss)]
                                let shrink_amount =
                                    (shrinkable as f64 / shrinkable_total as f64 * excess as f64)
                                        .ceil() as usize;
                                let new_width = w.saturating_sub(shrink_amount);
                                *w = new_width.max(min_col_widths[i]);
                            }
                        }
                    }
                } else if available_content < total_min {
                    // Even minimums don't fit - force-shrink columns proportionally.
                    // This will break words, but ensures all content is visible.
                    // Each column gets at least 3 chars (absolute minimum).
                    let absolute_min = 3usize;
                    let num_columns = col_widths.len();
                    let absolute_total = num_columns * absolute_min;

                    if available_content >= absolute_total {
                        // Distribute available space proportionally based on original widths
                        let extra = available_content - absolute_total;
                        let total_original: usize = col_widths.iter().sum();

                        for w in col_widths.iter_mut() {
                            // Each column gets absolute_min plus a proportional share of extra
                            #[allow(clippy::cast_precision_loss)]
                            let proportion = *w as f64 / total_original as f64;
                            let extra_for_col = (proportion * extra as f64).floor() as usize;
                            *w = absolute_min + extra_for_col;
                        }
                    } else {
                        // Can't even fit absolute minimums - use them anyway
                        for w in col_widths.iter_mut() {
                            *w = absolute_min;
                        }
                    }
                }
            }
        }
    }

    let border_style = ctx.theme.style_for(MarkdownElement::TableBorder);
    let header_style = ctx.theme.style_for(MarkdownElement::Bold);
    let text_style = ctx.theme.style_for(MarkdownElement::Text);

    // Add blank line before table (unless at start)
    if !ctx.lines.is_empty() {
        ctx.add_blank_line();
    }

    // Top border: ┌─────┬─────┬─────┐
    render_table_top_border(ctx, &col_widths, num_cols, border_style);

    // Render header rows
    for row in &header_rows {
        render_table_data_row(ctx, row, &col_widths, num_cols, header_style, border_style);
    }

    // Header separator (if we have headers and data)
    if !header_rows.is_empty() && !body_rows.is_empty() {
        render_table_separator_row(ctx, &col_widths, num_cols, border_style);
    }

    // Render data rows with separators between them
    for (idx, row) in body_rows.iter().enumerate() {
        render_table_data_row(ctx, row, &col_widths, num_cols, text_style, border_style);

        // Add separator between data rows (but not after the last one)
        if idx < body_rows.len() - 1 {
            render_table_separator_row(ctx, &col_widths, num_cols, border_style);
        }
    }

    // Bottom border: └─────┴─────┴─────┘
    render_table_bottom_border(ctx, &col_widths, num_cols, border_style);
}

/// Render a horizontal table border row.
///
/// This function renders borders like:
/// - Top: ┌─────┬─────┐
/// - Separator: ├─────┼─────┤
/// - Bottom: └─────┴─────┘
fn render_table_border_row(
    ctx: &mut RenderContext<'_>,
    col_widths: &[usize],
    num_cols: usize,
    style: Style,
    left: char,
    middle: char,
    right: char,
) {
    let mut spans = Vec::new();

    spans.push(Span::styled(left.to_string(), style));
    for (i, &w) in col_widths.iter().enumerate() {
        spans.push(Span::styled(
            box_chars::HORIZONTAL.to_string().repeat(w + 2),
            style,
        ));
        if i < num_cols - 1 {
            spans.push(Span::styled(middle.to_string(), style));
        }
    }
    spans.push(Span::styled(right.to_string(), style));

    ctx.lines.push(Line::from(spans));
}

/// Render the top border of a table: ┌─────┬─────┐
fn render_table_top_border(
    ctx: &mut RenderContext<'_>,
    col_widths: &[usize],
    num_cols: usize,
    style: Style,
) {
    render_table_border_row(
        ctx,
        col_widths,
        num_cols,
        style,
        box_chars::TOP_LEFT,
        box_chars::TOP_T,
        box_chars::TOP_RIGHT,
    );
}

/// Render a separator row: ├─────┼─────┤
fn render_table_separator_row(
    ctx: &mut RenderContext<'_>,
    col_widths: &[usize],
    num_cols: usize,
    style: Style,
) {
    render_table_border_row(
        ctx,
        col_widths,
        num_cols,
        style,
        box_chars::LEFT_T,
        box_chars::CROSS,
        box_chars::RIGHT_T,
    );
}

/// Render the bottom border of a table: └─────┴─────┘
fn render_table_bottom_border(
    ctx: &mut RenderContext<'_>,
    col_widths: &[usize],
    num_cols: usize,
    style: Style,
) {
    render_table_border_row(
        ctx,
        col_widths,
        num_cols,
        style,
        box_chars::BOTTOM_LEFT,
        box_chars::BOTTOM_T,
        box_chars::BOTTOM_RIGHT,
    );
}

/// Render a data row (potentially wrapping cells if needed)
fn render_table_data_row(
    ctx: &mut RenderContext<'_>,
    row: &[String],
    col_widths: &[usize],
    num_cols: usize,
    content_style: Style,
    border_style: Style,
) {
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
        let mut spans = Vec::new();

        spans.push(Span::styled(box_chars::VERTICAL.to_string(), border_style));

        for (col_idx, wrapped) in wrapped_cells.iter().enumerate() {
            let w = col_widths.get(col_idx).copied().unwrap_or(3);
            let content = wrapped.get(line_idx).map(String::as_str).unwrap_or("");
            let content_width = display_width(content);
            let padding_right = w.saturating_sub(content_width);

            // Left padding
            spans.push(Span::styled(" ".to_string(), border_style));
            // Content
            spans.push(Span::styled(content.to_string(), content_style));
            // Right padding
            spans.push(Span::styled(" ".repeat(padding_right), border_style));
            // Right padding (after content)
            spans.push(Span::styled(" ".to_string(), border_style));
            // Vertical border
            spans.push(Span::styled(box_chars::VERTICAL.to_string(), border_style));
        }

        ctx.lines.push(Line::from(spans));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_plain_text() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("Hello world");
        assert_eq!(lines.len(), 1);
        // Should contain the text
        let line_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(line_text.contains("Hello world"));
    }

    #[test]
    fn renders_bold_with_modifier() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("**bold**");
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::BOLD)),
            "Should have BOLD modifier"
        );
    }

    #[test]
    fn renders_italic_with_modifier() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("*italic*");
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::ITALIC)),
            "Should have ITALIC modifier"
        );
    }

    #[test]
    fn renders_bold_italic_with_both_modifiers() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("***bold italic***");
        let has_both = lines[0].spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::BOLD)
                && s.style.add_modifier.contains(Modifier::ITALIC)
        });
        assert!(has_both, "Should have both BOLD and ITALIC modifiers");
    }

    #[test]
    fn renders_multiple_paragraphs() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("Para 1\n\nPara 2");
        // Should have at least 2 content lines (with blank line between)
        assert!(lines.len() >= 2, "Should have multiple lines");
        // Check both paragraphs are present
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Para 1"));
        assert!(all_text.contains("Para 2"));
    }

    #[test]
    fn renders_heading() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("# Heading 1");
        assert!(!lines.is_empty());
        let line_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(line_text.contains("Heading 1"));
        // Heading should be bold
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::BOLD)),
            "Heading should be bold"
        );
    }

    #[test]
    fn renders_inline_code() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("Use `code` here");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("code"));
    }

    #[test]
    fn renders_code_block() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("```rust\nfn main() {}\n```");
        // Should have code line (language is used for highlighting, not displayed)
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("main"));
    }

    #[test]
    fn renders_bullet_list() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("- Item 1\n- Item 2");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Item 1"));
        assert!(all_text.contains("Item 2"));
        assert!(all_text.contains("-"), "Should have bullet marker");
    }

    #[test]
    fn renders_ordered_list() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("1. First\n2. Second");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("First"));
        assert!(all_text.contains("Second"));
        assert!(all_text.contains("1."));
        assert!(all_text.contains("2."));
    }

    #[test]
    fn list_item_wraps_with_proper_indent() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(30);
        // Long list item that should wrap
        let lines = r.render("- This is a very long list item that should wrap to the next line");

        // Should produce multiple lines for the wrapped content
        assert!(
            lines.len() >= 2,
            "Long list item should wrap to multiple lines, got {} lines",
            lines.len()
        );

        // First line should start with bullet
        let first_line: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            first_line.starts_with("- "),
            "First line should start with bullet: {:?}",
            first_line
        );

        // Continuation lines should have proper indentation (spaces, not bullet)
        if lines.len() > 1 {
            let second_line: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                second_line.starts_with("  "),
                "Continuation line should be indented: {:?}",
                second_line
            );
            assert!(
                !second_line.starts_with("- "),
                "Continuation line should not have bullet: {:?}",
                second_line
            );
        }
    }

    #[test]
    fn renders_link_with_underline() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("[Click here](https://example.com)");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Click here"));
        assert!(all_text.contains("example.com"));
        // Link text should be underlined
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::UNDERLINED)),
            "Link should be underlined"
        );
    }

    #[test]
    fn renders_blockquote() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> Quote text");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Quote text"));
        assert!(all_text.contains("│"), "Should have blockquote border");
    }

    #[test]
    fn renders_horizontal_rule() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("---");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("-"), "Should have dashes for HR");
    }

    #[test]
    fn table_renders_with_borders() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| A | B |\n|---|---|\n| 1 | 2 |");

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains('\u{250C}'), "Should have top-left corner (┌)");
        assert!(
            text.contains('\u{2510}'),
            "Should have top-right corner (┐)"
        );
        assert!(
            text.contains('\u{2514}'),
            "Should have bottom-left corner (└)"
        );
        assert!(
            text.contains('\u{2518}'),
            "Should have bottom-right corner (┘)"
        );
        assert!(
            text.contains('\u{2502}'),
            "Should have vertical borders (│)"
        );
        assert!(
            text.contains('\u{2500}'),
            "Should have horizontal borders (─)"
        );
    }

    #[test]
    fn table_header_is_bold() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| Header |\n|--------|\n| Data   |");

        // Header line should have bold spans
        let header_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.contains("Header")));
        assert!(header_line.is_some(), "Should have header line");
        let has_bold = header_line
            .unwrap()
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(has_bold, "Header should be bold");
    }

    #[test]
    fn table_contains_data() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| A | B |\n|---|---|\n| 1 | 2 |");

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains('A'), "Should contain header A");
        assert!(text.contains('B'), "Should contain header B");
        assert!(text.contains('1'), "Should contain data 1");
        assert!(text.contains('2'), "Should contain data 2");
    }

    #[test]
    fn table_with_multiple_rows() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| H1 | H2 |\n|----|----|\n| R1C1 | R1C2 |\n| R2C1 | R2C2 |");

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        // Check for separator row character (├)
        assert!(
            text.contains('\u{251C}'),
            "Should have separator row left T (├)"
        );
    }

    #[test]
    fn table_border_has_gray_color() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| A | B |\n|---|---|\n| 1 | 2 |");

        // Border characters should have Gray foreground color (from TableBorder style)
        let has_gray_border = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains('\u{2500}') // horizontal border
                    && s.style.fg == Some(Color::Gray)
            })
        });
        assert!(has_gray_border, "Border should have Gray color");
    }

    #[test]
    fn table_structure_is_correct() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let lines = r.render("| Col1 | Col2 |\n|------|------|\n| A    | B    |");

        // Verify minimum expected lines:
        // 1. Top border (┌───┬───┐)
        // 2. Header row (│ Col1 │ Col2 │)
        // 3. Separator row (├───┼───┤)
        // 4. Data row (│ A │ B │)
        // 5. Bottom border (└───┴───┘)
        assert!(
            lines.len() >= 5,
            "Table should have at least 5 lines (borders + rows)"
        );

        // First line should start with top-left corner
        let first_line_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            first_line_text.starts_with('\u{250C}'),
            "First line should start with ┌"
        );
        assert!(
            first_line_text.ends_with('\u{2510}'),
            "First line should end with ┐"
        );

        // Last line should start with bottom-left corner
        let last_line_text: String = lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            last_line_text.starts_with('\u{2514}'),
            "Last line should start with └"
        );
        assert!(
            last_line_text.ends_with('\u{2518}'),
            "Last line should end with ┘"
        );
    }

    #[test]
    fn table_no_blank_lines_with_wrapping() {
        // Test that tables with wrapped cell content don't have blank lines between rows
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(60);

        // Table with long content that will wrap
        let markdown = "| Tool | Description |\n|------|-------------|\n| Glob | Fast file pattern matching tool that finds files by pattern. |\n| Grep | Search content with regex patterns. |";

        let lines = r.render(markdown);

        // Extract text from each line
        let line_texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        // Check for blank lines between table rows
        // A blank line would be a line that doesn't contain any table characters
        let mut prev_line_was_table_row = false;
        for (i, text) in line_texts.iter().enumerate() {
            let is_table_row = text.contains('│')
                || text.contains('├')
                || text.contains('┌')
                || text.contains('└');
            let is_blank = text.trim().is_empty();

            if prev_line_was_table_row && is_blank {
                panic!(
                    "Found blank line at index {} after table row. Lines:\n{}",
                    i,
                    line_texts.join("\n")
                );
            }

            if is_table_row {
                prev_line_was_table_row = true;
            }
        }
    }

    #[test]
    fn wrap_text_splits_at_word_boundaries() {
        // "hello world foo bar" = 19 chars
        // With width 10: "hello" (5), " world" would be 11 > 10, so wrap
        // Line 1: "hello"
        // Line 2: "world foo" (9)
        // Line 3: "bar" (3)
        let wrapped = wrap_text("hello world foo bar", 10);
        assert_eq!(wrapped.len(), 3);
        assert_eq!(wrapped[0], "hello");
        assert_eq!(wrapped[1], "world foo");
        assert_eq!(wrapped[2], "bar");
    }

    #[test]
    fn wrap_text_breaks_long_words() {
        let wrapped = wrap_text("supercalifragilisticexpialidocious", 10);
        // Single long word should be broken to fit width
        assert!(
            wrapped.len() > 1,
            "Long word should be broken into multiple lines"
        );
        // All pieces should be <= 10 chars
        for piece in &wrapped {
            assert!(
                display_width(piece) <= 10,
                "Each piece should fit within width: '{}'",
                piece
            );
        }
        // Joined, they should reconstruct the original word
        let joined: String = wrapped.join("");
        assert_eq!(joined, "supercalifragilisticexpialidocious");
    }

    #[test]
    fn display_width_counts_chars() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("hello world"), 11);
    }

    #[test]
    fn wikilink_is_styled() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("See [[Project Architecture]] for details.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("[[Project Architecture]]"),
            "Wikilink should be present"
        );

        let wikilink_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("[["));
        assert!(wikilink_span.is_some(), "Should have wikilink span");
        assert_eq!(
            wikilink_span.unwrap().style.fg,
            Some(Color::Indexed(5)),
            "Wikilink should be magenta"
        );
    }

    #[test]
    fn wikilink_with_alias_shows_alias() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("See [[long-note-name|Short Name]] for details.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("[[Short Name]]"),
            "Aliased wikilink should show alias: {}",
            all_text
        );
    }

    #[test]
    fn tag_is_styled() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("Topics: #rust #ai-ml #project/backend");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("#rust"), "Tag should be present");
        assert!(all_text.contains("#ai-ml"), "Hyphenated tag should work");
        assert!(
            all_text.contains("#project/backend"),
            "Nested tag should work"
        );

        let tag_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("#rust"));
        assert!(tag_span.is_some(), "Should have tag span");
        assert_eq!(
            tag_span.unwrap().style.fg,
            Some(Color::Indexed(3)),
            "Tag should be yellow"
        );
    }

    #[test]
    fn agent_mention_is_styled() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("Ask @oracle or @librarian for help.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("@oracle"),
            "Agent mention should be present"
        );
        assert!(
            all_text.contains("@librarian"),
            "Second agent mention should be present"
        );

        let agent_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("@oracle"));
        assert!(agent_span.is_some(), "Should have agent span");
        let style = agent_span.unwrap().style;
        assert_eq!(style.fg, Some(Color::Indexed(6)), "Agent should be cyan");
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "Agent should be bold"
        );
    }

    #[test]
    fn mixed_custom_syntax_in_text() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("See [[Docs]] about #setup, ask @helper if stuck.");

        let spans: Vec<_> = lines.iter().flat_map(|l| l.spans.iter()).collect();

        let has_wikilink = spans.iter().any(|s| s.content.contains("[[Docs]]"));
        let has_tag = spans.iter().any(|s| s.content.contains("#setup"));
        let has_agent = spans.iter().any(|s| s.content.contains("@helper"));

        assert!(has_wikilink, "Should have wikilink");
        assert!(has_tag, "Should have tag");
        assert!(has_agent, "Should have agent mention");
    }

    #[test]
    fn callout_note_renders_with_icon() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> [!note]\n> This is a note.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("ℹ"), "Note callout should have info icon");
        assert!(
            all_text.contains("This is a note"),
            "Callout content should be present: {}",
            all_text
        );
    }

    #[test]
    fn callout_warning_renders_with_icon() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> [!warning] Watch out\n> Be careful here.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("⚠"),
            "Warning callout should have warning icon"
        );
        assert!(
            all_text.contains("Watch out"),
            "Custom title should be present"
        );
        assert!(
            all_text.contains("Be careful"),
            "Callout content should be present"
        );
    }

    #[test]
    fn callout_tip_has_green_color() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> [!tip]\n> A helpful tip.");

        let tip_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("✓"));
        assert!(tip_span.is_some(), "Should have tip icon");
        assert_eq!(
            tip_span.unwrap().style.fg,
            Some(Color::Indexed(2)),
            "Tip callout should be green"
        );
    }

    #[test]
    fn callout_danger_has_red_color() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> [!danger]\n> Critical issue!");

        let danger_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("✕"));
        assert!(danger_span.is_some(), "Should have danger icon");
        assert_eq!(
            danger_span.unwrap().style.fg,
            Some(Color::Indexed(1)),
            "Danger callout should be red"
        );
    }

    #[test]
    fn regular_blockquote_still_works() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("> Just a normal quote.");

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("Just a normal quote"),
            "Regular blockquote content should render"
        );
        assert!(all_text.contains("│"), "Should have blockquote border");
        assert!(
            !all_text.contains("ℹ") && !all_text.contains("⚠"),
            "Regular blockquote should not have callout icons"
        );
    }

    #[test]
    fn wide_table_fits_viewport_with_word_breaking() {
        // Table with long words in columns that force minimum widths exceeding viewport
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(60);

        // 5 columns with long single words that can't be shrunk without word breaking
        // Each column has a ~12-15 char word, plus borders and padding = way over 60
        let markdown = "| ToolFunction | CategoryType | PrimaryPurpose | ArgumentsList | ExampleUsage |\n\
                        |--------------|--------------|----------------|---------------|---------------|\n\
                        | read_file_op | filesystem | RetrieveContent | path,offset | read(path) |\n\
                        | write_file_op | filesystem | WriteContent | path,content | write(path) |";

        let lines = r.render(markdown);

        // Every table line should fit within the viewport width
        for (i, line) in lines.iter().enumerate() {
            let line_width: usize = line.spans.iter().map(|s| display_width(&s.content)).sum();
            assert!(
                line_width <= 60,
                "Line {} exceeds viewport width of 60: {} chars - content: {}",
                i,
                line_width,
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            );
        }

        // All original content should be present (words may be broken across lines)
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        // Check that key content from each column is present
        assert!(
            all_text.contains("Tool") || all_text.contains("ool"),
            "Header content should be present"
        );
        assert!(
            all_text.contains("read") || all_text.contains("ead"),
            "Data content should be present"
        );
    }

    #[test]
    fn theme_accessor_works() {
        let theme = MarkdownTheme::dark();
        let r = RatatuiMarkdown::new(theme);
        assert!(r.theme().is_dark());
    }

    #[test]
    fn code_block_rust_has_colored_spans() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("```rust\nfn main() {\n    println!(\"Hello\");\n}\n```");

        // Should have multiple lines (language indicator + code lines)
        assert!(lines.len() >= 3, "Should have code lines");

        // At least one line should have colored spans (not just default)
        let has_colored = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| matches!(span.style.fg, Some(Color::Rgb(_, _, _))))
        });
        assert!(has_colored, "Rust code should have syntax highlighting");
    }

    #[test]
    fn code_block_unknown_lang_still_renders() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = r.render("```unknownlang\nsome code\n```");
        assert!(!lines.is_empty());

        // Should still contain the code text
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("some code"),
            "Unknown language code should still render"
        );
    }

    #[test]
    fn asterisk_bullet_list_with_bold_wraps_correctly() {
        // Test case based on user-reported issue: asterisk bullet lists with bold
        // content should wrap at narrow widths
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(50);
        let markdown = "* **Vectorized operations** (`np.sum`, `np.cos`, etc.) make the calculations fast and efficient.";

        let lines = r.render(markdown);

        // Should have multiple lines (content wraps)
        assert!(
            lines.len() >= 2,
            "Long bullet list item should wrap. Got {} lines:\n{}",
            lines.len(),
            lines
                .iter()
                .map(|l| l
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        );

        // All content should be present
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        assert!(all_text.contains("Vectorized"), "Should contain bold text");
        assert!(
            all_text.contains("calculations"),
            "Should contain wrapped content"
        );

        // First line should have bullet marker (- not *)
        let first_line: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            first_line.contains('-'),
            "First line should have bullet marker: {}",
            first_line
        );
    }

    #[test]
    fn narrow_width_wrapping_preserves_content() {
        // Test at very narrow width to ensure all content is preserved
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(30);
        let markdown = "This is a paragraph with some long words that need to wrap correctly across multiple lines.";

        let lines = r.render(markdown);

        // Should wrap into multiple lines
        assert!(
            lines.len() >= 3,
            "Should wrap at narrow width. Got {} lines",
            lines.len()
        );

        // All words should be present
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        for word in [
            "paragraph",
            "long",
            "words",
            "wrap",
            "correctly",
            "multiple",
            "lines",
        ] {
            assert!(
                all_text.contains(word),
                "Should contain '{}' in: {}",
                word,
                all_text
            );
        }
    }

    #[test]
    fn ordered_list_wraps_at_narrow_width() {
        // Test ordered list wrapping - reproducing user-reported truncation
        // User saw: "1. **Opens** the CSV file (`sample_table.csv`) using the built‑i"
        // This tests that ordered list items wrap correctly
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(60);
        let markdown = r#"1. **Opens** the CSV file (`sample_table.csv`) using the built-in `open()` function with `mode='r'`
2. **Creates** a `DictReader`, which automatically uses the first row as column headers
3. **Iterates** over each row, converting the `Age` column to an integer"#;

        let lines = r.render(markdown);

        // Collect all text to check content preservation
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        // All key content should be present (not truncated)
        assert!(
            all_text.contains("built-in"),
            "Content should not be truncated, missing 'built-in' in:\n{}",
            all_text
        );
        assert!(
            all_text.contains("DictReader"),
            "Content should not be truncated, missing 'DictReader' in:\n{}",
            all_text
        );
        assert!(
            all_text.contains("integer"),
            "Content should not be truncated, missing 'integer' in:\n{}",
            all_text
        );

        // Should have multiple lines (wrapping occurred)
        assert!(
            lines.len() >= 5,
            "Should wrap into multiple lines. Got {} lines:\n{}",
            lines.len(),
            lines
                .iter()
                .enumerate()
                .map(|(i, l)| format!(
                    "{}: {}",
                    i,
                    l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // Verify each line is not too long (should be <= 60 chars)
        for (i, line) in lines.iter().enumerate() {
            let line_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(
                line_len <= 60,
                "Line {} is too long ({} chars): {}",
                i,
                line_len,
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            );
        }
    }

    #[test]
    fn list_item_bold_has_bold_modifier() {
        // Verify that **bold** text in list items actually gets the BOLD modifier
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let markdown = "- The **Eiffel Tower** is a famous landmark";

        let lines = r.render(markdown);

        // Find spans containing "Eiffel" or "Tower" - word wrapping splits them
        let mut found_eiffel = false;
        let mut found_tower = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("Eiffel") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "Span '{}' should have BOLD modifier, got {:?}",
                        span.content,
                        span.style
                    );
                    found_eiffel = true;
                }
                if span.content.contains("Tower") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "Span '{}' should have BOLD modifier, got {:?}",
                        span.content,
                        span.style
                    );
                    found_tower = true;
                }
            }
        }

        assert!(
            found_eiffel && found_tower,
            "Should have found both 'Eiffel' and 'Tower' spans with BOLD. Lines:\n{}",
            lines
                .iter()
                .map(|l| format!("{:?}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn nested_code_block_in_bullet_list() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let markdown = "- Item with code:\n  ```rust\n  fn main() {}\n  ```";
        let lines = r.render(markdown);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("main"),
            "Code block content should be present in list"
        );
        assert!(all_text.contains("-"), "Should have bullet marker");
    }

    #[test]
    fn nested_code_block_in_ordered_list() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark());
        let markdown = "1. Step with code:\n   ```python\n   print('hello')\n   ```";
        let lines = r.render(markdown);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("hello"),
            "Code block content should be present in ordered list"
        );
        assert!(all_text.contains("1."), "Should have number marker");
    }

    #[test]
    fn blockquote_after_table_has_correct_indent() {
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let markdown = "| Col1 | Col2 |\n|------|------|\n| A    | B    |\n\n> Switch to act mode";

        let lines = r.render(markdown);

        let mut found_blockquote = false;
        for (i, line) in lines.iter().enumerate() {
            let line_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if line_text.contains("Switch") {
                found_blockquote = true;
                let first_content = line.spans.first().map(|s| s.content.as_ref()).unwrap_or("");
                assert!(
                    first_content.starts_with("  │")
                        || first_content == "│"
                        || first_content.trim_start() == "│",
                    "Line {}: Blockquote should have border. First span: '{}', Full line: '{}'",
                    i,
                    first_content,
                    line_text
                );
            }
        }
        assert!(
            found_blockquote,
            "Should have found blockquote line with 'Switch'"
        );
    }

    #[test]
    fn content_after_table_is_preserved() {
        // Test that content after a table is not eaten/lost
        let r = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(80);
        let markdown = "| Tool | Purpose |\n|------|--------|\n| read | Read files |\n\nThis is text after the table.\n\nAnd another paragraph.";

        let lines = r.render(markdown);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        // All content should be present
        assert!(all_text.contains("Tool"), "Table header should be present");
        assert!(
            all_text.contains("Read files"),
            "Table data should be present"
        );
        assert!(
            all_text.contains("text after the table"),
            "Content after table should be present, got:\n{}",
            all_text
        );
        assert!(
            all_text.contains("another paragraph"),
            "Second paragraph after table should be present"
        );
    }
}
