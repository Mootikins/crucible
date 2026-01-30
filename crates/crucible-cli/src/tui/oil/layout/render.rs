//! Render a LayoutTree to ANSI-formatted string output.
//!
//! This module provides the final step in the Oil rendering pipeline:
//!
//! ```text
//! Node → Taffy → LayoutTree → render_layout_tree() → String
//! ```
//!
//! The renderer uses a 2D character buffer (CellGrid) to position content
//! at computed coordinates, then converts the buffer to an ANSI string.

use crate::tui::oil::cell_grid::CellGrid;
use crate::tui::oil::utils::truncate_to_width;
use crucible_oil::ansi::apply_style;
use crucible_oil::layout::Rect;
use crucible_oil::style::{Border, Style};

use super::types::{LayoutBox, LayoutContent, LayoutTree, PopupItem};

/// Default spinner frames used when none are specified.
const DEFAULT_SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

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
/// An ANSI-formatted string representing the rendered layout.
pub fn render_layout_tree(tree: &LayoutTree) -> String {
    let width = tree.root.rect.width as usize;
    let height = tree.root.rect.height as usize;

    if width == 0 || height == 0 {
        return String::new();
    }

    let mut grid = CellGrid::new(width, height);
    render_box(&tree.root, &mut grid);
    grid.to_string_joined()
}

/// Recursively render a LayoutBox and its children to the grid.
fn render_box(layout_box: &LayoutBox, grid: &mut CellGrid) {
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
            render_input(
                value,
                *cursor,
                placeholder.as_deref(),
                *focused,
                style,
                x,
                y,
                grid,
            );
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
        } => {
            render_popup(
                items,
                *selected,
                *viewport_offset,
                *max_visible,
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
        render_box(child, grid);
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

    let styled = apply_style(content, style);

    // Handle text wrapping
    let lines: Vec<&str> = styled.lines().collect();
    for (row_idx, line) in lines.iter().enumerate() {
        let target_y = y + row_idx;
        if target_y < grid.height() {
            // Truncate line to width if needed
            let truncated = truncate_to_width(line, width, false);
            grid.blit_line(&truncated, x, target_y);
        }
    }
}

/// Render an input field at the given position.
#[allow(clippy::too_many_arguments)]
fn render_input(
    value: &str,
    _cursor: usize,
    placeholder: Option<&str>,
    _focused: bool,
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
    let spinner_frames = frames.unwrap_or(DEFAULT_SPINNER_FRAMES);
    let frame_char = spinner_frames
        .get(frame % spinner_frames.len())
        .copied()
        .unwrap_or('⠋');

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
    x: usize,
    y: usize,
    width: usize,
    grid: &mut CellGrid,
) {
    use crucible_oil::style::Color;

    let popup_bg = Color::Rgb(45, 50, 60);
    let selected_bg = Color::Rgb(60, 70, 90);

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

        // Calculate available space for label
        let prefix_width = visible_width(&line);
        let max_label_width = width.saturating_sub(prefix_width + 2);
        let label = if item.label.chars().count() > max_label_width && max_label_width > 4 {
            let s: String = item.label.chars().take(max_label_width - 1).collect();
            format!("{}…", s)
        } else {
            item.label.clone()
        };
        line.push_str(&label);

        // Pad to full width
        let current_width = visible_width(&line);
        let padding = width.saturating_sub(current_width + 1);
        line.push_str(&" ".repeat(padding));
        line.push(' ');

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

/// Apply ANSI styling to content.

/// Calculate visible width of a string (excluding ANSI escape codes).
fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ANSI escape sequence
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
            width += 1;
        }
    }

    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_empty_tree() {
        let tree = LayoutTree::empty();
        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
        assert!(result.contains("placeholder"));
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

        let result = render_layout_tree(&tree);
        assert!(result.contains("Loading"));
        // Should contain spinner character
        assert!(result.contains('⠋'));
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

        let result = render_layout_tree(&tree);
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

        let result = render_layout_tree(&tree);
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
            },
        ));

        let result = render_layout_tree(&tree);
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
        assert!(result.contains("Item 3"));
        // Selected item should have indicator
        assert!(result.contains("▸"));
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

        let result = render_layout_tree(&tree);
        // Should be "ABBA" followed by spaces
        assert!(result.starts_with("ABB"));
    }
}
