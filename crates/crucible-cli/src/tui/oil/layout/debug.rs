//! Debug printing utilities for layout trees.
//!
//! Provides ASCII art visualization of layout tree structure for debugging
//! and understanding layout hierarchies without rendering.

use super::types::{LayoutBox, LayoutContent, LayoutTree};
use crate::tui::oil::utils::truncate_to_chars;

impl LayoutTree {
    /// Generate an ASCII art representation of the layout tree structure.
    ///
    /// Produces a tree diagram showing:
    /// - Content type (Box, Text, Input, Spinner, Popup, Fragment, Empty)
    /// - Position [x,y]
    /// - Size [width x height]
    /// - Key content (text preview, input value, etc.)
    /// - Tree structure with proper indentation
    ///
    /// # Example
    ///
    /// ```text
    /// LayoutTree (80x24)
    /// └─ Box [0,0 80x24]
    ///    ├─ Text [0,0 80x1] "Hello"
    ///    ├─ Box [0,1 80x10]
    ///    │  ├─ Text [0,1 40x1] "Left"
    ///    │  └─ Text [40,1 40x1] "Right"
    ///    └─ Input [0,11 80x1] value="test"
    /// ```
    pub fn debug_print(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "LayoutTree ({}x{})\n",
            self.root.rect.width, self.root.rect.height
        ));

        if self.root.children.is_empty() && matches!(self.root.content, LayoutContent::Empty) {
            output.push_str("└─ (empty)\n");
        } else {
            self.debug_print_box(&self.root, &mut output, "", true);
        }

        output
    }

    /// Recursively print a layout box and its children.
    fn debug_print_box(
        &self,
        box_node: &LayoutBox,
        output: &mut String,
        prefix: &str,
        is_last: bool,
    ) {
        // Determine the connector characters
        let connector = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "   " } else { "│  " };

        // Format the box info
        let box_info = format_box_info(box_node);
        output.push_str(&format!("{}{} {}\n", prefix, connector, box_info));

        // Process children
        let child_count = box_node.children.len();
        for (idx, child) in box_node.children.iter().enumerate() {
            let is_last_child = idx == child_count - 1;
            let child_prefix = format!("{}{}", prefix, continuation);
            self.debug_print_box(child, output, &child_prefix, is_last_child);
        }
    }
}

