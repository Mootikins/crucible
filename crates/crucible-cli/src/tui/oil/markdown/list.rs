use super::context::RenderContext;
use super::render::{render_node, text_node};
use super::table::wrap_text;
use super::BR_TAG_REGEX;
use crate::tui::oil::theme;
use crucible_oil::node::*;
use markdown_it::parser::inline::Text;
use markdown_it::plugins::cmark::block::list::{BulletList, OrderedList};
use markdown_it::plugins::cmark::inline::newline::{Hardbreak, Softbreak};

pub(super) fn render_list_item(node: &markdown_it::Node, ctx: &mut RenderContext) {
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
        let t = theme::active();
        let b = format!("{} ", t.decorations.bullet_char);
        let w = crucible_oil::ansi::visible_width(&b);
        (b, w)
    };

    let item_text = extract_list_item_text(node);
    let content_width = ctx
        .width
        .saturating_sub(margins.left + list_indent.len() + bullet_width);
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
