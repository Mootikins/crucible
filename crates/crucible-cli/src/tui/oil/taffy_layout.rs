use crate::tui::oil::layout::{LayoutBox, LayoutContent, LayoutTree, PopupItem};
use crate::tui::oil::node::{BoxNode, Direction, Node, Size as OilSize};
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::style::{AlignItems as OilAlignItems, JustifyContent as OilJustifyContent};
use crucible_oil::layout::Rect as OilRect;
use crucible_oil::style::Style as OilStyle;
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
            OilSize::Fixed(h) => (length(available_width), length(h as f32), 0.0),
            OilSize::Flex(weight) => (length(available_width), auto(), weight as f32),
            OilSize::Content => (length(available_width), auto(), 0.0),
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

    pub fn to_layout_tree(&self, node: &Node, root_id: NodeId) -> LayoutTree {
        let root_box = self.node_to_layout_box(node, root_id, 0.0, 0.0);
        LayoutTree::new(root_box)
    }

    pub fn compute_layout_tree(&mut self, node: &Node, width: f32, height: f32) -> LayoutTree {
        self.tree = TaffyTree::new();
        self.node_map.clear();
        self.next_id = 0;

        let root_id = self.build_node(node, width);

        let available = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };

        self.tree.compute_layout(root_id, available).ok();

        self.to_layout_tree(node, root_id)
    }

    fn node_to_layout_box(
        &self,
        node: &Node,
        taffy_id: NodeId,
        offset_x: f32,
        offset_y: f32,
    ) -> LayoutBox {
        let layout = self.tree.layout(taffy_id).unwrap();
        let x = offset_x + layout.location.x;
        let y = offset_y + layout.location.y;

        let rect = OilRect::new(
            x as u16,
            y as u16,
            layout.size.width as u16,
            layout.size.height as u16,
        );

        match node {
            Node::Empty => LayoutBox::new(rect, LayoutContent::Empty),

            Node::Text(text) => LayoutBox::new(
                rect,
                LayoutContent::Text {
                    content: text.content.clone(),
                    style: text.style,
                },
            ),

            Node::Box(boxnode) => {
                let taffy_children = self.tree.children(taffy_id).unwrap();
                let children: Vec<LayoutBox> = boxnode
                    .children
                    .iter()
                    .zip(taffy_children.iter())
                    .map(|(child_node, &child_taffy_id)| {
                        self.node_to_layout_box(child_node, child_taffy_id, x, y)
                    })
                    .collect();

                LayoutBox {
                    rect,
                    content: LayoutContent::Box {
                        border: boxnode.border,
                        style: boxnode.style,
                    },
                    children,
                    style: boxnode.style,
                    key: None,
                }
            }

            Node::Static(static_node) => {
                let fragment = Node::Fragment(static_node.children.clone());
                let mut layout_box =
                    self.node_to_layout_box(&fragment, taffy_id, offset_x, offset_y);
                layout_box.key = Some(static_node.key.clone());
                layout_box
            }

            Node::Input(input) => LayoutBox::new(
                rect,
                LayoutContent::Input {
                    value: input.value.clone(),
                    cursor: input.cursor,
                    placeholder: input.placeholder.clone(),
                    focused: input.focused,
                    style: input.style,
                },
            ),

            Node::Spinner(spinner) => LayoutBox::new(
                rect,
                LayoutContent::Spinner {
                    label: spinner.label.clone(),
                    frame: spinner.frame,
                    frames: spinner.frames,
                    style: spinner.style,
                },
            ),

            Node::Popup(popup) => LayoutBox::new(
                rect,
                LayoutContent::Popup {
                    items: popup
                        .items
                        .iter()
                        .map(|item| PopupItem {
                            label: item.label.clone(),
                            description: item.description.clone(),
                            kind: item.kind.clone(),
                        })
                        .collect(),
                    selected: popup.selected,
                    viewport_offset: popup.viewport_offset,
                    max_visible: popup.max_visible,
                },
            ),

            Node::Fragment(children) => {
                let taffy_children = self.tree.children(taffy_id).unwrap();
                let child_boxes: Vec<LayoutBox> = children
                    .iter()
                    .zip(taffy_children.iter())
                    .map(|(child_node, &child_taffy_id)| {
                        self.node_to_layout_box(child_node, child_taffy_id, x, y)
                    })
                    .collect();

                LayoutBox {
                    rect,
                    content: LayoutContent::Fragment,
                    children: child_boxes,
                    style: OilStyle::default(),
                    key: None,
                }
            }

            Node::Focusable(focusable) => {
                self.node_to_layout_box(&focusable.child, taffy_id, offset_x, offset_y)
            }

            Node::ErrorBoundary(boundary) => {
                self.node_to_layout_box(&boundary.child, taffy_id, offset_x, offset_y)
            }

            Node::Overlay(_) => LayoutBox::new(rect, LayoutContent::Empty),

            Node::Raw(raw) => LayoutBox::new(
                rect,
                LayoutContent::Raw {
                    content: raw.content.clone(),
                    display_width: raw.display_width,
                    display_height: raw.display_height,
                },
            ),
        }
    }
}

