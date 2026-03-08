//! Render a LayoutTree to ANSI-formatted string output.
//!
//! This module provides the final step in the Oil rendering pipeline:
//!
//! ```text
//! Node → Taffy → LayoutTree → render_layout_tree() → (String, CursorInfo)
//! ```
//!
//! The renderer uses a 2D character buffer (CellGrid) to position content
//! at computed coordinates, then converts the buffer to an ANSI string.

use crate::ansi::apply_style;
use crate::cell_grid::CellGrid;
use crate::utils::visible_width;

use crate::render::CursorInfo;
use crate::render_helpers::{format_popup_item_line, select_spinner_frame, wrap_and_style_padded};
use crate::style::{Border, Style};

use super::types::{LayoutBox, LayoutContent, LayoutTree, PopupItem};

/// Render a LayoutTree to an ANSI-formatted string.
///
/// This function:
/// 1. Creates a 2D character buffer sized to the root rect
/// 2. Recursively renders each LayoutBox at its computed position
/// 3. Converts the buffer to a string with ANSI escape codes
///
/// # Arguments
///
/// * `tree` - The layout tree to render
///
/// # Returns
///
/// An ANSI-formatted string and cursor information.
pub fn render_layout_tree(tree: &LayoutTree) -> (String, CursorInfo) {
    let width = tree.root.rect.width as usize;
    let height = tree.root.rect.height as usize;

    if width == 0 || height == 0 {
        return (String::new(), CursorInfo::default());
    }

    let mut grid = CellGrid::new(width, height);
    let mut cursor_position = None;
    render_box_filtered(&tree.root, &mut grid, &|_| false, &mut cursor_position);
    let content = grid.to_string_joined();
    let cursor_info = cursor_info_from_position(cursor_position, content.lines().count());
    (content, cursor_info)
}

pub fn render_layout_tree_filtered<F>(tree: &LayoutTree, skip_key: F) -> (String, CursorInfo)
where
    F: Fn(&str) -> bool,
{
    let width = tree.root.rect.width as usize;
    let height = tree.root.rect.height as usize;

    if width == 0 || height == 0 {
        return (String::new(), CursorInfo::default());
    }

    let mut grid = CellGrid::new(width, height);
    let mut cursor_position = None;
    render_box_filtered(&tree.root, &mut grid, &skip_key, &mut cursor_position);

    // Trim trailing blank lines so fully-filtered trees produce empty string
    let lines = grid.to_lines();
    let trimmed: Vec<&str> = {
        let mut v: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        while v
            .last()
            .is_some_and(|l| l.is_empty() || l.chars().all(|c| c == ' ' || c == '\0'))
        {
            v.pop();
        }
        v
    };
    let content = trimmed.join("\r\n");
    let cursor_info = cursor_info_from_position(cursor_position, trimmed.len());
    (content, cursor_info)
}

fn cursor_info_from_position(
    cursor_position: Option<(u16, u16)>,
    rendered_line_count: usize,
) -> CursorInfo {
    if let Some((col, cursor_y)) = cursor_position {
        return CursorInfo {
            col,
            row_from_end: rendered_line_count.saturating_sub(cursor_y as usize + 1) as u16,
            visible: true,
        };
    }

    CursorInfo::default()
}

