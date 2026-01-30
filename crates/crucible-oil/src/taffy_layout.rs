use crate::node::{BoxNode, Direction, Node, Size as InkSize};
use crate::style::{AlignItems as InkAlignItems, JustifyContent as InkJustifyContent};
use std::collections::HashMap;
use taffy::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct ComputedLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct LayoutEngine {
    tree: TaffyTree<usize>,
    node_map: HashMap<usize, NodeId>,
    next_id: usize,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            node_map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn compute(
        &mut self,
        node: &Node,
        width: f32,
        height: f32,
    ) -> HashMap<usize, ComputedLayout> {
        self.tree = TaffyTree::new();
        self.node_map.clear();
        self.next_id = 0;

        let root_id = self.build_node(node, width);

        let available = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };

        self.tree.compute_layout(root_id, available).ok();

        let mut layouts = HashMap::new();
        self.collect_layouts(root_id, 0.0, 0.0, &mut layouts);
        layouts
    }

    fn build_node(&mut self, node: &Node, available_width: f32) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;

        let node_id = match node {
            Node::Empty => self
                .tree
                .new_leaf(Style {
                    display: Display::None,
                    ..Default::default()
                })
                .unwrap(),

            Node::Text(text) => {
                let lines = measure_text_lines(&text.content, available_width as usize);
                self.tree
                    .new_leaf(Style {
                        size: Size {
                            width: length(available_width),
                            height: length(lines as f32),
                        },
                        ..Default::default()
                    })
                    .unwrap()
            }

            Node::Box(boxnode) => self.build_box(boxnode, available_width),

            Node::Static(static_node) => {
                let fragment = Node::Fragment(static_node.children.clone());
                return self.build_node(&fragment, available_width);
            }

            Node::Input(_) => self
                .tree
                .new_leaf(Style {
                    size: Size {
                        width: length(available_width),
                        height: length(1.0),
                    },
                    ..Default::default()
                })
                .unwrap(),

            Node::Spinner(_) => self
                .tree
                .new_leaf(Style {
                    size: Size {
                        width: auto(),
                        height: length(1.0),
                    },
                    ..Default::default()
                })
                .unwrap(),

            Node::Popup(popup) => {
                let height = popup.max_visible.min(popup.items.len()) as f32;
                self.tree
                    .new_leaf(Style {
                        size: Size {
                            width: length(available_width),
                            height: length(height),
                        },
                        ..Default::default()
                    })
                    .unwrap()
            }

            Node::Fragment(children) => {
                let child_ids: Vec<NodeId> = children
                    .iter()
                    .map(|c| self.build_node(c, available_width))
                    .collect();

                self.tree
                    .new_with_children(
                        Style {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Column,
                            size: Size {
                                width: length(available_width),
                                height: auto(),
                            },
                            ..Default::default()
                        },
                        &child_ids,
                    )
                    .unwrap()
            }

            Node::Focusable(focusable) => {
                return self.build_node(&focusable.child, available_width);
            }

            Node::ErrorBoundary(boundary) => {
                return self.build_node(&boundary.child, available_width);
            }

            Node::Overlay(_) => self.tree.new_leaf(taffy::style::Style::default()).unwrap(),

            Node::Raw(raw) => self
                .tree
                .new_leaf(Style {
                    size: Size {
                        width: length(raw.display_width as f32),
                        height: length(raw.display_height as f32),
                    },
                    ..Default::default()
                })
                .unwrap(),
        };

        self.node_map.insert(id, node_id);
        node_id
    }

    fn build_box(&mut self, boxnode: &BoxNode, available_width: f32) -> NodeId {
        let padding = &boxnode.padding;
        let margin = &boxnode.margin;
        let border_width = if boxnode.border.is_some() { 1.0 } else { 0.0 };

        let inner_width =
            available_width - padding.left as f32 - padding.right as f32 - border_width * 2.0;

        let child_ids: Vec<NodeId> = boxnode
            .children
            .iter()
            .map(|c| self.build_node(c, inner_width.max(0.0)))
            .collect();

        let flex_direction = match boxnode.direction {
            Direction::Column => FlexDirection::Column,
            Direction::Row => FlexDirection::Row,
        };

        let justify_content = convert_justify_content(boxnode.justify);
        let align_items = convert_align_items(boxnode.align);

        let (width, height, flex_grow) = match boxnode.size {
            InkSize::Fixed(h) => (length(available_width), length(h as f32), 0.0),
            InkSize::Flex(weight) => (length(available_width), auto(), weight as f32),
            InkSize::Content => (length(available_width), auto(), 0.0),
        };

        self.tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction,
                    flex_grow,
                    justify_content: Some(justify_content),
                    align_items: Some(align_items),
                    gap: Size {
                        width: length(boxnode.gap.column as f32),
                        height: length(boxnode.gap.row as f32),
                    },
                    size: Size { width, height },
                    padding: Rect {
                        top: length(padding.top as f32),
                        right: length(padding.right as f32),
                        bottom: length(padding.bottom as f32),
                        left: length(padding.left as f32),
                    },
                    margin: Rect {
                        top: length(margin.top as f32),
                        right: length(margin.right as f32),
                        bottom: length(margin.bottom as f32),
                        left: length(margin.left as f32),
                    },
                    border: Rect {
                        top: length(border_width),
                        right: length(border_width),
                        bottom: length(border_width),
                        left: length(border_width),
                    },
                    ..Default::default()
                },
                &child_ids,
            )
            .unwrap()
    }

    fn collect_layouts(
        &self,
        node_id: NodeId,
        offset_x: f32,
        offset_y: f32,
        layouts: &mut HashMap<usize, ComputedLayout>,
    ) {
        let layout = self.tree.layout(node_id).unwrap();
        let x = offset_x + layout.location.x;
        let y = offset_y + layout.location.y;

        if let Some(&id) = self
            .node_map
            .iter()
            .find(|(_, &nid)| nid == node_id)
            .map(|(id, _)| id)
        {
            layouts.insert(
                id,
                ComputedLayout {
                    x,
                    y,
                    width: layout.size.width,
                    height: layout.size.height,
                },
            );
        }

        for &child_id in self.tree.children(node_id).unwrap().iter() {
            self.collect_layouts(child_id, x, y, layouts);
        }
    }
}

