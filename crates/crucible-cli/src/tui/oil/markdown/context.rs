use super::render::{render_node, text_node};
use super::Margins;
use crucible_oil::ansi::wrap_styled_text;
use crucible_oil::node::*;
use crucible_oil::style::Style;
use markdown_it::MarkdownIt;

pub(super) struct RenderContext {
    pub(super) blocks: Vec<Node>,
    pub(super) current_spans: Vec<(String, Style)>,
    pub(super) style_stack: Vec<Style>,
    pub(super) list_depth: usize,
    pub(super) list_counter: Option<usize>,
    pub(super) needs_blank_line: bool,
    pub(super) width: usize,
    pub(super) table_width: usize,
    pub(super) blockquote_width: usize,
    pub(super) margins: Margins,
    pub(super) is_first_paragraph: bool,
}

impl RenderContext {
    pub(super) fn new(
        width: usize,
        table_width: usize,
        blockquote_width: usize,
        margins: Margins,
    ) -> Self {
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

    pub(super) fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    pub(super) fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    pub(super) fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    pub(super) fn push_text(&mut self, text: &str) {
        if !text.is_empty() {
            self.current_spans
                .push((text.to_string(), self.current_style()));
        }
    }

    pub(super) fn flush_line(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }

        let spans = std::mem::take(&mut self.current_spans);

        if spans.len() == 1 {
            if let Some((content, style)) = spans.into_iter().next() {
                if style == Style::default() {
                    self.blocks.push(text_node(&content));
                } else {
                    self.blocks.push(styled(&content, style));
                }
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

    pub(super) fn push_block(&mut self, node: Node) {
        self.flush_line();
        self.blocks.push(node);
    }

    pub(super) fn ensure_block_spacing(&mut self) {
        if self.needs_blank_line && !self.blocks.is_empty() && self.list_depth == 0 {
            // A space character gives this node height=1 in Taffy layout;
            // empty string would get height=0 and collapse to nothing.
            // CellGrid compact mode strips trailing spaces, so it renders
            // as an empty line (visual blank line between blocks).
            self.blocks.push(text(" "));
        }
        self.needs_blank_line = false;
    }

    pub(super) fn mark_block_end(&mut self) {
        self.needs_blank_line = true;
    }

    pub(super) fn into_node(mut self) -> Node {
        self.flush_line();
        if self.blocks.is_empty() {
            Node::Empty
        } else if self.blocks.len() == 1 {
            self.blocks.pop().unwrap_or(Node::Empty)
        } else {
            col(self.blocks)
        }
    }
}

pub(super) fn create_parser() -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::tables::add(&mut md);
    md
}

pub(super) fn parse_and_render_internal(
    markdown: &str,
    text_width: usize,
    table_width: usize,
    blockquote_width: usize,
    margins: Margins,
) -> Node {
    use std::cell::RefCell;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // Single-entry cache: skip re-parse when content + params unchanged between frames.
    // During streaming, content changes every few tokens but not every 50ms tick,
    // so many consecutive frames are cache hits.
    thread_local! {
        static CACHE: RefCell<Option<(u64, Node)>> = const { RefCell::new(None) };
    }

    let mut hasher = DefaultHasher::new();
    markdown.hash(&mut hasher);
    text_width.hash(&mut hasher);
    table_width.hash(&mut hasher);
    blockquote_width.hash(&mut hasher);
    margins.left.hash(&mut hasher);
    margins.right.hash(&mut hasher);
    margins.show_bullet.hash(&mut hasher);
    let key = hasher.finish();

    if let Some(cached) = CACHE.with(|c| {
        c.borrow().as_ref().and_then(
            |(k, node)| {
                if *k == key {
                    Some(node.clone())
                } else {
                    None
                }
            },
        )
    }) {
        return cached;
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        let md = create_parser();
        let ast = md.parse(markdown);

        let mut ctx = RenderContext::new(text_width, table_width, blockquote_width, margins);
        render_node(&ast, &mut ctx);
        ctx.into_node()
    }));

    let node = result.unwrap_or_else(|_| text(markdown));
    CACHE.with(|c| *c.borrow_mut() = Some((key, node.clone())));
    node
}
