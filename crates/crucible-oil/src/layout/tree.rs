//! LayoutTree intermediate representation for Taffy-based rendering.
//!
//! This module provides the bridge between Taffy layout computation and ANSI rendering.
//! The pipeline is: Node → Taffy → LayoutTree → render_layout_tree() → String

use crate::node::{BoxNode, Direction, InputNode, Node, PopupNode, RawNode, Size, SpinnerNode};
use crate::style::Style;

/// The root of a computed layout tree.
#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub root: LayoutBox,
    pub width: u16,
    pub height: u16,
}

/// A positioned box in the layout tree with computed dimensions.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Position and size computed by layout engine
    pub rect: LayoutRect,
    /// The content to render at this position
    pub content: LayoutContent,
    /// Child boxes (for containers)
    pub children: Vec<LayoutBox>,
    /// Style for rendering
    pub style: Style,
    /// Gap between children (for containers)
    pub gap: u16,
    /// Direction for containers
    pub direction: Direction,
    /// Optional key for Static nodes
    pub key: Option<String>,
}

/// Computed position and dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl LayoutRect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn zero() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

/// The content type for a layout box.
#[derive(Debug, Clone)]
pub enum LayoutContent {
    /// Empty node
    Empty,
    /// Text content with style
    Text {
        content: String,
        style: Style,
    },
    /// Container (Box or Fragment)
    Container,
    /// Input field
    Input(InputNode),
    /// Spinner
    Spinner(SpinnerNode),
    /// Popup menu
    Popup(PopupNode),
    Raw(RawNode),
}

impl LayoutBox {
    pub fn empty() -> Self {
        Self {
            rect: LayoutRect::zero(),
            content: LayoutContent::Empty,
            children: Vec::new(),
            style: Style::default(),
            gap: 0,
            direction: Direction::Column,
            key: None,
        }
    }

    pub fn text(rect: LayoutRect, content: String, style: Style) -> Self {
        Self {
            rect,
            content: LayoutContent::Text { content, style },
            children: Vec::new(),
            style: Style::default(),
            gap: 0,
            direction: Direction::Column,
            key: None,
        }
    }

    pub fn container(
        rect: LayoutRect,
        children: Vec<LayoutBox>,
        gap: u16,
        direction: Direction,
    ) -> Self {
        Self {
            rect,
            content: LayoutContent::Container,
            children,
            style: Style::default(),
            gap,
            direction,
            key: None,
        }
    }

    pub fn with_key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }
}

/// Build a LayoutTree from a Node tree.
///
/// This performs layout computation and creates the intermediate representation
/// that can be rendered to a string.
pub fn build_layout_tree(node: &Node, width: u16, height: u16) -> LayoutTree {
    let root = build_layout_box(node, LayoutRect::new(0, 0, width, height));
    LayoutTree {
        root,
        width,
        height,
    }
}

fn build_layout_box(node: &Node, available: LayoutRect) -> LayoutBox {
    match node {
        Node::Empty => LayoutBox::empty(),

        Node::Text(text) => {
            let content_height = measure_text_height(&text.content, available.width);
            LayoutBox::text(
                LayoutRect::new(
                    available.x,
                    available.y,
                    available.width,
                    content_height.min(available.height),
                ),
                text.content.clone(),
                text.style,
            )
        }

        Node::Box(boxnode) => build_box_layout(boxnode, available),

        Node::Static(static_node) => {
            let fragment = Node::Fragment(static_node.children.clone());
            let mut layout = build_layout_box(&fragment, available);
            layout.key = Some(static_node.key.clone());
            layout
        }

        Node::Input(input) => LayoutBox {
            rect: LayoutRect::new(available.x, available.y, available.width, 1),
            content: LayoutContent::Input(input.clone()),
            children: Vec::new(),
            style: Style::default(),
            gap: 0,
            direction: Direction::Column,
            key: None,
        },

        Node::Spinner(spinner) => LayoutBox {
            rect: LayoutRect::new(available.x, available.y, available.width, 1),
            content: LayoutContent::Spinner(spinner.clone()),
            children: Vec::new(),
            style: Style::default(),
            gap: 0,
            direction: Direction::Column,
            key: None,
        },

        Node::Popup(popup) => {
            let height = popup.max_visible.min(popup.items.len()) as u16;
            LayoutBox {
                rect: LayoutRect::new(available.x, available.y, available.width, height),
                content: LayoutContent::Popup(popup.clone()),
                children: Vec::new(),
                style: Style::default(),
                gap: 0,
                direction: Direction::Column,
                key: None,
            }
        }

        Node::Fragment(children) => {
            let mut child_layouts = Vec::new();
            let mut y = available.y;

            for child in children {
                if matches!(child, Node::Empty) {
                    continue;
                }
                let child_available = LayoutRect::new(
                    available.x,
                    y,
                    available.width,
                    available.height.saturating_sub(y - available.y),
                );
                let child_layout = build_layout_box(child, child_available);
                y += child_layout.rect.height;
                child_layouts.push(child_layout);
            }

            let total_height = y - available.y;
            LayoutBox::container(
                LayoutRect::new(available.x, available.y, available.width, total_height),
                child_layouts,
                0,
                Direction::Column,
            )
        }

        Node::Focusable(focusable) => build_layout_box(&focusable.child, available),

        Node::ErrorBoundary(boundary) => build_layout_box(&boundary.child, available),

        Node::Overlay(_) => LayoutBox::empty(),

        Node::Raw(raw) => LayoutBox {
            rect: LayoutRect::new(
                available.x,
                available.y,
                raw.display_width.min(available.width),
                raw.display_height.min(available.height),
            ),
            content: LayoutContent::Raw(raw.clone()),
            children: Vec::new(),
            style: Style::default(),
            gap: 0,
            direction: Direction::Column,
            key: None,
        },
    }
}

