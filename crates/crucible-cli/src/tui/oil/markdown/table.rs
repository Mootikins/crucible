use super::context::RenderContext;
use super::render::extract_all_text;
use crucible_oil::ansi::visible_width;
use crucible_oil::node::*;
use crucible_oil::style::Style;
use markdown_it::plugins::extra::tables::{TableBody, TableCell, TableHead, TableRow};

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

pub(super) fn render_table(node: &markdown_it::Node, ctx: &mut RenderContext) {
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

pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
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
