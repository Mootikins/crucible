use crate::tui::oil::ansi::{visible_width, visual_rows};
use crate::tui::oil::layout::{calculate_row_widths, ChildMeasurement, FlexLayoutInput};
use crate::tui::oil::node::{
    BoxNode, Direction, InputNode, Node, PopupNode, Size, SpinnerNode, TextNode,
};
use crate::tui::oil::style::{Color, Style};
use crossterm::style::Stylize;
use std::io::{self, Write};
use textwrap::{wrap, Options, WordSplitter};

pub trait RenderFilter {
    fn skip_static(&self, key: &str) -> bool;
}

pub struct NoFilter;

impl RenderFilter for NoFilter {
    fn skip_static(&self, _key: &str) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorInfo {
    pub col: u16,
    pub row_from_end: u16,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub content: String,
    pub cursor: CursorInfo,
}

pub fn render_to_string(node: &Node, width: usize) -> String {
    render_with_cursor(node, width).content
}

/// Render a slice of nodes without cloning them into a Fragment.
pub fn render_children_to_string(children: &[Node], width: usize) -> String {
    let mut output = String::new();
    let mut cursor_info = CursorInfo::default();
    for child in children {
        render_node_filtered(child, width, &NoFilter, &mut output, &mut cursor_info);
    }
    output
}

pub fn render_with_cursor(node: &Node, width: usize) -> RenderResult {
    render_with_cursor_filtered(node, width, &NoFilter)
}

pub fn render_with_cursor_filtered(
    node: &Node,
    width: usize,
    filter: &dyn RenderFilter,
) -> RenderResult {
    let mut output = String::new();
    let mut cursor_info = CursorInfo::default();
    render_node_filtered(node, width, filter, &mut output, &mut cursor_info);

    if cursor_info.visible {
        let lines: Vec<&str> = output.lines().collect();
        let cursor_line_idx = cursor_info.row_from_end as usize;

        let visual_rows_below: usize = lines
            .iter()
            .skip(cursor_line_idx + 1)
            .map(|line| visual_rows(line, width))
            .sum();

        cursor_info.row_from_end = visual_rows_below as u16;
    }

    RenderResult {
        content: output,
        cursor: cursor_info,
    }
}

fn render_node_tracking_cursor(
    node: &Node,
    width: usize,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    render_node_filtered(node, width, &NoFilter, output, cursor_info);
}

fn render_node_filtered(
    node: &Node,
    width: usize,
    filter: &dyn RenderFilter,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    match node {
        Node::Empty => {}

        Node::Text(text) => {
            render_text(text, width, output);
        }

        Node::Box(boxnode) => {
            render_box_filtered(boxnode, width, filter, output, cursor_info);
        }

        Node::Static(static_node) => {
            if filter.skip_static(&static_node.key) {
                return;
            }
            for child in &static_node.children {
                render_node_filtered(child, width, filter, output, cursor_info);
            }
        }

        Node::Input(input) => {
            render_input_tracking_cursor(input, output, cursor_info);
        }

        Node::Spinner(spinner) => {
            render_spinner(spinner, output);
        }

        Node::Popup(popup) => {
            render_popup(popup, width, output);
        }

        Node::Fragment(children) => {
            for child in children {
                render_node_filtered(child, width, filter, output, cursor_info);
            }
        }

        Node::Focusable(focusable) => {
            render_node_filtered(&focusable.child, width, filter, output, cursor_info);
        }

        Node::ErrorBoundary(boundary) => {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut child_output = String::new();
                let mut child_cursor = CursorInfo::default();
                render_node_filtered(
                    &boundary.child,
                    width,
                    &NoFilter,
                    &mut child_output,
                    &mut child_cursor,
                );
                (child_output, child_cursor)
            }));

            match result {
                Ok((child_output, child_cursor)) => {
                    output.push_str(&child_output);
                    if child_cursor.visible {
                        *cursor_info = child_cursor;
                    }
                }
                Err(_) => {
                    render_node_filtered(&boundary.fallback, width, filter, output, cursor_info)
                }
            }
        }

        Node::Overlay(overlay) => {
            render_node_filtered(&overlay.child, width, filter, output, cursor_info);
        }
    }
}