/// Recursively render a LayoutBox and its children to the grid.
fn render_box_filtered<F>(
    layout_box: &LayoutBox,
    grid: &mut CellGrid,
    skip_key: &F,
    cursor_position: &mut Option<(u16, u16)>,
) where
    F: Fn(&str) -> bool,
{
    if layout_box.key.as_deref().is_some_and(skip_key) {
        return;
    }

    let x = layout_box.rect.x as usize;
    let y = layout_box.rect.y as usize;
    let width = layout_box.rect.width as usize;
    let height = layout_box.rect.height as usize;

    // Render content based on type
    match &layout_box.content {
        LayoutContent::Empty => {
            // Nothing to render, just process children
        }

        LayoutContent::Text { content, style } => {
            render_text(content, style, x, y, width, grid);
        }

        LayoutContent::Input {
            value,
            cursor,
            placeholder,
            focused,
            style,
        } => {
            render_input(value, placeholder.as_deref(), style, x, y, grid);

            if *focused {
                let cursor_char_pos = (*cursor).min(value.chars().count());
                let cursor_col = visible_width(
                    &value[..value
                        .char_indices()
                        .nth(cursor_char_pos)
                        .map(|(i, _)| i)
                        .unwrap_or(value.len())],
                ) as u16;

                *cursor_position = Some((layout_box.rect.x + cursor_col, layout_box.rect.y));
            }
        }

        LayoutContent::Spinner {
            label,
            frame,
            frames,
            style,
        } => {
            render_spinner(label.as_deref(), *frame, *frames, style, x, y, grid);
        }

        LayoutContent::Popup {
            items,
            selected,
            viewport_offset,
            max_visible,
            bg_style,
            selected_style,
        } => {
            render_popup(
                items,
                *selected,
                *viewport_offset,
                *max_visible,
                bg_style,
                selected_style,
                x,
                y,
                width,
                grid,
            );
        }

        LayoutContent::Box { border, style } => {
            render_box_content(border.as_ref(), style, x, y, width, height, grid);
        }

        LayoutContent::Fragment => {
            // Transparent container, no visual representation
        }

        LayoutContent::Raw { content, .. } => {
            grid.blit_line(content, x, y);
        }
    }

    // Render children (later children can overwrite earlier ones for z-order)
    for child in &layout_box.children {
        render_box_filtered(child, grid, skip_key, cursor_position);
    }
}

/// Render styled text at the given position, wrapping within width bounds.
fn render_text(
    content: &str,
    style: &Style,
    x: usize,
    y: usize,
    width: usize,
    grid: &mut CellGrid,
) {
    if content.is_empty() || width == 0 {
        return;
    }

    // Use the shared helper to wrap and style the text
    let styled_lines = wrap_and_style_padded(content, style, width);

    for (row_idx, line) in styled_lines.iter().enumerate() {
        let target_y = y + row_idx;
        if target_y < grid.height() {
            grid.blit_line(line, x, target_y);
        }
    }
}

/// Render an input field at the given position.
#[allow(clippy::too_many_arguments)]
fn render_input(
    value: &str,
    placeholder: Option<&str>,
    style: &Style,
    x: usize,
    y: usize,
    grid: &mut CellGrid,
) {
    let display_text = if value.is_empty() {
        placeholder
            .map(|p| apply_style(p, &Style::default().dim()))
            .unwrap_or_default()
    } else {
        apply_style(value, style)
    };

    grid.blit_line(&display_text, x, y);
}

/// Render a spinner at the given position.
fn render_spinner(
    label: Option<&str>,
    frame: usize,
    frames: Option<&'static [char]>,
    style: &Style,
    x: usize,
    y: usize,
    grid: &mut CellGrid,
) {
    let frame_char = select_spinner_frame(frame, frames);
    let mut output = apply_style(&frame_char.to_string(), style);

    if let Some(label_text) = label {
        output.push(' ');
        output.push_str(&apply_style(label_text, style));
    }

    grid.blit_line(&output, x, y);
}

/// Render a popup menu at the given position.
#[allow(clippy::too_many_arguments)]
fn render_popup(
    items: &[PopupItem],
    selected: usize,
    viewport_offset: usize,
    max_visible: usize,
    bg_style: &Style,
    selected_style: &Style,
    x: usize,
    y: usize,
    width: usize,
    grid: &mut CellGrid,
) {
    use crate::node::{DEFAULT_POPUP_BG, DEFAULT_POPUP_SELECTED_BG};

    let popup_bg = bg_style.bg.unwrap_or(DEFAULT_POPUP_BG);
    let selected_bg = selected_style.bg.unwrap_or(DEFAULT_POPUP_SELECTED_BG);

    let visible_end = (viewport_offset + max_visible).min(items.len());
    let visible_items = &items[viewport_offset..visible_end];
    let item_count = visible_items.len();
    let blank_lines = max_visible.saturating_sub(item_count);

    let mut current_y = y;

    // Render blank lines first (for bottom-aligned popups)
    for _ in 0..blank_lines {
        let blank_line = apply_style(&" ".repeat(width), &Style::new().bg(popup_bg));
        grid.blit_line(&blank_line, x, current_y);
        current_y += 1;
    }

    // Render visible items
    for (i, item) in visible_items.iter().enumerate() {
        let actual_index = viewport_offset + i;
        let is_selected = actual_index == selected;
        let bg = if is_selected { selected_bg } else { popup_bg };

        let line = format_popup_item_line(
            is_selected,
            item.kind.as_deref(),
            &item.label,
            item.description.as_deref(),
            width,
        );

        let styled_line = apply_style(&line, &Style::new().bg(bg));
        grid.blit_line(&styled_line, x, current_y);
        current_y += 1;
    }
}

