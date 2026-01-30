//! Layout tree intermediate representation for Oil rendering.
//!
//! This module defines the `LayoutTree` IR that bridges Taffy layout computation
//! and ANSI rendering. The pipeline is:
//!
//! ```text
//! Node → Taffy → LayoutTree → render_layout_tree() → String
//! ```
//!
//! ## Key Types
//!
//! - [`LayoutTree`] - Root container holding the computed layout
//! - [`LayoutBox`] - A positioned box with content and children
//! - [`LayoutContent`] - The actual content to render (text, input, spinner, etc.)
//! - [`Rect`] - Re-exported from crucible-oil for position/size

use crucible_oil::layout::Rect;
use crucible_oil::style::Style;

/// Root container for a computed layout tree.
///
/// Created by `LayoutEngine::to_layout_tree()` after Taffy computes positions.
/// Consumed by `render_layout_tree()` to produce ANSI output.
#[derive(Debug, Clone)]
pub struct LayoutTree {
    /// The root layout box containing all content.
    pub root: LayoutBox,
}

impl LayoutTree {
    /// Create a new layout tree with the given root box.
    pub fn new(root: LayoutBox) -> Self {
        Self { root }
    }

    /// Create an empty layout tree with zero dimensions.
    pub fn empty() -> Self {
        Self {
            root: LayoutBox::empty(),
        }
    }
}

/// A positioned box in the layout tree.
///
/// Each `LayoutBox` has:
/// - A computed position and size (`rect`)
/// - Content to render (`content`)
/// - Child boxes for nested layouts (`children`)
/// - Visual styling (`style`)
///
/// The `rect` coordinates are absolute (relative to the viewport origin).
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Computed position and size from Taffy layout.
    pub rect: Rect,

    /// The content to render in this box.
    pub content: LayoutContent,

    /// Child boxes for nested layouts.
    pub children: Vec<LayoutBox>,

    /// Visual style (colors, bold, etc.) for this box.
    pub style: Style,

    /// Optional key for Static nodes (used for graduation tracking).
    pub key: Option<String>,
}

impl LayoutBox {
    /// Create a new layout box with the given rect and content.
    pub fn new(rect: Rect, content: LayoutContent) -> Self {
        Self {
            rect,
            content,
            children: Vec::new(),
            style: Style::default(),
            key: None,
        }
    }

    /// Create an empty layout box with zero dimensions.
    pub fn empty() -> Self {
        Self {
            rect: Rect::new(0, 0, 0, 0),
            content: LayoutContent::Empty,
            children: Vec::new(),
            style: Style::default(),
            key: None,
        }
    }

    /// Set the style for this box.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the key for this box (for Static node tracking).
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child box.
    pub fn with_child(mut self, child: LayoutBox) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple child boxes.
    pub fn with_children(mut self, children: impl IntoIterator<Item = LayoutBox>) -> Self {
        self.children.extend(children);
        self
    }
}

/// Content types that can be rendered in a layout box.
///
/// This enum mirrors the `Node` variants but contains only the data
/// needed for rendering (no layout hints like `Size` or `Direction`).
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutContent {
    /// No content (spacer, empty box).
    Empty,

    /// Text content with optional styling.
    Text {
        /// The text to render.
        content: String,
        /// Style applied to the text.
        style: Style,
    },

    /// Text input field.
    Input {
        /// Current input value.
        value: String,
        /// Cursor position (character index).
        cursor: usize,
        /// Placeholder text when empty.
        placeholder: Option<String>,
        /// Whether the input is focused.
        focused: bool,
        /// Style for the input.
        style: Style,
    },

    /// Animated spinner.
    Spinner {
        /// Optional label next to spinner.
        label: Option<String>,
        /// Current animation frame.
        frame: usize,
        /// Custom spinner frames (None = default).
        frames: Option<&'static [char]>,
        /// Style for the spinner.
        style: Style,
    },

    /// Popup/dropdown menu.
    Popup {
        /// Menu items.
        items: Vec<PopupItem>,
        /// Currently selected index.
        selected: usize,
        /// Scroll offset for long lists.
        viewport_offset: usize,
        /// Maximum visible items.
        max_visible: usize,
    },

    /// Container box (column or row).
    ///
    /// The actual layout is already computed in `LayoutBox.children`,
    /// this just indicates the box type for border/background rendering.
    Box {
        /// Border style if any.
        border: Option<crucible_oil::style::Border>,
        /// Background style.
        style: Style,
    },

    /// Fragment (transparent container, no visual representation).
    Fragment,

    Raw {
        content: String,
        display_width: u16,
        display_height: u16,
    },
}

impl Default for LayoutContent {
    fn default() -> Self {
        Self::Empty
    }
}

/// A single item in a popup menu.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupItem {
    /// Display label.
    pub label: String,
    /// Optional description/help text.
    pub description: Option<String>,
    /// Optional kind indicator (file, command, etc.).
    pub kind: Option<String>,
}

impl PopupItem {
    /// Create a new popup item with just a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
            kind: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the kind.
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_tree_empty() {
        let tree = LayoutTree::empty();
        assert_eq!(tree.root.rect.width, 0);
        assert_eq!(tree.root.rect.height, 0);
        assert!(matches!(tree.root.content, LayoutContent::Empty));
    }

    #[test]
    fn layout_box_builder() {
        let rect = Rect::new(0, 0, 80, 24);
        let content = LayoutContent::Text {
            content: "Hello".to_string(),
            style: Style::default(),
        };

        let box_node = LayoutBox::new(rect.clone(), content)
            .with_style(Style::new().bold())
            .with_key("test-key");

        assert_eq!(box_node.rect, rect);
        assert!(box_node.style.bold);
        assert_eq!(box_node.key, Some("test-key".to_string()));
    }

    #[test]
    fn layout_box_with_children() {
        let parent = LayoutBox::empty().with_children([
            LayoutBox::new(
                Rect::new(0, 0, 40, 1),
                LayoutContent::Text {
                    content: "Child 1".to_string(),
                    style: Style::default(),
                },
            ),
            LayoutBox::new(
                Rect::new(40, 0, 40, 1),
                LayoutContent::Text {
                    content: "Child 2".to_string(),
                    style: Style::default(),
                },
            ),
        ]);

        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn popup_item_builder() {
        let item = PopupItem::new("Label")
            .with_description("Description")
            .with_kind("file");

        assert_eq!(item.label, "Label");
        assert_eq!(item.description, Some("Description".to_string()));
        assert_eq!(item.kind, Some("file".to_string()));
    }

    #[test]
    fn layout_content_default() {
        let content: LayoutContent = Default::default();
        assert!(matches!(content, LayoutContent::Empty));
    }
}
