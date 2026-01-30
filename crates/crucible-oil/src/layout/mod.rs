pub mod flex;
pub mod tree;
pub mod tree_render;

use crate::node::{BoxNode, Direction, Node, Size};
use crate::render::render_to_string;

pub use tree::{build_layout_tree, LayoutBox, LayoutContent, LayoutRect, LayoutTree};
pub use tree_render::{render_layout_tree, render_layout_tree_filtered};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub rect: Rect,
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    pub fn empty() -> Self {
        Self {
            rect: Rect::new(0, 0, 0, 0),
            children: Vec::new(),
        }
    }
}

pub fn calculate_layout(node: &Node, width: u16, height: u16) -> LayoutNode {
    let rect = Rect::new(0, 0, width, height);
    layout_node(node, &rect)
}

fn layout_node(node: &Node, available: &Rect) -> LayoutNode {
    match node {
        Node::Empty => LayoutNode::empty(),

        Node::Text(text) => {
            let content_height = measure_text_height(&text.content, available.width);
            LayoutNode {
                rect: Rect::new(
                    available.x,
                    available.y,
                    available.width,
                    content_height.min(available.height),
                ),
                children: Vec::new(),
            }
        }

        Node::Box(boxnode) => layout_box(boxnode, available),

        Node::Static(static_node) => {
            let fragment = Node::Fragment(static_node.children.clone());
            layout_node(&fragment, available)
        }

        Node::Input(_) => {
            let content_height = 1;
            LayoutNode {
                rect: Rect::new(
                    available.x,
                    available.y,
                    available.width,
                    content_height.min(available.height),
                ),
                children: Vec::new(),
            }
        }

        Node::Spinner(_) => LayoutNode {
            rect: Rect::new(available.x, available.y, available.width, 1),
            children: Vec::new(),
        },

        Node::Popup(popup) => {
            let height = popup.max_visible.min(popup.items.len()) as u16;
            LayoutNode {
                rect: Rect::new(available.x, available.y, available.width, height),
                children: Vec::new(),
            }
        }

        Node::Fragment(children) => {
            let mut child_layouts = Vec::new();
            let mut y = available.y;

            for child in children {
                let child_available = Rect::new(
                    available.x,
                    y,
                    available.width,
                    available.height.saturating_sub(y - available.y),
                );
                let child_layout = layout_node(child, &child_available);
                y += child_layout.rect.height;
                child_layouts.push(child_layout);
            }

            let total_height = y - available.y;
            LayoutNode {
                rect: Rect::new(available.x, available.y, available.width, total_height),
                children: child_layouts,
            }
        }

        Node::Focusable(focusable) => layout_node(&focusable.child, available),

        Node::ErrorBoundary(boundary) => layout_node(&boundary.child, available),

        Node::Overlay(_) => LayoutNode::empty(),

        Node::Raw(raw) => LayoutNode {
            rect: Rect::new(
                available.x,
                available.y,
                raw.display_width.min(available.width),
                raw.display_height.min(available.height),
            ),
            children: Vec::new(),
        },
    }
}

fn layout_box(boxnode: &BoxNode, available: &Rect) -> LayoutNode {
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

        return LayoutNode {
            rect: Rect::new(box_x, box_y, box_width, content_height),
            children: Vec::new(),
        };
    }

    match boxnode.direction {
        Direction::Column => layout_column(
            boxnode,
            box_x,
            box_y,
            box_width,
            inner_x,
            inner_y,
            inner_width,
            inner_height,
        ),
        Direction::Row => layout_row(
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
fn layout_column(
    boxnode: &BoxNode,
    box_x: u16,
    box_y: u16,
    box_width: u16,
    inner_x: u16,
    inner_y: u16,
    inner_width: u16,
    inner_height: u16,
) -> LayoutNode {
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

    let remaining = inner_height.saturating_sub(total_fixed);
    let flex_unit = if total_flex > 0 {
        remaining / total_flex
    } else {
        0
    };

    let mut child_layouts = Vec::new();
    let mut y = inner_y;
    let gap = boxnode.gap.row;

    for (i, child) in boxnode.children.iter().enumerate() {
        let (_, size_val, is_flex) = child_sizes[i];
        let child_height = if is_flex {
            size_val * flex_unit
        } else {
            size_val
        };

        let child_available = Rect::new(inner_x, y, inner_width, child_height);
        let mut child_layout = layout_node(child, &child_available);
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

    LayoutNode {
        rect: Rect::new(box_x, box_y, box_width, final_height),
        children: child_layouts,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_row(
    boxnode: &BoxNode,
    box_x: u16,
    box_y: u16,
    box_width: u16,
    inner_x: u16,
    inner_y: u16,
    inner_width: u16,
    inner_height: u16,
) -> LayoutNode {
    let child_count = boxnode.children.len() as u16;
    if child_count == 0 {
        return LayoutNode {
            rect: Rect::new(box_x, box_y, box_width, 1),
            children: Vec::new(),
        };
    }

    let child_width = inner_width / child_count;
    let mut child_layouts = Vec::new();
    let mut x = inner_x;
    let mut max_height = 0u16;

    for child in &boxnode.children {
        let child_available = Rect::new(x, inner_y, child_width, inner_height);
        let child_layout = layout_node(child, &child_available);
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

    LayoutNode {
        rect: Rect::new(box_x, box_y, box_width, final_height),
        children: child_layouts,
    }
}

fn get_child_size(node: &Node) -> Size {
    match node {
        Node::Box(b) => b.size,
        _ => Size::Content,
    }
}

fn measure_content_height(node: &Node, width: u16) -> u16 {
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