fn render_input_tracking_cursor(
    input: &InputNode,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    let col_before = output.lines().last().map(visible_width).unwrap_or(0);

    if input.value.is_empty() {
        if let Some(placeholder) = &input.placeholder {
            let styled = apply_style(placeholder, &Style::new().dim());
            output.push_str(&styled);
        }
    } else {
        let styled = apply_style(&input.value, &input.style);
        output.push_str(&styled);
    }

    if input.focused {
        let cursor_char_pos = input.cursor.min(input.value.chars().count());
        let cursor_col = input.value.chars().take(cursor_char_pos).count();
        cursor_info.col = (col_before + cursor_col) as u16;
        cursor_info.row_from_end = output.lines().count().saturating_sub(1) as u16;
        cursor_info.visible = true;
    }
}

fn render_box_filtered(
    boxnode: &BoxNode,
    width: usize,
    filter: &dyn RenderFilter,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    let border_size = if boxnode.border.is_some() { 2 } else { 0 };
    let inner_width = width
        .saturating_sub(boxnode.padding.horizontal() as usize)
        .saturating_sub(border_size);

    match boxnode.direction {
        Direction::Column => {
            render_column_children_filtered(
                &boxnode.children,
                inner_width,
                boxnode.gap.row,
                filter,
                output,
                cursor_info,
            );
        }
        Direction::Row => {
            render_row_children_filtered(
                &boxnode.children,
                inner_width,
                filter,
                output,
                cursor_info,
            );
        }
    };

    if cursor_info.visible {
        let padding_offset = boxnode.padding.left;
        let border_offset = if boxnode.border.is_some() { 1 } else { 0 };
        cursor_info.col += padding_offset + border_offset;
    }
}

fn render_column_children_filtered(
    children: &[Node],
    width: usize,
    gap: u16,
    filter: &dyn RenderFilter,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    let mut rendered_any = false;
    for child in children.iter() {
        if matches!(child, Node::Empty) {
            continue;
        }
        if rendered_any && !output.is_empty() {
            // Separate children with newlines: 1 base + `gap` additional blank lines
            // gap=0 → "A\r\nB", gap=1 → "A\r\n\r\nB", gap=2 → "A\r\n\r\n\r\nB"
            for _ in 0..=gap {
                output.push_str("\r\n");
            }
        }
        render_node_filtered(child, width, filter, output, cursor_info);
        rendered_any = true;
    }
}

fn render_row_children_filtered(
    children: &[Node],
    width: usize,
    filter: &dyn RenderFilter,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    if children.is_empty() {
        return;
    }

    // Phase 1: measure all children
    let mut measurements: Vec<ChildMeasurement> = Vec::with_capacity(children.len());
    let mut child_infos: Vec<RowChildInfo> = Vec::with_capacity(children.len());
    let mut max_height: usize = 1;

    for child in children {
        if matches!(child, Node::Empty) {
            measurements.push(ChildMeasurement::Fixed(0));
            child_infos.push(RowChildInfo::Skip);
            continue;
        }

        let size = get_node_size(child);
        match size {
            Size::Fixed(w) => {
                let mut temp = String::new();
                let mut temp_cursor = CursorInfo::default();
                render_node_filtered(child, w as usize, filter, &mut temp, &mut temp_cursor);
                let line_count = temp.lines().count().max(1);
                max_height = max_height.max(line_count);
                measurements.push(ChildMeasurement::Fixed(w as usize));
                child_infos.push(RowChildInfo::Fixed(temp));
            }
            Size::Flex(weight) => {
                measurements.push(ChildMeasurement::Flex(weight));
                child_infos.push(RowChildInfo::Flex);
            }
            Size::Content => {
                let mut temp = String::new();
                let mut temp_cursor = CursorInfo::default();
                render_node_filtered(child, width, filter, &mut temp, &mut temp_cursor);
                let line_count = temp.lines().count().max(1);
                max_height = max_height.max(line_count);
                let content_width = temp.lines().next().map(visible_width).unwrap_or(0);
                measurements.push(ChildMeasurement::Content(content_width));
                child_infos.push(RowChildInfo::Content(temp, temp_cursor));
            }
        }
    }

    let layout_result = calculate_row_widths(&FlexLayoutInput {
        available: width,
        children: measurements,
    });

    // Phase 2: render with calculated widths
    // Use CellGrid for multi-line rows, fast path for single-line
    if max_height > 1 {
        render_row_to_grid(
            children,
            &child_infos,
            &layout_result.widths,
            width,
            max_height,
            output,
        );
    } else {
        render_row_single_line(
            children,
            child_infos,
            &layout_result.widths,
            filter,
            output,
            cursor_info,
        );
    }
}

