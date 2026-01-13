//! Markdown to Ink Node renderer
//!
//! Parses markdown using markdown-it and converts to ink Node trees
//! for styled terminal rendering.

use crate::tui::ink::node::*;
use crate::tui::ink::style::{Color, Style};
use markdown_it::parser::inline::Text;
use markdown_it::plugins::cmark::block::code::CodeBlock as MdCodeBlock;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use markdown_it::plugins::cmark::block::paragraph::Paragraph;
use markdown_it::plugins::cmark::inline::backticks::CodeInline;
use markdown_it::plugins::cmark::inline::emphasis::{Em, Strong};
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use markdown_it::MarkdownIt;

/// Convert markdown text to an ink Node tree
pub fn markdown_to_node(markdown: &str) -> Node {
    let md = create_parser();
    let ast = md.parse(markdown);

    let mut ctx = RenderContext::new();
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
    /// Accumulated block-level nodes
    blocks: Vec<Node>,
    /// Current inline spans being accumulated
    current_spans: Vec<(String, Style)>,
    /// Current style stack
    style_stack: Vec<Style>,
    /// List nesting level
    list_depth: usize,
    /// Ordered list counter
    list_counter: Option<usize>,
}

impl RenderContext {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![Style::default()],
            list_depth: 0,
            list_counter: None,
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

        let nodes: Vec<Node> = spans
            .into_iter()
            .map(|(text, style)| {
                if style == Style::default() {
                    text_node(&text)
                } else {
                    styled(&text, style)
                }
            })
            .collect();

        if nodes.len() == 1 {
            self.blocks.push(nodes.into_iter().next().unwrap());
        } else {
            self.blocks.push(row(nodes));
        }
    }

    fn push_block(&mut self, node: Node) {
        self.flush_line();
        self.blocks.push(node);
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
        render_children(node, ctx);
        ctx.flush_line();
        return;
    }

    if let Some(heading) = node.cast::<ATXHeading>() {
        let style = heading_style(heading.level);
        ctx.push_style(style);
        render_children(node, ctx);
        ctx.pop_style();
        ctx.flush_line();
        return;
    }

    if node.cast::<CodeFence>().is_some() || node.cast::<MdCodeBlock>().is_some() {
        render_code_block(node, ctx);
        return;
    }

    if node.cast::<BulletList>().is_some() {
        ctx.list_depth += 1;
        ctx.list_counter = None;
        render_children(node, ctx);
        ctx.list_depth -= 1;
        return;
    }

    if node.cast::<OrderedList>().is_some() {
        ctx.list_depth += 1;
        ctx.list_counter = Some(1);
        render_children(node, ctx);
        ctx.list_depth -= 1;
        ctx.list_counter = None;
        return;
    }

    if node.cast::<ListItem>().is_some() {
        render_list_item(node, ctx);
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

    let code_style = Style::new().fg(Color::Green);

    if let Some(ref lang) = lang {
        ctx.push_block(styled(
            format!("```{}", lang),
            Style::new().fg(Color::DarkGray),
        ));
    } else {
        ctx.push_block(styled("```", Style::new().fg(Color::DarkGray)));
    }

    for line in content.lines() {
        ctx.push_block(styled(format!("  {}", line), code_style));
    }

    ctx.push_block(styled("```", Style::new().fg(Color::DarkGray)));
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

    ctx.current_spans
        .push((bullet, Style::new().fg(Color::DarkGray)));
    render_children(node, ctx);
    ctx.flush_line();
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
        assert!(output.contains("fn main()"));
    }

    #[test]
    fn test_bullet_list() {
        let node = markdown_to_node("- Item 1\n- Item 2\n- Item 3");
        let output = render_to_string(&node, 80);
        assert!(output.contains("Item 1"));
        assert!(output.contains("•"));
    }
}
