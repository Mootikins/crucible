//! Render a LayoutTree to an ANSI string.
//!
//! This module renders the computed layout tree to a string with ANSI escape codes.

use super::tree::{LayoutBox, LayoutContent, LayoutTree};
use crate::ansi::visible_width;
use crate::node::Direction;
use crate::style::{Color, Style};
use textwrap::{wrap, Options, WordSplitter};

/// Render a LayoutTree to a string with ANSI escape codes.
pub fn render_layout_tree(tree: &LayoutTree) -> String {
    let mut output = String::new();
    render_layout_box(&tree.root, tree.width as usize, &mut output);
    output
}

/// Render a LayoutTree with a filter for skipping Static nodes.
pub fn render_layout_tree_filtered<F>(tree: &LayoutTree, skip_key: F) -> String
where
    F: Fn(&str) -> bool,
{
    let mut output = String::new();
    render_layout_box_filtered(&tree.root, tree.width as usize, &skip_key, &mut output);
    output
}

fn render_layout_box(layout: &LayoutBox, width: usize, output: &mut String) {
    render_layout_box_filtered(layout, width, &|_| false, output);
}

fn render_layout_box_filtered<F>(
    layout: &LayoutBox,
    width: usize,
    skip_key: &F,
    output: &mut String,
) where
    F: Fn(&str) -> bool,
{
    // Check if this node should be skipped (Static node filtering)
    if let Some(key) = &layout.key {
        if skip_key(key) {
            return;
        }
    }

    match &layout.content {
        LayoutContent::Empty => {}

        LayoutContent::Text { content, style } => {
            render_text(content, style, layout.rect.width as usize, output);
        }

        LayoutContent::Container => {
            render_container_children(layout, width, skip_key, output);
        }

        LayoutContent::Input(input) => {
            render_input(input, output);
        }

        LayoutContent::Spinner(spinner) => {
            render_spinner(spinner, output);
        }

        LayoutContent::Popup(popup) => {
            render_popup(popup, width, output);
        }

        LayoutContent::Raw(raw) => {
            output.push_str(&raw.content);
        }
    }
}

fn render_container_children<F>(layout: &LayoutBox, width: usize, skip_key: &F, output: &mut String)
where
    F: Fn(&str) -> bool,
{
    match layout.direction {
        Direction::Column => {
            let gap = layout.gap;
            let mut rendered_any = false;

            for child in &layout.children {
                // Skip empty children
                if matches!(child.content, LayoutContent::Empty) {
                    continue;
                }

                // Check if child should be skipped
                if let Some(key) = &child.key {
                    if skip_key(key) {
                        continue;
                    }
                }

                if rendered_any && !output.is_empty() {
                    // Add newline between children
                    output.push_str("\r\n");
                    // Add gap lines
                    for _ in 0..gap {
                        output.push_str("\r\n");
                    }
                }

                render_layout_box_filtered(child, width, skip_key, output);
                rendered_any = true;
            }
        }

        Direction::Row => {
            for child in &layout.children {
                if matches!(child.content, LayoutContent::Empty) {
                    continue;
                }
                render_layout_box_filtered(child, child.rect.width as usize, skip_key, output);
            }
        }
    }
}

fn render_text(content: &str, style: &Style, width: usize, output: &mut String) {
    let styled_content = apply_style(content, style);

    if width == 0 || content.chars().count() <= width {
        output.push_str(&styled_content);
    } else {
        let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
        let wrapped: Vec<_> = wrap(content, options);

        for (i, line) in wrapped.iter().enumerate() {
            if i > 0 {
                output.push_str("\r\n");
            }
            output.push_str(&apply_style(line, style));
        }
    }
}

fn render_input(input: &crate::node::InputNode, output: &mut String) {
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

fn render_spinner(spinner: &crate::node::SpinnerNode, output: &mut String) {
    let frame_char = spinner.current_char();
    let styled_spinner = apply_style(&frame_char.to_string(), &spinner.style);

    output.push_str(&styled_spinner);

    if let Some(label) = &spinner.label {
        output.push(' ');
        output.push_str(&apply_style(label, &spinner.style));
    }
}

fn render_popup(popup: &crate::node::PopupNode, width: usize, output: &mut String) {
    let popup_width = width.saturating_sub(2);
    if popup_width == 0 {
        return;
    }

    let popup_bg = Color::Rgb(45, 50, 60);
    let selected_bg = Color::Rgb(60, 70, 90);

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
                let desc_style = Style::new().bg(bg).dim();
                output.push_str(&apply_style(&line, &Style::new().bg(bg)));
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
                output.push_str(&apply_style(&line, &Style::new().bg(bg)));
            }
        } else {
            let padding = popup_width.saturating_sub(label_width);
            line.push_str(&" ".repeat(padding));
            line.push(' ');
            output.push_str(&apply_style(&line, &Style::new().bg(bg)));
        }

        lines_rendered += 1;
        if lines_rendered < popup.max_visible {
            output.push_str("\r\n");
        }
    }
}

fn apply_style(content: &str, style: &Style) -> String {
    if style == &Style::default() {
        return content.to_string();
    }

    use crossterm::style::StyledContent;
    let ct_style = style.to_crossterm();
    format!("{}", StyledContent::new(ct_style, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tree::build_layout_tree;
    use crate::node::{col, row, text};

    #[test]
    fn test_render_simple_text() {
        let node = text("Hello, World!");
        let tree = build_layout_tree(&node, 80, 24);
        let output = render_layout_tree(&tree);

        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_render_column() {
        let node = col([text("Line 1"), text("Line 2")]);
        let tree = build_layout_tree(&node, 80, 24);
        let output = render_layout_tree(&tree);

        assert_eq!(output, "Line 1\r\nLine 2");
    }

    #[test]
    fn test_render_row() {
        let node = row([text("A"), text("B")]);
        let tree = build_layout_tree(&node, 80, 24);
        let output = render_layout_tree(&tree);

        // Row children are rendered side by side
        assert!(output.contains("A"));
        assert!(output.contains("B"));
    }

    #[test]
    fn test_render_with_filter() {
        use crate::node::scrollback;

        let node = col([
            scrollback("key1", [text("Should show")]),
            scrollback("key2", [text("Should hide")]),
        ]);
        let tree = build_layout_tree(&node, 80, 24);
        let output = render_layout_tree_filtered(&tree, |key| key == "key2");

        assert!(output.contains("Should show"));
        assert!(!output.contains("Should hide"));
    }
}