fn convert_justify_content(justify: InkJustifyContent) -> JustifyContent {
    match justify {
        InkJustifyContent::Start => JustifyContent::Start,
        InkJustifyContent::End => JustifyContent::End,
        InkJustifyContent::Center => JustifyContent::Center,
        InkJustifyContent::SpaceBetween => JustifyContent::SpaceBetween,
        InkJustifyContent::SpaceAround => JustifyContent::SpaceAround,
        InkJustifyContent::SpaceEvenly => JustifyContent::SpaceEvenly,
    }
}

fn convert_align_items(align: InkAlignItems) -> AlignItems {
    match align {
        InkAlignItems::Start => AlignItems::Start,
        InkAlignItems::End => AlignItems::End,
        InkAlignItems::Center => AlignItems::Center,
        InkAlignItems::Stretch => AlignItems::Stretch,
    }
}

fn measure_text_lines(content: &str, width: usize) -> usize {
    if content.is_empty() {
        return 0;
    }
    if width == 0 {
        return content.lines().count().max(1);
    }

    let mut total = 0;
    for line in content.lines() {
        let chars = line.chars().count();
        if chars == 0 {
            total += 1;
        } else {
            total += chars.div_ceil(width);
        }
    }
    total.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::flex as ink_flex;
    use crate::node::{col, row, text};

    #[test]
    fn test_simple_column() {
        let mut engine = LayoutEngine::new();

        let tree = col([text("Header"), text("Body content here"), text("Footer")]);

        let layouts = engine.compute(&tree, 80.0, 24.0);
        assert!(!layouts.is_empty());
    }

    #[test]
    fn test_flex_grow() {
        let mut engine = LayoutEngine::new();

        let tree = col([
            text("Fixed header"),
            ink_flex(1, col([text("Expanding body")])),
            text("Fixed footer"),
        ]);

        let layouts = engine.compute(&tree, 80.0, 24.0);
        assert!(!layouts.is_empty());
    }

    #[test]
    fn test_row_layout() {
        let mut engine = LayoutEngine::new();

        let tree = row([text("Left"), text("Center"), text("Right")]);

        let layouts = engine.compute(&tree, 80.0, 24.0);
        assert!(!layouts.is_empty());
    }
}
