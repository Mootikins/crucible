//! Test helper functions for making structural assertions on LayoutTree.
//!
//! This module provides reusable assertion helpers that enable tests to verify
//! layout structure without string comparison, offering better error messages
//! and more readable test code.
//!
//! # Examples
//!
//! ```ignore
//! use crate::tui::oil::tests::layout_test_helpers::*;
//!
//! let tree = create_test_tree();
//!
//! // Assert node exists
//! assert_has_key(&tree, "my-button");
//!
//! // Assert node has specific text
//! assert_key_has_text(&tree, "my-button", "Click me");
//!
//! // Assert node position
//! assert_key_at_position(&tree, "my-button", 10, 5);
//!
//! // Assert node dimensions
//! assert_key_has_size(&tree, "my-button", 20, 1);
//! ```

use crate::tui::oil::layout::{LayoutBox, LayoutContent, LayoutTree, PopupItem};
use crucible_oil::layout::Rect;

/// Assert that a node with the given key exists in the tree.
///
/// # Panics
///
/// Panics if no node with the specified key is found.
///
/// # Example
///
/// ```ignore
/// assert_has_key(&tree, "my-button");
/// ```
pub fn assert_has_key(tree: &LayoutTree, key: &str) {
    assert!(
        tree.find_by_key(key).is_some(),
        "Expected node with key '{}' to exist in tree",
        key
    );
}

/// Assert that a node with the given key does NOT exist in the tree.
///
/// # Panics
///
/// Panics if a node with the specified key is found.
///
/// # Example
///
/// ```ignore
/// assert_no_key(&tree, "deleted-button");
/// ```
pub fn assert_no_key(tree: &LayoutTree, key: &str) {
    assert!(
        tree.find_by_key(key).is_none(),
        "Expected node with key '{}' to NOT exist in tree",
        key
    );
}

/// Assert that a node with the given key has specific text content.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The node has no text content
/// - The text doesn't match the expected value
///
/// # Example
///
/// ```ignore
/// assert_key_has_text(&tree, "my-button", "Click me");
/// ```
pub fn assert_key_has_text(tree: &LayoutTree, key: &str, expected: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    let text = node
        .content_text()
        .unwrap_or_else(|| panic!("Node '{}' has no extractable text content", key));

    assert_eq!(
        text, expected,
        "Node '{}' text mismatch.\nExpected: {:?}\nActual: {:?}",
        key, expected, text
    );
}

/// Assert that a node with the given key contains specific text (substring match).
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The node has no text content
/// - The text doesn't contain the expected substring
///
/// # Example
///
/// ```ignore
/// assert_key_contains_text(&tree, "status", "Loading");
/// ```
pub fn assert_key_contains_text(tree: &LayoutTree, key: &str, expected_substring: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    let text = node
        .content_text()
        .unwrap_or_else(|| panic!("Node '{}' has no extractable text content", key));

    assert!(
        text.contains(expected_substring),
        "Node '{}' text does not contain expected substring.\nExpected substring: {:?}\nActual text: {:?}",
        key, expected_substring, text
    );
}

/// Assert that a node with the given key is at a specific position.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The position doesn't match
///
/// # Example
///
/// ```ignore
/// assert_key_at_position(&tree, "my-button", 10, 5);
/// ```
pub fn assert_key_at_position(tree: &LayoutTree, key: &str, expected_x: u16, expected_y: u16) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert_eq!(
        node.rect.x, expected_x,
        "Node '{}' x position mismatch.\nExpected: {}\nActual: {}",
        key, expected_x, node.rect.x
    );

    assert_eq!(
        node.rect.y, expected_y,
        "Node '{}' y position mismatch.\nExpected: {}\nActual: {}",
        key, expected_y, node.rect.y
    );
}

/// Assert that a node with the given key has specific dimensions.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The dimensions don't match
///
/// # Example
///
/// ```ignore
/// assert_key_has_size(&tree, "my-button", 20, 1);
/// ```
pub fn assert_key_has_size(
    tree: &LayoutTree,
    key: &str,
    expected_width: u16,
    expected_height: u16,
) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert_eq!(
        node.rect.width, expected_width,
        "Node '{}' width mismatch.\nExpected: {}\nActual: {}",
        key, expected_width, node.rect.width
    );

    assert_eq!(
        node.rect.height, expected_height,
        "Node '{}' height mismatch.\nExpected: {}\nActual: {}",
        key, expected_height, node.rect.height
    );
}

/// Assert that a node with the given key has a specific bounding rectangle.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The rectangle doesn't match
///
/// # Example
///
/// ```ignore
/// assert_key_has_rect(&tree, "my-button", Rect::new(10, 5, 20, 1));
/// ```
pub fn assert_key_has_rect(tree: &LayoutTree, key: &str, expected_rect: Rect) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert_eq!(
        node.rect, expected_rect,
        "Node '{}' rectangle mismatch.\nExpected: {:?}\nActual: {:?}",
        key, expected_rect, node.rect
    );
}