/// Format a single box into a debug string.
fn format_box_info(box_node: &LayoutBox) -> String {
    let rect = &box_node.rect;
    let pos_size = format!("[{},{} {}x{}]", rect.x, rect.y, rect.width, rect.height);

    let content_str = match &box_node.content {
        LayoutContent::Empty => "Empty".to_string(),

        LayoutContent::Text { content, .. } => {
            let preview = truncate_to_chars(content, 30, true);
            format!("Text \"{}\"", preview)
        }

        LayoutContent::Input {
            value,
            cursor,
            placeholder,
            focused,
            ..
        } => {
            let value_preview = truncate_to_chars(value, 20, true);
            let focus_indicator = if *focused { " [focused]" } else { "" };
            let placeholder_str = placeholder
                .as_ref()
                .map(|p| format!(" placeholder=\"{}\"", truncate_to_chars(p, 15, true)))
                .unwrap_or_default();
            format!(
                "Input value=\"{}\" cursor={}{}{}",
                value_preview, cursor, placeholder_str, focus_indicator
            )
        }

        LayoutContent::Spinner {
            label,
            frame,
            frames,
            ..
        } => {
            let label_str = label
                .as_ref()
                .map(|l| format!(" \"{}\"", l))
                .unwrap_or_default();
            let frame_info = if frames.is_some() {
                format!(" frame={}/custom", frame)
            } else {
                format!(" frame={}/default", frame)
            };
            format!("Spinner{}{}", label_str, frame_info)
        }

        LayoutContent::Popup {
            items,
            selected,
            viewport_offset,
            max_visible,
        } => {
            format!(
                "Popup items={} selected={} viewport_offset={} max_visible={}",
                items.len(),
                selected,
                viewport_offset,
                max_visible
            )
        }

        LayoutContent::Box { border, .. } => {
            let border_str = if border.is_some() { " [bordered]" } else { "" };
            format!("Box{}", border_str)
        }

        LayoutContent::Fragment => "Fragment".to_string(),
    };

    let key_str = box_node
        .key
        .as_ref()
        .map(|k| format!(" key=\"{}\"", k))
        .unwrap_or_default();

    format!("{} {} {}{}", content_str, pos_size, key_str, "")
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::layout::Rect;
    use crucible_oil::style::Style;

    #[test]
    fn debug_print_empty_tree() {
        let tree = LayoutTree::empty();
        let output = tree.debug_print();
        assert!(output.contains("LayoutTree (0x0)"));
        assert!(output.contains("(empty)"));
    }

    #[test]
    fn debug_print_simple_text() {
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 80, 1),
            LayoutContent::Text {
                content: "Hello World".to_string(),
                style: Style::default(),
            },
        );
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("LayoutTree (80x1)"));
        assert!(output.contains("Text \"Hello World\""));
        assert!(output.contains("[0,0 80x1]"));
    }

    #[test]
    fn debug_print_nested_structure() {
        let parent = LayoutBox::new(
            Rect::new(0, 0, 80, 24),
            LayoutContent::Box {
                border: None,
                style: Style::default(),
            },
        )
        .with_children([
            LayoutBox::new(
                Rect::new(0, 0, 80, 1),
                LayoutContent::Text {
                    content: "Header".to_string(),
                    style: Style::default(),
                },
            ),
            LayoutBox::new(
                Rect::new(0, 1, 80, 10),
                LayoutContent::Box {
                    border: None,
                    style: Style::default(),
                },
            )
            .with_children([
                LayoutBox::new(
                    Rect::new(0, 1, 40, 10),
                    LayoutContent::Text {
                        content: "Left".to_string(),
                        style: Style::default(),
                    },
                ),
                LayoutBox::new(
                    Rect::new(40, 1, 40, 10),
                    LayoutContent::Text {
                        content: "Right".to_string(),
                        style: Style::default(),
                    },
                ),
            ]),
        ]);

        let tree = LayoutTree::new(parent);
        let output = tree.debug_print();

        assert!(output.contains("LayoutTree (80x24)"));
        assert!(output.contains("Box [0,0 80x24]"));
        assert!(output.contains("Text \"Header\""));
        assert!(output.contains("Text \"Left\""));
        assert!(output.contains("Text \"Right\""));
        // Check tree structure with connectors
        assert!(output.contains("├─"));
        assert!(output.contains("└─"));
    }

    #[test]
    fn debug_print_input_field() {
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 40, 1),
            LayoutContent::Input {
                value: "test input".to_string(),
                cursor: 5,
                placeholder: Some("Enter text".to_string()),
                focused: true,
                style: Style::default(),
            },
        );
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("Input"));
        assert!(output.contains("value=\"test input\""));
        assert!(output.contains("cursor=5"));
        assert!(output.contains("placeholder=\"Enter text\""));
        assert!(output.contains("[focused]"));
    }

    #[test]
    fn debug_print_spinner() {
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Spinner {
                label: Some("Loading".to_string()),
                frame: 2,
                frames: None,
                style: Style::default(),
            },
        );
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("Spinner \"Loading\""));
        assert!(output.contains("frame=2/default"));
    }

    #[test]
    fn debug_print_popup() {
        let box_node = LayoutBox::new(
            Rect::new(10, 5, 30, 10),
            LayoutContent::Popup {
                items: vec![
                    super::super::PopupItem::new("Item 1"),
                    super::super::PopupItem::new("Item 2"),
                    super::super::PopupItem::new("Item 3"),
                ],
                selected: 1,
                viewport_offset: 0,
                max_visible: 5,
            },
        );
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("Popup"));
        assert!(output.contains("items=3"));
        assert!(output.contains("selected=1"));
        assert!(output.contains("viewport_offset=0"));
        assert!(output.contains("max_visible=5"));
    }

    #[test]
    fn debug_print_with_key() {
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 40, 1),
            LayoutContent::Text {
                content: "Keyed text".to_string(),
                style: Style::default(),
            },
        )
        .with_key("my-key");

        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("key=\"my-key\""));
    }

    #[test]
    fn debug_print_text_truncation() {
        let long_text = "a".repeat(50);
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 80, 1),
            LayoutContent::Text {
                content: long_text,
                style: Style::default(),
            },
        );
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        // Should be truncated to 30 chars + ellipsis
        assert!(output.contains("…"));
        assert!(!output.contains(&"a".repeat(50)));
    }

    #[test]
    fn debug_print_fragment() {
        let box_node = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Fragment);
        let tree = LayoutTree::new(box_node);
        let output = tree.debug_print();

        assert!(output.contains("Fragment"));
    }

    #[test]
    fn debug_print_complex_tree() {
        // Build a more complex tree structure
        let root = LayoutBox::new(
            Rect::new(0, 0, 80, 24),
            LayoutContent::Box {
                border: None,
                style: Style::default(),
            },
        )
        .with_children([
            LayoutBox::new(
                Rect::new(0, 0, 80, 2),
                LayoutContent::Text {
                    content: "Title".to_string(),
                    style: Style::default(),
                },
            ),
            LayoutBox::new(
                Rect::new(0, 2, 80, 20),
                LayoutContent::Box {
                    border: None,
                    style: Style::default(),
                },
            )
            .with_children([
                LayoutBox::new(
                    Rect::new(0, 2, 40, 20),
                    LayoutContent::Box {
                        border: None,
                        style: Style::default(),
                    },
                )
                .with_children([
                    LayoutBox::new(
                        Rect::new(0, 2, 40, 1),
                        LayoutContent::Text {
                            content: "Left Header".to_string(),
                            style: Style::default(),
                        },
                    ),
                    LayoutBox::new(
                        Rect::new(0, 3, 40, 19),
                        LayoutContent::Text {
                            content: "Left Content".to_string(),
                            style: Style::default(),
                        },
                    ),
                ]),
                LayoutBox::new(
                    Rect::new(40, 2, 40, 20),
                    LayoutContent::Box {
                        border: None,
                        style: Style::default(),
                    },
                )
                .with_children([
                    LayoutBox::new(
                        Rect::new(40, 2, 40, 1),
                        LayoutContent::Text {
                            content: "Right Header".to_string(),
                            style: Style::default(),
                        },
                    ),
                    LayoutBox::new(
                        Rect::new(40, 3, 40, 19),
                        LayoutContent::Input {
                            value: "input".to_string(),
                            cursor: 5,
                            placeholder: None,
                            focused: false,
                            style: Style::default(),
                        },
                    ),
                ]),
            ]),
            LayoutBox::new(
                Rect::new(0, 22, 80, 2),
                LayoutContent::Text {
                    content: "Footer".to_string(),
                    style: Style::default(),
                },
            ),
        ]);

        let tree = LayoutTree::new(root);
        let output = tree.debug_print();

        // Verify structure
        assert!(output.contains("LayoutTree (80x24)"));
        assert!(output.contains("Title"));
        assert!(output.contains("Left Header"));
        assert!(output.contains("Right Header"));
        assert!(output.contains("Footer"));
        assert!(output.contains("Input"));
        // Verify tree connectors are present
        assert!(output.contains("├─"));
        assert!(output.contains("└─"));
        assert!(output.contains("│"));
    }
}
