use super::blockquote::render_blockquote;
use super::code::render_code_block;
use super::context::RenderContext;
use super::list::render_list_item;
use super::table::render_table;
use super::{ASSISTANT_BULLET, BR_TAG_REGEX};
use crate::tui::oil::markdown::table::wrap_text;
use crate::tui::oil::theme;
use crucible_oil::node::*;
use crucible_oil::style::Style;
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
use markdown_it::plugins::extra::tables::Table;

pub(super) fn render_node(node: &markdown_it::Node, ctx: &mut RenderContext) {
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
                styled(ASSISTANT_BULLET, {
                    let t = theme::active();
                    Style::new().fg(t.resolve_color(t.colors.bullet_prefix))
                })
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
        ctx.current_spans.push((format!("`{}`", code_text), {
            let t = theme::active();
            Style::new().fg(t.resolve_color(t.colors.code_inline))
        }));
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

pub(super) fn render_children(node: &markdown_it::Node, ctx: &mut RenderContext) {
    for child in node.children.iter() {
        render_node(child, ctx);
    }
}

pub(super) fn render_paragraph(node: &markdown_it::Node, ctx: &mut RenderContext) {
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
            styled(ASSISTANT_BULLET, {
                let t = theme::active();
                Style::new().fg(t.resolve_color(t.colors.bullet_prefix))
            })
        } else {
            text(&indent)
        };
        ctx.blocks.push(row([prefix, text_node(line)]));
    }

    ctx.is_first_paragraph = false;
    ctx.mark_block_end();
}

pub(super) fn render_link(node: &markdown_it::Node, link: &Link, ctx: &mut RenderContext) {
    let link_text = extract_all_text(node);
    let display = if link_text.is_empty() {
        link.url.clone()
    } else {
        link_text
    };
    ctx.current_spans.push((display, {
        let t = theme::active();
        Style::new().fg(t.resolve_color(t.colors.link)).underline()
    }));
}

pub(super) fn heading_style(level: u8) -> Style {
    let t = theme::active();
    match level {
        1 => Style::new().fg(t.resolve_color(t.colors.heading_1)).bold(),
        2 => Style::new().fg(t.resolve_color(t.colors.heading_2)).bold(),
        3 => Style::new().fg(t.resolve_color(t.colors.heading_3)).bold(),
        _ => Style::new().bold(),
    }
}

pub(super) fn extract_all_text(node: &markdown_it::Node) -> String {
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

pub(super) fn text_node(content: &str) -> Node {
    text(content)
}

#[allow(dead_code)] // WIP: normalize_br_tags not yet used
pub(super) fn normalize_br_tags(input: &str) -> String {
    // Replace with "  \n" (two trailing spaces = markdown Hardbreak)
    BR_TAG_REGEX.replace_all(input, "  \n").into_owned()
}
