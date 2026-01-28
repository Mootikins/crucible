//! Query operations on layout trees.
//!
//! This module provides utilities for searching and finding nodes in a layout tree,
//! useful for testing, debugging, and graduation tracking verification.

use super::types::{LayoutBox, LayoutContent, LayoutTree};

impl LayoutBox {
    /// Extract text content from this layout box for testing and assertions.
    ///
    /// Returns the text content for Text, Input, and Spinner nodes.
    /// Returns `None` for Box, Fragment, Empty, and Popup nodes.
    ///
    /// # Returns
    ///
    /// `Some(String)` containing the text content, or `None` if the box
    /// doesn't contain extractable text.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let text_box = LayoutBox::new(
    ///     Rect::new(0, 0, 10, 1),
    ///     LayoutContent::Text {
    ///         content: "Hello".to_string(),
    ///         style: Style::default(),
    ///     },
    /// );
    /// assert_eq!(text_box.content_text(), Some("Hello".to_string()));
    /// ```
    pub fn content_text(&self) -> Option<String> {
        match &self.content {
            LayoutContent::Text { content, .. } => Some(content.clone()),
            LayoutContent::Input { value, .. } => Some(value.clone()),
            LayoutContent::Spinner { label, .. } => label.clone(),
            LayoutContent::Box { .. } => None,
            LayoutContent::Fragment => None,
            LayoutContent::Empty => None,
            LayoutContent::Popup { .. } => None,
        }
    }
}

impl LayoutTree {
    /// Find a layout box by its Static key.
    ///
    /// Recursively searches the tree for a box with a matching key.
    /// Returns the first match found (depth-first search).
    ///
    /// # Arguments
    ///
    /// * `key` - The key to search for
    ///
    /// # Returns
    ///
    /// `Some(&LayoutBox)` if a box with the key is found, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tree = LayoutTree::new(root);
    /// if let Some(box_node) = tree.find_by_key("my-button") {
    ///     assert_eq!(box_node.rect.width, 10);
    /// }
    /// ```
    pub fn find_by_key(&self, key: &str) -> Option<&LayoutBox> {
        find_box_by_key(&self.root, key)
    }
}