/// Assert that a node with the given key has a specific number of children.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The child count doesn't match
///
/// # Example
///
/// ```ignore
/// assert_key_has_child_count(&tree, "container", 3);
/// ```
pub fn assert_key_has_child_count(tree: &LayoutTree, key: &str, expected_count: usize) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert_eq!(
        node.children.len(),
        expected_count,
        "Node '{}' child count mismatch.\nExpected: {}\nActual: {}",
        key,
        expected_count,
        node.children.len()
    );
}

/// Assert that a node with the given key has at least one child.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The node has no children
///
/// # Example
///
/// ```ignore
/// assert_key_has_children(&tree, "container");
/// ```
pub fn assert_key_has_children(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(!node.children.is_empty(), "Node '{}' has no children", key);
}

/// Assert that a node with the given key has no children.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The node has children
///
/// # Example
///
/// ```ignore
/// assert_key_has_no_children(&tree, "leaf-node");
/// ```
pub fn assert_key_has_no_children(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        node.children.is_empty(),
        "Node '{}' has {} children, expected 0",
        key,
        node.children.len()
    );
}

/// Assert that a node with the given key has a specific content type.
///
/// # Panics
///
/// Panics if:
/// - The node doesn't exist
/// - The content type doesn't match
///
/// # Example
///
/// ```ignore
/// assert_key_is_text(&tree, "my-label");
/// assert_key_is_input(&tree, "my-input");
/// assert_key_is_empty(&tree, "spacer");
/// ```
pub fn assert_key_is_text(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Text { .. }),
        "Node '{}' is not a Text node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is an Input node.
pub fn assert_key_is_input(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Input { .. }),
        "Node '{}' is not an Input node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is a Spinner node.
pub fn assert_key_is_spinner(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Spinner { .. }),
        "Node '{}' is not a Spinner node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is a Popup node.
pub fn assert_key_is_popup(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Popup { .. }),
        "Node '{}' is not a Popup node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is a Box node.
pub fn assert_key_is_box(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Box { .. }),
        "Node '{}' is not a Box node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is a Fragment node.
pub fn assert_key_is_fragment(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Fragment),
        "Node '{}' is not a Fragment node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that a node with the given key is an Empty node.
pub fn assert_key_is_empty(tree: &LayoutTree, key: &str) {
    let node = tree
        .find_by_key(key)
        .unwrap_or_else(|| panic!("Node '{}' not found in tree", key));

    assert!(
        matches!(node.content, LayoutContent::Empty),
        "Node '{}' is not an Empty node. Content: {:?}",
        key,
        node.content
    );
}

/// Assert that the tree structure matches expected debug output.
///
/// This is useful for verifying the overall tree hierarchy without
/// checking individual node properties.
///
/// # Panics
///
/// Panics if the debug output doesn't match the expected lines.
///
/// # Example
///
/// ```ignore
/// assert_tree_structure(&tree, &[
///     "LayoutTree (80x24)",
///     "└─ Box [0,0 80x24]",
///     "   ├─ Text \"Header\" [0,0 80x1]",
///     "   └─ Text \"Content\" [0,1 80x23]",
/// ]);
/// ```
pub fn assert_tree_structure(tree: &LayoutTree, expected_lines: &[&str]) {
    let debug = tree.debug_print();
    let actual_lines: Vec<&str> = debug.lines().collect();

    assert_eq!(
        actual_lines,
        expected_lines,
        "Tree structure mismatch.\nExpected:\n{}\n\nActual:\n{}",
        expected_lines.join("\n"),
        debug
    );
}

/// Assert that the tree debug output contains a specific substring.
///
/// Useful for checking that certain nodes or structures exist without
/// requiring exact structure matching.
///
/// # Panics
///
/// Panics if the debug output doesn't contain the expected substring.
///
/// # Example
///
/// ```ignore
/// assert_tree_contains(&tree, "Text \"Hello\"");
/// assert_tree_contains(&tree, "key=\"my-button\"");
/// ```
pub fn assert_tree_contains(tree: &LayoutTree, expected_substring: &str) {
    let debug = tree.debug_print();
    assert!(
        debug.contains(expected_substring),
        "Tree debug output does not contain expected substring.\nExpected: {:?}\n\nActual:\n{}",
        expected_substring,
        debug
    );
}