fn render_row_single_line(
    children: &[Node],
    child_infos: Vec<RowChildInfo>,
    widths: &[usize],
    _filter: &dyn RenderFilter,
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    for (i, (_child, child_info)) in children.iter().zip(child_infos.into_iter()).enumerate() {
        let child_width = widths.get(i).copied().unwrap_or(0);

        match child_info {
            RowChildInfo::Skip => {}
            RowChildInfo::Content(rendered, child_cursor) => {
                if child_cursor.visible {
                    let col_offset = output.lines().last().map(visible_width).unwrap_or(0);
                    let row_offset = output.lines().count().saturating_sub(1);
                    cursor_info.col = child_cursor.col + col_offset as u16;
                    cursor_info.row_from_end = child_cursor.row_from_end + row_offset as u16;
                    cursor_info.visible = true;
                }
                if !rendered.is_empty() {
                    output.push_str(&rendered);
                }
            }
            RowChildInfo::Fixed(rendered) => {
                if !rendered.is_empty() {
                    output.push_str(&rendered);
                }
            }
            RowChildInfo::Flex => {
                if child_width > 0 {
                    output.push_str(&" ".repeat(child_width));
                }
            }
        }
    }
}

fn render_row_to_grid(
    _children: &[Node],
    child_infos: &[RowChildInfo],
    widths: &[usize],
    total_width: usize,
    height: usize,
    output: &mut String,
) {
    use crate::tui::oil::cell_grid::CellGrid;

    let mut grid = CellGrid::new(total_width, height);
    let mut x_offset: usize = 0;

    for (i, child_info) in child_infos.iter().enumerate() {
        let child_width = widths.get(i).copied().unwrap_or(0);

        match child_info {
            RowChildInfo::Skip => {}
            RowChildInfo::Content(rendered, _cursor) => {
                grid.blit_string(rendered, x_offset, 0);
                x_offset += child_width;
            }
            RowChildInfo::Fixed(rendered) => {
                grid.blit_string(rendered, x_offset, 0);
                x_offset += child_width;
            }
            RowChildInfo::Flex => {
                x_offset += child_width;
            }
        }
    }

    output.push_str(&grid.to_string_joined());
}

enum RowChildInfo {
    Skip,
    Fixed(String),
    Flex,
    Content(String, CursorInfo),
}

fn render_node_to_string(node: &Node, width: usize, output: &mut String) {
    match node {
        Node::Empty => {}

        Node::Text(text) => {
            render_text(text, width, output);
        }

        Node::Box(boxnode) => {
            render_box(boxnode, width, output);
        }

        Node::Static(static_node) => {
            for child in &static_node.children {
                render_node_to_string(child, width, output);
            }
        }

        Node::Input(input) => {
            render_input(input, output);
        }

        Node::Spinner(spinner) => {
            render_spinner(spinner, output);
        }

        Node::Popup(popup) => {
            render_popup(popup, width, output);
        }

        Node::Fragment(children) => {
            for child in children {
                render_node_to_string(child, width, output);
            }
        }

        Node::Focusable(focusable) => {
            render_node_to_string(&focusable.child, width, output);
        }

        Node::ErrorBoundary(boundary) => {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut child_output = String::new();
                render_node_to_string(&boundary.child, width, &mut child_output);
                child_output
            }));

            match result {
                Ok(child_output) => output.push_str(&child_output),
                Err(_) => render_node_to_string(&boundary.fallback, width, output),
            }
        }

        Node::Overlay(overlay) => {
            render_node_to_string(&overlay.child, width, output);
        }
    }
}

fn render_text(text: &TextNode, width: usize, output: &mut String) {
    let styled_content = apply_style(&text.content, &text.style);

    if width == 0 || text.content.chars().count() <= width {
        output.push_str(&styled_content);
    } else {
        let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
        let wrapped: Vec<_> = wrap(&text.content, options);

        for (i, line) in wrapped.iter().enumerate() {
            if i > 0 {
                output.push_str("\r\n");
            }
            output.push_str(&apply_style(line, &text.style));
        }
    }
}