fn convert_justify_content(justify: OilJustifyContent) -> JustifyContent {
    match justify {
        OilJustifyContent::Start => JustifyContent::Start,
        OilJustifyContent::End => JustifyContent::End,
        OilJustifyContent::Center => JustifyContent::Center,
        OilJustifyContent::SpaceBetween => JustifyContent::SpaceBetween,
        OilJustifyContent::SpaceAround => JustifyContent::SpaceAround,
        OilJustifyContent::SpaceEvenly => JustifyContent::SpaceEvenly,
    }
}

fn convert_align_items(align: OilAlignItems) -> AlignItems {
    match align {
        OilAlignItems::Start => AlignItems::Start,
        OilAlignItems::End => AlignItems::End,
        OilAlignItems::Center => AlignItems::Center,
        OilAlignItems::Stretch => AlignItems::Stretch,
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
    use crate::tui::oil::node::flex as oil_flex;
    use crate::tui::oil::node::{col, row, text};

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
            oil_flex(1, col([text("Expanding body")])),
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

    #[test]
    fn to_layout_tree_simple_text() {
        let mut engine = LayoutEngine::new();
        let node = text("Hello");

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        assert!(matches!(
            layout_tree.root.content,
            LayoutContent::Text { ref content, .. } if content == "Hello"
        ));
        assert_eq!(layout_tree.root.rect.width, 80);
    }

    #[test]
    fn to_layout_tree_column_with_children() {
        let mut engine = LayoutEngine::new();
        let node = col([text("Header"), text("Body"), text("Footer")]);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        assert!(matches!(
            layout_tree.root.content,
            LayoutContent::Box { .. }
        ));
        assert_eq!(layout_tree.root.children.len(), 3);

        for (i, expected) in ["Header", "Body", "Footer"].iter().enumerate() {
            match &layout_tree.root.children[i].content {
                LayoutContent::Text { content, .. } => assert_eq!(content, *expected),
                _ => panic!("Expected Text content"),
            }
        }
    }

    #[test]
    fn to_layout_tree_preserves_positions() {
        let mut engine = LayoutEngine::new();
        let node = col([text("Line 1"), text("Line 2")]);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        let child1 = &layout_tree.root.children[0];
        let child2 = &layout_tree.root.children[1];

        assert!(
            child2.rect.y > child1.rect.y,
            "Second child should be below first"
        );
    }

    #[test]
    fn to_layout_tree_static_preserves_key() {
        use crate::tui::oil::node::scrollback;

        let mut engine = LayoutEngine::new();
        let node = scrollback("msg-123", [text("Message content")]);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        assert_eq!(layout_tree.root.key, Some("msg-123".to_string()));
    }

    #[test]
    fn to_layout_tree_input_node() {
        use crate::tui::oil::node::text_input;

        let mut engine = LayoutEngine::new();
        let node = text_input("hello", 5);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        match &layout_tree.root.content {
            LayoutContent::Input {
                value,
                cursor,
                focused,
                ..
            } => {
                assert_eq!(value, "hello");
                assert_eq!(*cursor, 5);
                assert!(*focused);
            }
            _ => panic!("Expected Input content"),
        }
    }

    #[test]
    fn to_layout_tree_spinner_node() {
        use crate::tui::oil::node::spinner;

        let mut engine = LayoutEngine::new();
        let node = spinner(Some("Loading...".to_string()), 2);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        match &layout_tree.root.content {
            LayoutContent::Spinner { label, frame, .. } => {
                assert_eq!(label.as_deref(), Some("Loading..."));
                assert_eq!(*frame, 2);
            }
            _ => panic!("Expected Spinner content"),
        }
    }

    #[test]
    fn to_layout_tree_popup_node() {
        use crate::tui::oil::node::{popup, popup_item};

        let mut engine = LayoutEngine::new();
        let items = vec![
            popup_item("Item 1"),
            popup_item("Item 2").desc("Description"),
        ];
        let node = popup(items, 1, 5);

        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);

        match &layout_tree.root.content {
            LayoutContent::Popup {
                items,
                selected,
                max_visible,
                ..
            } => {
                assert_eq!(items.len(), 2);
                assert_eq!(*selected, 1);
                assert_eq!(*max_visible, 5);
                assert_eq!(items[0].label, "Item 1");
                assert_eq!(items[1].description.as_deref(), Some("Description"));
            }
            _ => panic!("Expected Popup content"),
        }
    }
}
