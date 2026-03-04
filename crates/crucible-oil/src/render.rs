use crate::ansi::apply_style;
use crate::ansi::{visible_width, visual_rows};
use crate::cell_grid::CellGrid;
use crate::layout::flex::{calculate_row_widths, ChildMeasurement, FlexLayoutInput};
use crate::node::{
    BoxNode, Direction, InputNode, Node, PopupNode, RawNode, Size, SpinnerNode, TextNode,
};
use crate::style::Style;
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

pub fn render_to_plain_text(node: &Node, width: usize) -> String {
    let mut output = String::new();
    render_node_plain_text(node, width, &mut output);
    output
}

fn render_node_plain_text(node: &Node, width: usize, output: &mut String) {
    match node {
        Node::Raw(raw) => {
            output.push_str(&format!(
                "[raw: {}x{}]",
                raw.display_width, raw.display_height
            ));
        }
        Node::Fragment(children) => {
            for child in children {
                render_node_plain_text(child, width, output);
            }
        }
        Node::Box(_) => {
            // Delegate to the full layout engine so column/row direction,
            // spacer expansion, and width allocation are all honoured,
            // then strip ANSI codes and carriage returns (\r from \r\n).
            let rendered = render_to_string(node, width);
            let plain = crate::ansi::strip_ansi(&rendered).replace('\r', "");
            output.push_str(&plain);
        }
        Node::Static(static_node) => {
            for child in &static_node.children {
                render_node_plain_text(child, width, output);
            }
        }
        Node::Focusable(f) => render_node_plain_text(&f.child, width, output),
        Node::ErrorBoundary(b) => render_node_plain_text(&b.child, width, output),
        Node::Overlay(o) => render_node_plain_text(&o.child, width, output),
        other => {
            let rendered = render_to_string(other, width);
            output.push_str(&crate::ansi::strip_ansi(&rendered));
        }
    }
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

        Node::Raw(raw) => {
            render_raw(raw, width, output);
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
    if max_height > 1 || layout_result.total_used > width {
        render_row_to_grid(
            children,
            &child_infos,
            &layout_result.widths,
            width,
            max_height,
            output,
        );
    } else {
        render_row_single_line(child_infos, &layout_result.widths, output, cursor_info);
    }
}

fn render_row_single_line(
    child_infos: Vec<RowChildInfo>,
    widths: &[usize],
    output: &mut String,
    cursor_info: &mut CursorInfo,
) {
    for (i, child_info) in child_infos.into_iter().enumerate() {
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

fn render_raw(raw: &RawNode, width: usize, output: &mut String) {
    output.push_str(&raw.content);
    let pad = width.saturating_sub(raw.display_width as usize);
    if pad > 0 {
        output.push_str(&" ".repeat(pad));
    }
}

fn render_text(text: &TextNode, width: usize, output: &mut String) {
    let styled_content = apply_style(&text.content, &text.style);

    if width == 0 || text.content.chars().count() <= width {
        // Fast path: content fits on one line (or no width constraint)
        // But handle embedded newlines by converting them to \r\n
        if text.content.contains('\n') {
            let segments: Vec<&str> = text.content.split('\n').collect();
            for (i, segment) in segments.iter().enumerate() {
                if i > 0 {
                    output.push_str("\r\n");
                }
                output.push_str(&apply_style(segment, &text.style));
            }
        } else {
            output.push_str(&styled_content);
        }
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

fn get_node_size(node: &Node) -> Size {
    match node {
        Node::Box(b) => b.size,
        Node::Raw(raw) => Size::Fixed(raw.display_width),
        _ => Size::Content,
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
                line.push_str(&truncated);
                let after_desc_width = label_width + 2 + visible_width(&truncated);
                let padding = popup_width.saturating_sub(after_desc_width);
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                output.push_str(&apply_style(&line, &item_style));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::*;
    use crate::style::{Border, Color, Gap, Padding, Style};

    #[test]
    fn test_render_empty_node() {
        let node = Node::Empty;
        let result = render_to_string(&node, 80);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_simple_text() {
        let node = text("Hello, World!");
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_styled_text() {
        let style = Style::new().bold().fg(Color::Red);
        let node = styled("Bold Red", style);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Bold Red"));
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn test_render_column_with_gap() {
        let node = col(vec![text("Line 1"), text("Line 2"), text("Line 3")]);
        let result = render_to_string(&node, 80);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
    }

    #[test]
    fn test_render_row_simple() {
        let node = row(vec![text("A"), text("B"), text("C")]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("A"));
        assert!(result.contains("B"));
        assert!(result.contains("C"));
    }

    #[test]
    fn test_render_fragment() {
        let node = fragment(vec![text("First"), text("Second"), text("Third")]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
        assert!(result.contains("Third"));
    }

    #[test]
    fn test_render_input_unfocused() {
        let node = Node::Input(InputNode {
            value: "test input".to_string(),
            cursor: 0,
            placeholder: None,
            style: Style::default(),
            focused: false,
        });
        let result = render_to_string(&node, 80);
        assert_eq!(result, "test input");
    }

    #[test]
    fn test_render_input_with_placeholder() {
        let node = Node::Input(InputNode {
            value: String::new(),
            cursor: 0,
            placeholder: Some("Enter text...".to_string()),
            style: Style::default(),
            focused: false,
        });
        let result = render_to_string(&node, 80);
        assert!(result.contains("Enter text..."));
    }

    #[test]
    fn test_render_spinner() {
        let node = Node::Spinner(SpinnerNode {
            label: Some("Loading".to_string()),
            style: Style::default(),
            frame: 0,
            style_variant: None,
        });
        let result = render_to_string(&node, 80);
        assert!(result.contains("Loading"));
        assert!(result.contains("◐"));
    }

    #[test]
    fn test_render_spinner_no_label() {
        let node = Node::Spinner(SpinnerNode {
            label: None,
            style: Style::default(),
            frame: 1,
            style_variant: None,
        });
        let result = render_to_string(&node, 80);
        assert_eq!(result, "◓");
    }

    #[test]
    fn test_render_popup_single_item() {
        let items = vec![popup_item("Option 1")];
        let node = popup(items, 0, 5);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Option 1"));
    }

    #[test]
    fn test_render_popup_multiple_items() {
        let items = vec![
            popup_item("Option 1"),
            popup_item("Option 2"),
            popup_item("Option 3"),
        ];
        let node = popup(items, 1, 5);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Option 1"));
        assert!(result.contains("Option 2"));
        assert!(result.contains("Option 3"));
    }

    #[test]
    fn popup_description_has_no_style_reset_gap() {
        let items = vec![popup_item("Open file")
            .kind("CMD")
            .desc("Open a file in the current workspace")];
        let node = popup(items, 0, 1);
        let result = render_to_string(&node, 80);

        let content_line = result
            .lines()
            .find(|line| line.contains("Open file") && line.contains("current workspace"))
            .expect("expected popup line with label and description");

        assert!(
            content_line.matches("\x1b[48;2;").count() <= 1,
            "popup line should have a single background style span: {content_line:?}"
        );
    }

    #[test]
    fn test_render_focusable_node() {
        let node = focusable("test-id", text("Focusable content"));
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Focusable content");
    }

    #[test]
    fn test_render_error_boundary_success() {
        let node = error_boundary(text("Success"), text("Fallback"));
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Success");
    }

    #[test]
    fn test_render_overlay_node() {
        let node = overlay_from_bottom(text("Overlay content"), 5);
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Overlay content");
    }

    #[test]
    fn test_render_raw_node() {
        let node = raw("\\x1b[31mRed\\x1b[0m", 3, 1);
        let result = render_to_string(&node, 80);
        assert!(result.contains("\\x1b[31mRed\\x1b[0m"));
    }

    #[test]
    fn test_render_nested_col_row() {
        let node = col(vec![
            text("Header"),
            row(vec![text("A"), text("B")]),
            text("Footer"),
        ]);
        let result = render_to_string(&node, 80);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() >= 3);
        assert!(result.contains("Header"));
        assert!(result.contains("A"));
        assert!(result.contains("B"));
        assert!(result.contains("Footer"));
    }

    #[test]
    fn test_render_box_with_padding() {
        let boxnode = BoxNode {
            children: vec![text("Content")],
            direction: Direction::Column,
            padding: Padding {
                top: 1,
                bottom: 1,
                left: 2,
                right: 2,
            },
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_render_box_with_border() {
        let boxnode = BoxNode {
            children: vec![text("Content")],
            direction: Direction::Column,
            border: Some(Border::Single),
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_render_static_node() {
        let node = scrollback("key1", vec![text("Static content")]);
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Static content");
    }

    #[test]
    fn test_cursor_tracking_simple_input() {
        let node = Node::Input(InputNode {
            value: "hello".to_string(),
            cursor: 2,
            placeholder: None,
            style: Style::default(),
            focused: true,
        });
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 2);
        assert_eq!(result.cursor.row_from_end, 0);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_at_end() {
        let node = Node::Input(InputNode {
            value: "hello".to_string(),
            cursor: 5,
            placeholder: None,
            style: Style::default(),
            focused: true,
        });
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 5);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_with_padding() {
        let boxnode = BoxNode {
            children: vec![Node::Input(InputNode {
                value: "test".to_string(),
                cursor: 2,
                placeholder: None,
                style: Style::default(),
                focused: true,
            })],
            direction: Direction::Column,
            padding: Padding {
                top: 0,
                bottom: 0,
                left: 4,
                right: 0,
            },
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 6);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_with_border() {
        let boxnode = BoxNode {
            children: vec![Node::Input(InputNode {
                value: "test".to_string(),
                cursor: 1,
                placeholder: None,
                style: Style::default(),
                focused: true,
            })],
            direction: Direction::Column,
            border: Some(Border::Single),
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 2);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_nested_input() {
        let inner_box = Node::Box(BoxNode {
            children: vec![Node::Input(InputNode {
                value: "nested".to_string(),
                cursor: 3,
                placeholder: None,
                style: Style::default(),
                focused: true,
            })],
            direction: Direction::Column,
            padding: Padding {
                top: 0,
                bottom: 0,
                left: 2,
                right: 0,
            },
            ..Default::default()
        });

        let outer_box = Node::Box(BoxNode {
            children: vec![inner_box],
            direction: Direction::Column,
            padding: Padding {
                top: 0,
                bottom: 0,
                left: 3,
                right: 0,
            },
            ..Default::default()
        });

        let result = render_with_cursor(&outer_box, 80);
        assert_eq!(result.cursor.col, 8);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_multiline() {
        let node = col(vec![
            text("Line 1"),
            Node::Input(InputNode {
                value: "input".to_string(),
                cursor: 2,
                placeholder: None,
                style: Style::default(),
                focused: true,
            }),
            text("Line 3"),
        ]);
        let result = render_with_cursor(&node, 80);
        assert!(result.cursor.visible);
        // Cursor position is 2 on the input line, but "Line 1" is 6 chars,
        // so the cursor col accumulates: 6 (Line 1) + 2 (cursor in input) = 8
        assert_eq!(result.cursor.col, 8);
        // Cursor should be on the second line (row_from_end counts from bottom)
        // With 3 lines total, cursor on line 2 means row_from_end = 1
        assert!(result.cursor.row_from_end <= 1);
    }

    #[test]
    fn test_cursor_tracking_unfocused_input() {
        let node = Node::Input(InputNode {
            value: "unfocused".to_string(),
            cursor: 3,
            placeholder: None,
            style: Style::default(),
            focused: false,
        });
        let result = render_with_cursor(&node, 80);
        assert!(!result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_with_padding_and_border() {
        let boxnode = BoxNode {
            children: vec![Node::Input(InputNode {
                value: "test".to_string(),
                cursor: 2,
                placeholder: None,
                style: Style::default(),
                focused: true,
            })],
            direction: Direction::Column,
            padding: Padding {
                top: 0,
                bottom: 0,
                left: 3,
                right: 0,
            },
            border: Some(Border::Single),
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 6);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_render_column_with_custom_gap() {
        let boxnode = BoxNode {
            children: vec![text("A"), text("B"), text("C")],
            direction: Direction::Column,
            gap: Gap { row: 2, column: 0 },
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() >= 5);
    }

    #[test]
    fn test_render_empty_fragment() {
        let node = fragment(vec![]);
        let result = render_to_string(&node, 80);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_fragment_with_empty_nodes() {
        let node = fragment(vec![text("A"), Node::Empty, text("B")]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("A"));
        assert!(result.contains("B"));
    }

    #[test]
    fn test_render_text_embedded_newlines_use_crlf() {
        // Test that embedded newlines in text content are converted to \r\n
        let node = text("line1\nline2\nline3");
        let result = render_to_string(&node, 200); // width > total char count, triggers fast path

        // Should contain \r\n, not bare \n
        assert!(
            result.contains("line1\r\nline2\r\nline3"),
            "Expected \\r\\n between lines, got: {:?}",
            result
        );

        // Verify no bare \n without \r
        let lines: Vec<&str> = result.split("\r\n").collect();
        assert_eq!(lines.len(), 3, "Expected 3 lines separated by \\r\\n");
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");
    }
}