/// Assert that the tree debug output does NOT contain a specific substring.
///
/// # Panics
///
/// Panics if the debug output contains the unexpected substring.
///
/// # Example
///
/// ```ignore
/// assert_tree_not_contains(&tree, "deleted-node");
/// ```
pub fn assert_tree_not_contains(tree: &LayoutTree, unexpected_substring: &str) {
    let debug = tree.debug_print();
    assert!(
        !debug.contains(unexpected_substring),
        "Tree debug output unexpectedly contains substring.\nUnexpected: {:?}\n\nActual:\n{}",
        unexpected_substring,
        debug
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::style::Style;

    /// Helper to create a simple test tree
    fn create_simple_tree() -> LayoutTree {
        let child1 = LayoutBox::new(
            Rect::new(0, 0, 40, 1),
            LayoutContent::Text {
                content: "Child 1".to_string(),
                style: Style::default(),
            },
        )
        .with_key("child-1");

        let child2 = LayoutBox::new(
            Rect::new(40, 0, 40, 1),
            LayoutContent::Text {
                content: "Child 2".to_string(),
                style: Style::default(),
            },
        )
        .with_key("child-2");

        let root = LayoutBox::new(
            Rect::new(0, 0, 80, 24),
            LayoutContent::Box {
                border: None,
                style: Style::default(),
            },
        )
        .with_key("root")
        .with_children([child1, child2]);

        LayoutTree::new(root)
    }

    #[test]
    fn assert_has_key_succeeds() {
        let tree = create_simple_tree();
        assert_has_key(&tree, "root");
        assert_has_key(&tree, "child-1");
        assert_has_key(&tree, "child-2");
    }

    #[test]
    #[should_panic(expected = "Expected node with key 'nonexistent' to exist")]
    fn assert_has_key_fails_for_missing_key() {
        let tree = create_simple_tree();
        assert_has_key(&tree, "nonexistent");
    }

    #[test]
    fn assert_no_key_succeeds() {
        let tree = create_simple_tree();
        assert_no_key(&tree, "nonexistent");
    }

    #[test]
    #[should_panic(expected = "Expected node with key 'root' to NOT exist")]
    fn assert_no_key_fails_for_existing_key() {
        let tree = create_simple_tree();
        assert_no_key(&tree, "root");
    }

    #[test]
    fn assert_key_has_text_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_text(&tree, "child-1", "Child 1");
        assert_key_has_text(&tree, "child-2", "Child 2");
    }

    #[test]
    #[should_panic(expected = "text mismatch")]
    fn assert_key_has_text_fails_for_wrong_text() {
        let tree = create_simple_tree();
        assert_key_has_text(&tree, "child-1", "Wrong Text");
    }

    #[test]
    fn assert_key_contains_text_succeeds() {
        let tree = create_simple_tree();
        assert_key_contains_text(&tree, "child-1", "Child");
        assert_key_contains_text(&tree, "child-2", "2");
    }

    #[test]
    #[should_panic(expected = "does not contain expected substring")]
    fn assert_key_contains_text_fails_for_missing_substring() {
        let tree = create_simple_tree();
        assert_key_contains_text(&tree, "child-1", "Missing");
    }

    #[test]
    fn assert_key_at_position_succeeds() {
        let tree = create_simple_tree();
        assert_key_at_position(&tree, "child-1", 0, 0);
        assert_key_at_position(&tree, "child-2", 40, 0);
        assert_key_at_position(&tree, "root", 0, 0);
    }

    #[test]
    #[should_panic(expected = "x position mismatch")]
    fn assert_key_at_position_fails_for_wrong_x() {
        let tree = create_simple_tree();
        assert_key_at_position(&tree, "child-1", 10, 0);
    }

    #[test]
    #[should_panic(expected = "y position mismatch")]
    fn assert_key_at_position_fails_for_wrong_y() {
        let tree = create_simple_tree();
        assert_key_at_position(&tree, "child-1", 0, 10);
    }

    #[test]
    fn assert_key_has_size_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_size(&tree, "child-1", 40, 1);
        assert_key_has_size(&tree, "child-2", 40, 1);
        assert_key_has_size(&tree, "root", 80, 24);
    }

    #[test]
    #[should_panic(expected = "width mismatch")]
    fn assert_key_has_size_fails_for_wrong_width() {
        let tree = create_simple_tree();
        assert_key_has_size(&tree, "child-1", 50, 1);
    }

    #[test]
    #[should_panic(expected = "height mismatch")]
    fn assert_key_has_size_fails_for_wrong_height() {
        let tree = create_simple_tree();
        assert_key_has_size(&tree, "child-1", 40, 5);
    }

    #[test]
    fn assert_key_has_rect_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_rect(&tree, "child-1", Rect::new(0, 0, 40, 1));
        assert_key_has_rect(&tree, "child-2", Rect::new(40, 0, 40, 1));
    }

    #[test]
    #[should_panic(expected = "rectangle mismatch")]
    fn assert_key_has_rect_fails_for_wrong_rect() {
        let tree = create_simple_tree();
        assert_key_has_rect(&tree, "child-1", Rect::new(0, 0, 50, 2));
    }

    #[test]
    fn assert_key_has_child_count_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_child_count(&tree, "root", 2);
        assert_key_has_child_count(&tree, "child-1", 0);
    }

    #[test]
    #[should_panic(expected = "child count mismatch")]
    fn assert_key_has_child_count_fails_for_wrong_count() {
        let tree = create_simple_tree();
        assert_key_has_child_count(&tree, "root", 3);
    }

    #[test]
    fn assert_key_has_children_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_children(&tree, "root");
    }

    #[test]
    #[should_panic(expected = "has no children")]
    fn assert_key_has_children_fails_for_leaf_node() {
        let tree = create_simple_tree();
        assert_key_has_children(&tree, "child-1");
    }

    #[test]
    fn assert_key_has_no_children_succeeds() {
        let tree = create_simple_tree();
        assert_key_has_no_children(&tree, "child-1");
        assert_key_has_no_children(&tree, "child-2");
    }

    #[test]
    #[should_panic(expected = "has 2 children")]
    fn assert_key_has_no_children_fails_for_parent_node() {
        let tree = create_simple_tree();
        assert_key_has_no_children(&tree, "root");
    }

    #[test]
    fn assert_key_is_text_succeeds() {
        let tree = create_simple_tree();
        assert_key_is_text(&tree, "child-1");
        assert_key_is_text(&tree, "child-2");
    }

    #[test]
    #[should_panic(expected = "is not a Text node")]
    fn assert_key_is_text_fails_for_box_node() {
        let tree = create_simple_tree();
        assert_key_is_text(&tree, "root");
    }

    #[test]
    fn assert_key_is_box_succeeds() {
        let tree = create_simple_tree();
        assert_key_is_box(&tree, "root");
    }

    #[test]
    #[should_panic(expected = "is not a Box node")]
    fn assert_key_is_box_fails_for_text_node() {
        let tree = create_simple_tree();
        assert_key_is_box(&tree, "child-1");
    }

    #[test]
    fn assert_tree_contains_succeeds() {
        let tree = create_simple_tree();
        assert_tree_contains(&tree, "Text \"Child 1\"");
        assert_tree_contains(&tree, "key=\"root\"");
        assert_tree_contains(&tree, "Box");
    }

    #[test]
    #[should_panic(expected = "does not contain expected substring")]
    fn assert_tree_contains_fails_for_missing_substring() {
        let tree = create_simple_tree();
        assert_tree_contains(&tree, "Nonexistent");
    }

    #[test]
    fn assert_tree_not_contains_succeeds() {
        let tree = create_simple_tree();
        assert_tree_not_contains(&tree, "Nonexistent");
        assert_tree_not_contains(&tree, "key=\"missing\"");
    }

    #[test]
    #[should_panic(expected = "unexpectedly contains substring")]
    fn assert_tree_not_contains_fails_for_present_substring() {
        let tree = create_simple_tree();
        assert_tree_not_contains(&tree, "Child 1");
    }

    #[test]
    fn assert_key_is_input_succeeds() {
        let input_node = LayoutBox::new(
            Rect::new(0, 0, 40, 1),
            LayoutContent::Input {
                value: "test".to_string(),
                cursor: 0,
                placeholder: None,
                focused: false,
                style: Style::default(),
            },
        )
        .with_key("input");

        let tree = LayoutTree::new(input_node);
        assert_key_is_input(&tree, "input");
    }

    #[test]
    fn assert_key_is_spinner_succeeds() {
        let spinner_node = LayoutBox::new(
            Rect::new(0, 0, 10, 1),
            LayoutContent::Spinner {
                label: Some("Loading".to_string()),
                frame: 0,
                frames: None,
                style: Style::default(),
            },
        )
        .with_key("spinner");

        let tree = LayoutTree::new(spinner_node);
        assert_key_is_spinner(&tree, "spinner");
    }

    #[test]
    fn assert_key_is_popup_succeeds() {
        let popup_node = LayoutBox::new(
            Rect::new(10, 5, 30, 10),
            LayoutContent::Popup {
                items: vec![PopupItem::new("Item 1")],
                selected: 0,
                viewport_offset: 0,
                max_visible: 5,
            },
        )
        .with_key("popup");

        let tree = LayoutTree::new(popup_node);
        assert_key_is_popup(&tree, "popup");
    }

    #[test]
    fn assert_key_is_fragment_succeeds() {
        let fragment_node =
            LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Fragment).with_key("fragment");

        let tree = LayoutTree::new(fragment_node);
        assert_key_is_fragment(&tree, "fragment");
    }

    #[test]
    fn assert_key_is_empty_succeeds() {
        let empty_node =
            LayoutBox::new(Rect::new(0, 0, 80, 24), LayoutContent::Empty).with_key("empty");

        let tree = LayoutTree::new(empty_node);
        assert_key_is_empty(&tree, "empty");
    }
}