fn render_box(boxnode: &BoxNode, width: usize, output: &mut String) {
    let border_size = if boxnode.border.is_some() { 2 } else { 0 };
    let inner_width = width
        .saturating_sub(boxnode.padding.horizontal() as usize)
        .saturating_sub(border_size);

    let children_output = match boxnode.direction {
        Direction::Column => render_column_children(&boxnode.children, inner_width),
        Direction::Row => render_row_children(&boxnode.children, inner_width),
    };

    let content = match boxnode.direction {
        Direction::Column => {
            // Separate children with newlines: 1 base + `gap` additional blank lines
            // gap=0 → "A\r\nB", gap=1 → "A\r\n\r\nB", gap=2 → "A\r\n\r\n\r\nB"
            let separator = "\r\n".repeat(1 + boxnode.gap.row as usize);
            children_output.join(&separator)
        }
        Direction::Row => children_output.join(""),
    };

    if let Some(border) = &boxnode.border {
        render_bordered_content(&content, border, width, &boxnode.style, output);
    } else {
        output.push_str(&content);
    }
}

fn render_column_children(children: &[Node], width: usize) -> Vec<String> {
    children
        .iter()
        .filter(|c| !matches!(c, Node::Empty))
        .map(|child| {
            let mut s = String::new();
            render_node_to_string(child, width, &mut s);
            s
        })
        .collect()
}

fn render_row_children(children: &[Node], width: usize) -> Vec<String> {
    if children.is_empty() {
        return Vec::new();
    }

    let mut fixed_width_used = 0usize;
    let mut total_flex_weight = 0u16;
    let mut child_sizes: Vec<ChildSize> = Vec::with_capacity(children.len());

    for child in children {
        if matches!(child, Node::Empty) {
            child_sizes.push(ChildSize::Skip);
            continue;
        }

        let size = get_node_size(child);
        match size {
            Size::Fixed(w) => {
                fixed_width_used += w as usize;
                child_sizes.push(ChildSize::Fixed(w as usize));
            }
            Size::Flex(weight) => {
                total_flex_weight += weight;
                child_sizes.push(ChildSize::Flex(weight));
            }
            Size::Content => {
                let mut temp = String::new();
                render_node_to_string(child, width, &mut temp);
                let content_width = temp.lines().next().map(visible_width).unwrap_or(0);
                fixed_width_used += content_width;
                child_sizes.push(ChildSize::Content(temp));
            }
        }
    }

    let remaining = width.saturating_sub(fixed_width_used);

    let mut result = Vec::with_capacity(children.len());
    for (child, child_size) in children.iter().zip(child_sizes.into_iter()) {
        match child_size {
            ChildSize::Skip => {}
            ChildSize::Content(rendered) => {
                if !rendered.is_empty() {
                    result.push(rendered);
                }
            }
            ChildSize::Fixed(w) => {
                let mut s = String::new();
                render_node_to_string(child, w, &mut s);
                if !s.is_empty() {
                    result.push(s);
                }
            }
            ChildSize::Flex(weight) => {
                let flex_width = if total_flex_weight > 0 {
                    (remaining as u32 * weight as u32 / total_flex_weight as u32) as usize
                } else {
                    0
                };
                if flex_width > 0 {
                    result.push(" ".repeat(flex_width));
                }
            }
        }
    }

    result
}

enum ChildSize {
    Skip,
    Fixed(usize),
    Flex(u16),
    Content(String),
}

fn get_node_size(node: &Node) -> Size {
    match node {
        Node::Box(b) => b.size,
        _ => Size::Content,
    }
}

fn render_bordered_content(
    content: &str,
    border: &crate::tui::oil::style::Border,
    width: usize,
    style: &Style,
    output: &mut String,
) {
    let chars = border.chars();
    let inner_width = width.saturating_sub(2);

    let top = format!(
        "{}{}{}",
        chars.top_left,
        chars.horizontal.to_string().repeat(inner_width),
        chars.top_right
    );
    output.push_str(&apply_style(&top, style));
    output.push_str("\r\n");

    for line in content.lines() {
        let visible_len = strip_ansi_codes(line).chars().count();
        let padding = inner_width.saturating_sub(visible_len);
        let padded_line = format!("{}{}", line, " ".repeat(padding));
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str(&padded_line);
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str("\r\n");
    }

    if content.is_empty() {
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str(&" ".repeat(inner_width));
        output.push_str(&apply_style(&chars.vertical.to_string(), style));
        output.push_str("\r\n");
    }

    let bottom = format!(
        "{}{}{}",
        chars.bottom_left,
        chars.horizontal.to_string().repeat(inner_width),
        chars.bottom_right
    );
    output.push_str(&apply_style(&bottom, style));
}

fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn render_input(input: &InputNode, output: &mut String) {
    if input.value.is_empty() {
        if let Some(placeholder) = &input.placeholder {
            let styled = apply_style(placeholder, &Style::new().dim());
            output.push_str(&styled);
        }
    } else {
        let styled = apply_style(&input.value, &input.style);
        output.push_str(&styled);
    }
}

fn render_spinner(spinner: &SpinnerNode, output: &mut String) {
    let frame_char = spinner.current_char();
    let styled_spinner = apply_style(&frame_char.to_string(), &spinner.style);

    output.push_str(&styled_spinner);

    if let Some(label) = &spinner.label {
        output.push(' ');
        output.push_str(&apply_style(label, &spinner.style));
    }
}

fn render_popup(popup: &PopupNode, width: usize, output: &mut String) {
    let popup_width = width.saturating_sub(2);
    if popup_width == 0 {
        return;
    }

    let visible_end = (popup.viewport_offset + popup.max_visible).min(popup.items.len());
    let visible_items = &popup.items[popup.viewport_offset..visible_end];
    let item_count = visible_items.len();
    let blank_lines = popup.max_visible.saturating_sub(item_count);
    let mut lines_rendered = 0;

    for _ in 0..blank_lines {
        lines_rendered += 1;
        if lines_rendered < popup.max_visible {
            output.push_str("\r\n");
        }
    }

    for (i, item) in visible_items.iter().enumerate() {
        let actual_index = popup.viewport_offset + i;
        let is_selected = actual_index == popup.selected;
        let item_style = if is_selected {
            popup.selected_style
        } else {
            popup.unselected_style
        };

        let mut line = String::new();
        line.push(' ');

        if is_selected {
            line.push_str("▸ ");
        } else {
            line.push_str("  ");
        }

        if let Some(kind) = &item.kind {
            line.push_str(kind);
            line.push(' ');
        }

        let prefix_width = visible_width(&line);
        let max_label_width = popup_width.saturating_sub(prefix_width + 2);
        let label = if item.label.chars().count() > max_label_width && max_label_width > 4 {
            let s: String = item.label.chars().take(max_label_width - 1).collect();
            format!("{}…", s)
        } else {
            item.label.clone()
        };
        line.push_str(&label);

        let label_width = visible_width(&line);

        if let Some(desc) = &item.description {
            let available = popup_width.saturating_sub(label_width + 3);
            if available > 10 {
                let truncated = if desc.chars().count() > available {
                    let s: String = desc.chars().take(available - 1).collect();
                    format!("{}…", s)
                } else {
                    desc.clone()
                };
                line.push_str("  ");
                let desc_style = item_style.dim();
                output.push_str(&apply_style(&line, &item_style));
                line.clear();
                line.push_str(&truncated);
                let after_desc_width = label_width + 2 + visible_width(&truncated);
                let padding = popup_width.saturating_sub(after_desc_width);
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                output.push_str(&apply_style(&line, &desc_style));
            } else {
                let padding = popup_width.saturating_sub(label_width);
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                output.push_str(&apply_style(&line, &item_style));
            }
        } else {
            let padding = popup_width.saturating_sub(label_width);
            line.push_str(&" ".repeat(padding));
            line.push(' ');
            output.push_str(&apply_style(&line, &item_style));
        }

        lines_rendered += 1;
        if lines_rendered < popup.max_visible {
            output.push_str("\r\n");
        }
    }
}

pub fn render_popup_standalone(popup: &PopupNode, width: usize) -> String {
    let mut output = String::new();
    render_popup(popup, width, &mut output);
    output
}

pub fn render_to_plain_text(node: &Node, width: usize) -> String {
    use crate::tui::oil::ansi::strip_ansi;
    strip_ansi(&render_to_string(node, width))
}

fn apply_style(content: &str, style: &Style) -> String {
    if style == &Style::default() {
        return content.to_string();
    }

    use crossterm::style::StyledContent;
    let ct_style = style.to_crossterm();
    format!("{}", StyledContent::new(ct_style, content))
}

pub fn print_to_stdout(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "{}", content)?;
    stdout.flush()
}

pub fn println_to_stdout(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", content)?;
    stdout.flush()
}
