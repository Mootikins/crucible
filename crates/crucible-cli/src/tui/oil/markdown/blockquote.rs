use super::context::RenderContext;
use super::render::extract_all_text;
use super::table::wrap_text;
use crate::tui::oil::theme;
use crucible_oil::node::*;
use crucible_oil::style::Style;

pub(super) fn render_blockquote(node: &markdown_it::Node, ctx: &mut RenderContext) {
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
                styled(prefix, {
                    let t = theme::active();
                    Style::new().fg(t.resolve_color(t.colors.blockquote_prefix))
                }),
                styled(line, {
                    let t = theme::active();
                    Style::new()
                        .fg(t.resolve_color(t.colors.blockquote_text))
                        .italic()
                }),
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