/// Render a box container with optional border.
fn render_box_content(
    border: Option<&Border>,
    style: &Style,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    grid: &mut CellGrid,
) {
    if let Some(border) = border {
        let chars = border.chars();
        let inner_width = width.saturating_sub(2);

        // Top border
        let top = format!(
            "{}{}{}",
            chars.top_left,
            chars.horizontal.to_string().repeat(inner_width),
            chars.top_right
        );
        grid.blit_line(&apply_style(&top, style), x, y);

        // Side borders for each row
        for row in 1..height.saturating_sub(1) {
            let target_y = y + row;
            if target_y < grid.height() {
                grid.blit_line(
                    &apply_style(&chars.vertical.to_string(), style),
                    x,
                    target_y,
                );
                grid.blit_line(
                    &apply_style(&chars.vertical.to_string(), style),
                    x + width.saturating_sub(1),
                    target_y,
                );
            }
        }

        // Bottom border
        if height > 1 {
            let bottom = format!(
                "{}{}{}",
                chars.bottom_left,
                chars.horizontal.to_string().repeat(inner_width),
                chars.bottom_right
            );
            grid.blit_line(
                &apply_style(&bottom, style),
                x,
                y + height.saturating_sub(1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::{strip_ansi, visible_width};
    use crate::layout::Rect;
    use crate::utils::truncate_to_width;

    #[test]
    fn render_empty_tree() {
        let tree = LayoutTree::empty();
        let (result, _) = render_layout_tree(&tree);
        assert_eq!(result, "");
    }

    #[test]
    fn render_simple_text() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Text {
                content: "Hello".to_string(),
                style: Style::default(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("Hello"));
    }

    #[test]
    fn render_text_at_position() {
        let tree = LayoutTree::new(
            LayoutBox::new(Rect::new(0, 0, 20, 3), LayoutContent::Empty).with_child(
                LayoutBox::new(
                    Rect::new(5, 1, 10, 1),
                    LayoutContent::Text {
                        content: "Test".to_string(),
                        style: Style::default(),
                    },
                ),
            ),
        );

        let (result, _) = render_layout_tree(&tree);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        // Text should be at x=5, y=1
        assert!(lines[1].contains("Test"));
    }

    #[test]
    fn render_styled_text() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Text {
                content: "Bold".to_string(),
                style: Style::new().bold(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        // Should contain ANSI bold code
        assert!(result.contains("\x1b["));
        assert!(result.contains("Bold"));
    }

    #[test]
    fn render_input_with_value() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Input {
                value: "typed text".to_string(),
                cursor: 5,
                placeholder: Some("placeholder".to_string()),
                focused: true,
                style: Style::default(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("typed text"));
        assert!(!result.contains("placeholder"));
    }

    #[test]
    fn render_input_with_placeholder() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Input {
                value: String::new(),
                cursor: 0,
                placeholder: Some("placeholder".to_string()),
                focused: false,
                style: Style::default(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("placeholder"));
    }

    #[test]
    fn render_layout_tree_filtered_tracks_cursor_for_focused_input() {
        let tree = LayoutTree::new(
            LayoutBox::new(Rect::new(0, 0, 70, 4), LayoutContent::Empty).with_child(
                LayoutBox::new(
                    Rect::new(5, 2, 70, 1),
                    LayoutContent::Input {
                        value: "hello".to_string(),
                        cursor: 3,
                        placeholder: None,
                        focused: true,
                        style: Style::default(),
                    },
                ),
            ),
        );

        let (_, cursor_info) = render_layout_tree_filtered(&tree, |_| false);

        assert!(cursor_info.visible);
        assert_eq!(cursor_info.col, 8);
        assert_eq!(cursor_info.row_from_end, 0);
    }

    #[test]
    fn render_layout_tree_filtered_hides_cursor_without_focused_input() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Text {
                content: "just text".to_string(),
                style: Style::default(),
            },
        ));

        let (_, cursor_info) = render_layout_tree_filtered(&tree, |_| false);

        assert!(!cursor_info.visible);
    }

    #[test]
    fn render_spinner() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Spinner {
                label: Some("Loading".to_string()),
                frame: 0,
                frames: None,
                style: Style::default(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("Loading"));
        assert!(result.contains('◐'));
    }

    #[test]
    fn render_nested_boxes() {
        let child1 = LayoutBox::new(
            Rect::new(0, 0, 10, 1),
            LayoutContent::Text {
                content: "Child1".to_string(),
                style: Style::default(),
            },
        );

        let child2 = LayoutBox::new(
            Rect::new(0, 1, 10, 1),
            LayoutContent::Text {
                content: "Child2".to_string(),
                style: Style::default(),
            },
        );

        let tree = LayoutTree::new(
            LayoutBox::new(Rect::new(0, 0, 20, 3), LayoutContent::Empty)
                .with_child(child1)
                .with_child(child2),
        );

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("Child1"));
        assert!(result.contains("Child2"));
    }

    #[test]
    fn render_box_with_border() {
        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 10, 3),
            LayoutContent::Box {
                border: Some(Border::Single),
                style: Style::default(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        // Should contain border characters
        assert!(result.contains('┌'));
        assert!(result.contains('┐'));
        assert!(result.contains('└'));
        assert!(result.contains('┘'));
    }

    #[test]
    fn visible_width_excludes_ansi() {
        assert_eq!(visible_width("hello"), 5);
        assert_eq!(visible_width("\x1b[31mhello\x1b[0m"), 5);
        assert_eq!(visible_width("\x1b[1;31mtest\x1b[0m"), 4);
    }

    #[test]
    fn truncate_preserves_ansi() {
        let styled = "\x1b[31mhello world\x1b[0m";
        let truncated = truncate_to_width(styled, 5, false);
        assert!(truncated.contains("\x1b[31m"));
        assert!(truncated.contains("hello"));
        assert!(!truncated.contains("world"));
    }

    #[test]
    fn render_popup_items() {
        let items = vec![
            PopupItem::new("Item 1"),
            PopupItem::new("Item 2"),
            PopupItem::new("Item 3"),
        ];

        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 30, 3),
            LayoutContent::Popup {
                items,
                selected: 1,
                viewport_offset: 0,
                max_visible: 3,
                bg_style: Style::new(),
                selected_style: Style::new(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
        assert!(result.contains("Item 3"));
        // Selected item should have indicator
        assert!(result.contains("▸"));
    }

    #[test]
    fn render_popup_items_with_descriptions() {
        let items = vec![
            PopupItem::new("Open").with_description("Open file"),
            PopupItem::new("Save").with_description("Save current buffer to disk"),
            PopupItem::new("Quit"),
        ];

        let tree = LayoutTree::new(LayoutBox::new(
            Rect::new(0, 0, 34, 3),
            LayoutContent::Popup {
                items,
                selected: 0,
                viewport_offset: 0,
                max_visible: 3,
                bg_style: Style::new(),
                selected_style: Style::new(),
            },
        ));

        let (result, _) = render_layout_tree(&tree);
        assert!(result.contains("Open file"));
        assert!(result.contains("Save current buffer"));
        assert!(!result.contains("Save current buffer to disk"));

        let quit_line = result
            .lines()
            .find(|line| line.contains("Quit"))
            .expect("expected Quit item to render");
        let plain_quit_line = strip_ansi(quit_line);
        assert!(!plain_quit_line.contains("Quit  Save"));
        assert!(!plain_quit_line.contains("Quit  Open"));
    }

    #[test]
    fn z_order_later_children_overwrite() {
        // First child writes "AAAA" at position 0
        let child1 = LayoutBox::new(
            Rect::new(0, 0, 4, 1),
            LayoutContent::Text {
                content: "AAAA".to_string(),
                style: Style::default(),
            },
        );

        // Second child writes "BB" at position 1, should overwrite
        let child2 = LayoutBox::new(
            Rect::new(1, 0, 2, 1),
            LayoutContent::Text {
                content: "BB".to_string(),
                style: Style::default(),
            },
        );

        let tree = LayoutTree::new(
            LayoutBox::new(Rect::new(0, 0, 10, 1), LayoutContent::Empty)
                .with_child(child1)
                .with_child(child2),
        );

        let (result, _) = render_layout_tree(&tree);
        // Should be "ABBA" followed by spaces
        assert!(result.starts_with("ABB"));
    }
}