fn build_box_layout(boxnode: &BoxNode, available: LayoutRect) -> LayoutBox {
    let padding = &boxnode.padding;
    let margin = &boxnode.margin;
    let border_size = if boxnode.border.is_some() { 1 } else { 0 };

    let box_x = available.x + margin.left;
    let box_y = available.y + margin.top;
    let box_width = available.width.saturating_sub(margin.horizontal());
    let box_height = available.height.saturating_sub(margin.vertical());

    let inner_x = box_x + padding.left + border_size;
    let inner_y = box_y + padding.top + border_size;
    let inner_width = box_width
        .saturating_sub(padding.horizontal())
        .saturating_sub(border_size * 2);
    let inner_height = box_height
        .saturating_sub(padding.vertical())
        .saturating_sub(border_size * 2);

    if boxnode.children.is_empty() {
        let content_height = match boxnode.size {
            Size::Fixed(h) => h,
            Size::Flex(_) => box_height,
            Size::Content => 0,
        };

        return LayoutBox {
            rect: LayoutRect::new(box_x, box_y, box_width, content_height),
            content: LayoutContent::Container,
            children: Vec::new(),
            style: boxnode.style,
            gap: boxnode.gap.row,
            direction: boxnode.direction,
            key: None,
        };
    }

    match boxnode.direction {
        Direction::Column => build_column_layout(
            boxnode,
            box_x,
            box_y,
            box_width,
            inner_x,
            inner_y,
            inner_width,
            inner_height,
        ),
        Direction::Row => build_row_layout(
            boxnode,
            box_x,
            box_y,
            box_width,
            inner_x,
            inner_y,
            inner_width,
            inner_height,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_column_layout(
    boxnode: &BoxNode,
    box_x: u16,
    box_y: u16,
    box_width: u16,
    inner_x: u16,
    inner_y: u16,
    inner_width: u16,
    inner_height: u16,
) -> LayoutBox {
    let mut child_sizes: Vec<(usize, u16, bool)> = Vec::new();
    let mut total_fixed = 0u16;
    let mut total_flex = 0u16;

    for (i, child) in boxnode.children.iter().enumerate() {
        match get_child_size(child) {
            Size::Fixed(h) => {
                child_sizes.push((i, h, false));
                total_fixed += h;
            }
            Size::Flex(weight) => {
                child_sizes.push((i, weight, true));
                total_flex += weight;
            }
            Size::Content => {
                let content_h = measure_content_height(child, inner_width);
                child_sizes.push((i, content_h, false));
                total_fixed += content_h;
            }
        }
    }

    let gap = boxnode.gap.row;
    let total_gaps = gap * (boxnode.children.len().saturating_sub(1)) as u16;
    let remaining = inner_height
        .saturating_sub(total_fixed)
        .saturating_sub(total_gaps);
    let flex_unit = if total_flex > 0 {
        remaining / total_flex
    } else {
        0
    };

    let mut child_layouts = Vec::new();
    let mut y = inner_y;

    for (i, child) in boxnode.children.iter().enumerate() {
        if matches!(child, Node::Empty) {
            continue;
        }

        let (_, size_val, is_flex) = child_sizes[i];
        let child_height = if is_flex {
            size_val * flex_unit
        } else {
            size_val
        };

        let child_available = LayoutRect::new(inner_x, y, inner_width, child_height);
        let mut child_layout = build_layout_box(child, child_available);
        child_layout.rect.height = child_height;
        y += child_height;

        if i < boxnode.children.len() - 1 {
            y += gap;
        }

        child_layouts.push(child_layout);
    }

    let total_height =
        y - inner_y + boxnode.padding.vertical() + if boxnode.border.is_some() { 2 } else { 0 };
    let final_height = match boxnode.size {
        Size::Fixed(h) => h,
        Size::Flex(_) => {
            inner_height + boxnode.padding.vertical() + if boxnode.border.is_some() { 2 } else { 0 }
        }
        Size::Content => total_height,
    };

    LayoutBox {
        rect: LayoutRect::new(box_x, box_y, box_width, final_height),
        content: LayoutContent::Container,
        children: child_layouts,
        style: boxnode.style,
        gap,
        direction: Direction::Column,
        key: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_row_layout(
    boxnode: &BoxNode,
    box_x: u16,
    box_y: u16,
    box_width: u16,
    inner_x: u16,
    inner_y: u16,
    inner_width: u16,
    inner_height: u16,
) -> LayoutBox {
    let child_count = boxnode.children.len() as u16;
    if child_count == 0 {
        return LayoutBox {
            rect: LayoutRect::new(box_x, box_y, box_width, 1),
            content: LayoutContent::Container,
            children: Vec::new(),
            style: boxnode.style,
            gap: boxnode.gap.column,
            direction: Direction::Row,
            key: None,
        };
    }

    let child_width = inner_width / child_count;
    let mut child_layouts = Vec::new();
    let mut x = inner_x;
    let mut max_height = 0u16;

    for child in &boxnode.children {
        if matches!(child, Node::Empty) {
            continue;
        }
        let child_available = LayoutRect::new(x, inner_y, child_width, inner_height);
        let child_layout = build_layout_box(child, child_available);
        max_height = max_height.max(child_layout.rect.height);
        x += child_width;
        child_layouts.push(child_layout);
    }

    let total_height =
        max_height + boxnode.padding.vertical() + if boxnode.border.is_some() { 2 } else { 0 };
    let final_height = match boxnode.size {
        Size::Fixed(h) => h,
        Size::Content => total_height,
        Size::Flex(_) => {
            inner_height + boxnode.padding.vertical() + if boxnode.border.is_some() { 2 } else { 0 }
        }
    };

    LayoutBox {
        rect: LayoutRect::new(box_x, box_y, box_width, final_height),
        content: LayoutContent::Container,
        children: child_layouts,
        style: boxnode.style,
        gap: boxnode.gap.column,
        direction: Direction::Row,
        key: None,
    }
}

fn get_child_size(node: &Node) -> Size {
    match node {
        Node::Box(b) => b.size,
        _ => Size::Content,
    }
}

fn measure_content_height(node: &Node, width: u16) -> u16 {
    use crate::render::render_to_string;
    let rendered = render_to_string(node, width as usize);
    let lines = rendered.lines().count();
    (lines as u16).max(1)
}

fn measure_text_height(content: &str, width: u16) -> u16 {
    if content.is_empty() {
        return 0;
    }

    if width == 0 {
        return content.lines().count().max(1) as u16;
    }

    let mut total_lines = 0u16;
    for line in content.lines() {
        let chars = line.chars().count() as u16;
        if chars == 0 {
            total_lines += 1;
        } else {
            total_lines += chars.div_ceil(width);
        }
    }

    total_lines.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{col, row, text};

    #[test]
    fn test_build_simple_text() {
        let node = text("Hello");
        let tree = build_layout_tree(&node, 80, 24);

        assert_eq!(tree.width, 80);
        assert_eq!(tree.height, 24);
        matches!(tree.root.content, LayoutContent::Text { .. });
    }

    #[test]
    fn test_build_column() {
        let node = col([text("Line 1"), text("Line 2")]);
        let tree = build_layout_tree(&node, 80, 24);

        assert_eq!(tree.root.children.len(), 2);
        assert_eq!(tree.root.direction, Direction::Column);
    }

    #[test]
    fn test_build_row() {
        let node = row([text("Left"), text("Right")]);
        let tree = build_layout_tree(&node, 80, 24);

        assert_eq!(tree.root.children.len(), 2);
        assert_eq!(tree.root.direction, Direction::Row);
    }
}
