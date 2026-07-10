use super::context::RenderContext;
use super::render::{extract_all_text, text_node};
use crate::tui::oil::theme;
use crucible_oil::style::Style;
use markdown_it::plugins::cmark::block::code::CodeBlock as MdCodeBlock;
use markdown_it::plugins::cmark::block::fence::CodeFence;

pub(super) fn render_code_block(node: &markdown_it::Node, ctx: &mut RenderContext) {
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

    // Render entire code block as a single text node with embedded newlines.
    // This prevents the layout engine from adding inter-line spacing
    // (each line as a separate node in the outer col gets blank lines between).
    let t = theme::active();
    let fence_style = Style::new().fg(t.resolve_color(t.colors.fence_marker));

    let fence_codes = fence_style.to_ansi_codes();
    let fence_open_ansi = format!("{fence_codes}{indent}{fence_marker}\x1b[0m");
    let fence_close_ansi = format!("{fence_codes}{indent}```\x1b[0m");

    // Build highlighted code lines as ANSI strings
    let code_lines = build_highlighted_code_lines(&content, lang_str, &indent, ctx.width);

    // Join everything with \n into a single text node
    let mut full_block = fence_open_ansi;
    for line in &code_lines {
        full_block.push('\n');
        full_block.push_str(line);
    }
    full_block.push('\n');
    full_block.push_str(&fence_close_ansi);

    ctx.flush_line();
    // Push as a pre-formatted text node — the renderer handles \n within text
    ctx.blocks.push(text_node(&full_block));
    ctx.mark_block_end();
}

/// Build highlighted code lines as ANSI strings (one per visual line).
fn build_highlighted_code_lines(
    content: &str,
    lang: &str,
    indent: &str,
    width: usize,
) -> Vec<String> {
    use crate::formatting::SyntaxHighlighter;
    use crucible_oil::ansi::wrap_styled_text;

    let mut result = Vec::new();
    let wrap_width = width.saturating_sub(indent.len());

    if lang.is_empty() || !SyntaxHighlighter::supports_language(lang) {
        let t = theme::active();
        let fallback = Style::new().fg(t.resolve_color(t.colors.code_fallback));
        for line in content.lines() {
            let spans = vec![(line.to_string(), fallback.to_ansi_codes())];
            for wrapped in wrap_styled_text(&spans, wrap_width) {
                result.push(format!("{indent}{wrapped}"));
            }
        }
        return result;
    }

    let highlighter = SyntaxHighlighter::active();
    let highlighted_lines = highlighter.highlight(content, lang);

    for highlighted_line in highlighted_lines {
        if highlighted_line.spans.is_empty() {
            result.push(indent.to_string());
            continue;
        }

        let spans: Vec<(String, String)> = highlighted_line
            .spans
            .iter()
            .map(|span| (span.text.clone(), span.style.to_ansi_codes()))
            .collect();

        for wrapped in wrap_styled_text(&spans, wrap_width) {
            result.push(format!("{indent}{wrapped}"));
        }
    }

    result
}