/// Recursively search for a layout box by key.
fn find_box_by_key<'a>(box_node: &'a LayoutBox, key: &str) -> Option<&'a LayoutBox> {
    // Check if this box has the matching key
    if box_node.key.as_deref() == Some(key) {
        return Some(box_node);
    }

    // Recursively search children (depth-first)
    for child in &box_node.children {
        if let Some(found) = find_box_by_key(child, key) {
            return Some(found);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::layout::LayoutContent;
    use crucible_oil::layout::Rect;

    #[test]
    fn find_root_node_by_key() {
        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty).with_key("root");

        let tree = LayoutTree::new(root);
        let found = tree.find_by_key("root");

        assert!(found.is_some());
        assert_eq!(found.unwrap().key, Some("root".to_string()));
    }

    #[test]
    fn find_nested_node_by_key() {
        let child1 =
            LayoutBox::new(Rect::new(0, 0, 40, 12), LayoutContent::Empty).with_key("child-1");

        let child2 =
            LayoutBox::new(Rect::new(40, 0, 40, 12), LayoutContent::Empty).with_key("child-2");

        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty)
            .with_key("root")
            .with_children([child1, child2]);

        let tree = LayoutTree::new(root);

        // Find first child
        let found = tree.find_by_key("child-1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().key, Some("child-1".to_string()));

        // Find second child
        let found = tree.find_by_key("child-2");
        assert!(found.is_some());
        assert_eq!(found.unwrap().key, Some("child-2".to_string()));
    }

    #[test]
    fn find_deeply_nested_node() {
        let grandchild =
            LayoutBox::new(Rect::new(0, 0, 20, 6), LayoutContent::Empty).with_key("grandchild");

        let child = LayoutBox::new(Rect::new(0, 0, 40, 12), LayoutContent::Empty)
            .with_key("child")
            .with_child(grandchild);

        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty)
            .with_key("root")
            .with_child(child);

        let tree = LayoutTree::new(root);

        // Find grandchild through nested structure
        let found = tree.find_by_key("grandchild");
        assert!(found.is_some());
        assert_eq!(found.unwrap().key, Some("grandchild".to_string()));
    }

    #[test]
    fn find_nonexistent_key_returns_none() {
        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty).with_key("root");

        let tree = LayoutTree::new(root);
        let found = tree.find_by_key("nonexistent");

        assert!(found.is_none());
    }

    #[test]
    fn find_returns_first_match_in_dfs_order() {
        // Create a tree where multiple nodes could match
        // (though in practice keys should be unique)
        let child1 =
            LayoutBox::new(Rect::new(0, 0, 40, 12), LayoutContent::Empty).with_key("target");

        let child2 =
            LayoutBox::new(Rect::new(40, 0, 40, 12), LayoutContent::Empty).with_key("other");

        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty)
            .with_key("root")
            .with_children([child1, child2]);

        let tree = LayoutTree::new(root);

        // Should find the first "target" in depth-first order
        let found = tree.find_by_key("target");
        assert!(found.is_some());
        assert_eq!(found.unwrap().rect.x, 0);
        assert_eq!(found.unwrap().rect.y, 0);
    }

    #[test]
    fn find_node_without_key_returns_none() {
        let child_with_key =
            LayoutBox::new(Rect::new(0, 0, 40, 12), LayoutContent::Empty).with_key("child");

        let child_without_key = LayoutBox::new(Rect::new(40, 0, 40, 12), LayoutContent::Empty);

        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty)
            .with_key("root")
            .with_children([child_with_key, child_without_key]);

        let tree = LayoutTree::new(root);

        // Searching for a key that doesn't exist should return None
        let found = tree.find_by_key("missing");
        assert!(found.is_none());
    }

    #[test]
    fn find_in_empty_tree() {
        let tree = LayoutTree::empty();
        let found = tree.find_by_key("anything");

        assert!(found.is_none());
    }

    #[test]
    fn find_with_multiple_children_levels() {
        // Create a more complex tree structure
        let grandchild1 =
            LayoutBox::new(Rect::new(0, 0, 20, 6), LayoutContent::Empty).with_key("gc-1");

        let grandchild2 =
            LayoutBox::new(Rect::new(20, 0, 20, 6), LayoutContent::Empty).with_key("gc-2");

        let child1 = LayoutBox::new(Rect::new(0, 0, 40, 12), LayoutContent::Empty)
            .with_key("c-1")
            .with_children([grandchild1, grandchild2]);

        let child2 = LayoutBox::new(Rect::new(40, 0, 40, 12), LayoutContent::Empty).with_key("c-2");

        let root = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty)
            .with_key("root")
            .with_children([child1, child2]);

        let tree = LayoutTree::new(root);

        // Find various nodes at different depths
        assert!(tree.find_by_key("root").is_some());
        assert!(tree.find_by_key("c-1").is_some());
        assert!(tree.find_by_key("c-2").is_some());
        assert!(tree.find_by_key("gc-1").is_some());
        assert!(tree.find_by_key("gc-2").is_some());
        assert!(tree.find_by_key("nonexistent").is_none());
    }

    #[test]
    fn content_text_extracts_from_text_node() {
        use crucible_oil::style::Style;

        let box_node = LayoutBox::new(
            Rect::new(0, 0, 10, 1),
            LayoutContent::Text {
                content: "Hello".to_string(),
                style: Style::default(),
            },
        );

        assert_eq!(box_node.content_text(), Some("Hello".to_string()));
    }

    #[test]
    fn content_text_extracts_from_input_node() {
        use crucible_oil::style::Style;

        let box_node = LayoutBox::new(
            Rect::new(0, 0, 20, 1),
            LayoutContent::Input {
                value: "user input".to_string(),
                cursor: 5,
                placeholder: Some("Enter text".to_string()),
                focused: true,
                style: Style::default(),
            },
        );

        assert_eq!(box_node.content_text(), Some("user input".to_string()));
    }

    #[test]
    fn content_text_extracts_from_spinner_with_label() {
        use crucible_oil::style::Style;

        let box_node = LayoutBox::new(
            Rect::new(0, 0, 15, 1),
            LayoutContent::Spinner {
                label: Some("Loading".to_string()),
                frame: 0,
                frames: None,
                style: Style::default(),
            },
        );

        assert_eq!(box_node.content_text(), Some("Loading".to_string()));
    }

    #[test]
    fn content_text_returns_none_for_spinner_without_label() {
        use crucible_oil::style::Style;

        let box_node = LayoutBox::new(
            Rect::new(0, 0, 5, 1),
            LayoutContent::Spinner {
                label: None,
                frame: 0,
                frames: None,
                style: Style::default(),
            },
        );

        assert_eq!(box_node.content_text(), None);
    }

    #[test]
    fn content_text_returns_none_for_box() {
        use crucible_oil::style::Style;

        let box_node = LayoutBox::new(
            Rect::new(0, 0, 80, 24),
            LayoutContent::Box {
                border: None,
                style: Style::default(),
            },
        );

        assert_eq!(box_node.content_text(), None);
    }

    #[test]
    fn content_text_returns_none_for_fragment() {
        let box_node = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Fragment);

        assert_eq!(box_node.content_text(), None);
    }

    #[test]
    fn content_text_returns_none_for_empty() {
        let box_node = LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty);

        assert_eq!(box_node.content_text(), None);
    }

    #[test]
    fn content_text_returns_none_for_popup() {
        let box_node = LayoutBox::new(
            Rect::new(0, 0, 40, 10),
            LayoutContent::Popup {
                items: vec![],
                selected: 0,
                viewport_offset: 0,
                max_visible: 5,
            },
        );

        assert_eq!(box_node.content_text(), None);
    }
}
