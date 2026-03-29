use crate::layout::{LayoutBox, LayoutContent, LayoutTree, PopupItem, Rect as OilRect};
use crate::node::{BoxNode, Direction, Node, Size as OilSize};
use crate::style::{
    AlignItems as OilAlignItems, JustifyContent as OilJustifyContent, Style as OilStyle,
};
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
    reverse_node_map: HashMap<NodeId, usize>,
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
            reverse_node_map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn compute(
        &mut self,
        node: &Node,
        width: f32,
        height: f32,
    ) -> HashMap<usize, ComputedLayout> {
        self.tree.clear();
        self.node_map.clear();
        self.reverse_node_map.clear();
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

    /// Create a leaf node with the given dimensions.
    fn new_leaf_size(&mut self, width: f32, height: f32) -> NodeId {
        self.tree
            .new_leaf(Style {
                size: Size {
                    width: length(width),
                    height: length(height),
                },
                ..Default::default()
            })
            .expect("taffy operation failed")
    }

    /// Create a full-width leaf node with a height of 1 line.
    fn new_full_width_line(&mut self, available_width: f32) -> NodeId {
        self.new_leaf_size(available_width, 1.0)
    }

    /// Create a content-sized leaf node (flex_shrink: 0) with the given dimensions.
    fn new_leaf_content_sized(&mut self, width: f32, height: f32) -> NodeId {
        self.tree
            .new_leaf(Style {
                flex_shrink: 0.0,
                size: Size {
                    width: length(width),
                    height: length(height),
                },
                ..Default::default()
            })
            .expect("taffy operation failed")
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
                .expect("failed to create taffy leaf node"),

            Node::Text(text) => {
                let lines = measure_text_lines(&text.content, available_width as usize);
                self.new_leaf_size(available_width, lines as f32)
            }

            Node::Box(boxnode) => self.build_box(boxnode, available_width, false),

            Node::Static(static_node) => {
                let fragment = Node::Fragment(static_node.children.clone());
                return self.build_node(&fragment, available_width);
            }

            Node::Input(_) => self.new_full_width_line(available_width),

            Node::Spinner(_) => self.new_full_width_line(available_width),

            Node::Popup(popup) => {
                // Use max_visible for height — popup renderer fills blank lines
                // above items for bottom-aligned popups.
                self.new_leaf_size(available_width, popup.max_visible as f32)
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
                    .expect("taffy operation failed")
            }

            Node::Overlay(overlay) => {
                // Render overlay child content for standalone rendering.
                // In FramePlanner, overlays are extracted before Taffy layout.
                return self.build_node(&overlay.child, available_width);
            }

            Node::Raw(raw) => {
                self.new_leaf_size(raw.display_width as f32, raw.display_height as f32)
            }
        };

        self.node_map.insert(id, node_id);
        self.reverse_node_map.insert(node_id, id);
        node_id
    }

    /// Build a node that sizes to its content width rather than filling available space.
    /// Used for children inside Row layouts so items sit side-by-side at natural width.
    fn build_node_content_sized(&mut self, node: &Node, available_width: f32) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;

        let node_id = match node {
            Node::Text(text) => {
                let content_width = crate::ansi::visible_width(&text.content) as f32;
                let lines = measure_text_lines(&text.content, content_width as usize);
                self.new_leaf_content_sized(content_width, lines as f32)
            }

            Node::Spinner(spinner) => {
                let label_width = spinner
                    .label
                    .as_ref()
                    .map(|l| l.chars().count() + 2)
                    .unwrap_or(1) as f32;
                self.new_leaf_content_sized(label_width, 1.0)
            }

            Node::Input(_) => {
                // Inputs in rows should still fill remaining space
                return self.build_node(node, available_width);
            }

            Node::Box(boxnode) => {
                let node_id = self.build_box(boxnode, available_width, true);
                self.node_map.insert(id, node_id);
                self.reverse_node_map.insert(node_id, id);
                return node_id;
            }

            _ => return self.build_node(node, available_width),
        };

        self.node_map.insert(id, node_id);
        self.reverse_node_map.insert(node_id, id);
        node_id
    }

    fn build_box(
        &mut self,
        boxnode: &BoxNode,
        available_width: f32,
        content_sized: bool,
    ) -> NodeId {
        let padding = &boxnode.padding;
        let margin = &boxnode.margin;
        let border_width = if boxnode.border.is_some() { 1.0 } else { 0.0 };

        let inner_width =
            available_width - padding.left as f32 - padding.right as f32 - border_width * 2.0;

        let is_row = matches!(boxnode.direction, Direction::Row);
        let child_ids: Vec<NodeId> = boxnode
            .children
            .iter()
            .map(|c| {
                if is_row {
                    self.build_node_content_sized(c, inner_width.max(0.0))
                } else {
                    self.build_node(c, inner_width.max(0.0))
                }
            })
            .collect();

        let flex_direction = match boxnode.direction {
            Direction::Column => FlexDirection::Column,
            Direction::Row => FlexDirection::Row,
        };

        let justify_content = convert_justify_content(boxnode.justify);
        let align_items = convert_align_items(boxnode.align);

        let (width, height, flex_grow) = match boxnode.size {
            OilSize::Fixed(h) => (length(available_width), length(h as f32), 0.0),
            OilSize::Flex(weight) => (auto(), auto(), weight as f32),
            OilSize::Content if content_sized => (auto(), auto(), 0.0),
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
            .expect("taffy operation failed")
    }

    fn collect_layouts(
        &self,
        node_id: NodeId,
        offset_x: f32,
        offset_y: f32,
        layouts: &mut HashMap<usize, ComputedLayout>,
    ) {
        let layout = self
            .tree
            .layout(node_id)
            .expect("failed to get taffy layout");
        let x = offset_x + layout.location.x;
        let y = offset_y + layout.location.y;

        if let Some(&id) = self.reverse_node_map.get(&node_id) {
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

        for &child_id in self
            .tree
            .children(node_id)
            .expect("failed to get taffy children")
            .iter()
        {
            self.collect_layouts(child_id, x, y, layouts);
        }
    }

    pub fn to_layout_tree(&self, node: &Node, root_id: NodeId) -> LayoutTree {
        let root_box = self.node_to_layout_box(node, root_id, 0.0, 0.0);
        LayoutTree::new(root_box)
    }

    pub fn compute_layout_tree(&mut self, node: &Node, width: f32, height: f32) -> LayoutTree {
        self.tree.clear();
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
        let layout = self
            .tree
            .layout(taffy_id)
            .expect("failed to get taffy layout");
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
                let taffy_children = self
                    .tree
                    .children(taffy_id)
                    .expect("failed to get taffy children");
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

            Node::Spinner(spinner) => {
                let variant = spinner.style_variant.unwrap_or_default();
                LayoutBox::new(
                    rect,
                    LayoutContent::Spinner {
                        label: spinner.label.clone(),
                        frame: spinner.frame,
                        frames: Some(variant.frames()),
                        style: spinner.style,
                    },
                )
            }

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
                    bg_style: popup.bg_style,
                    selected_style: popup.selected_style,
                },
            ),

            Node::Fragment(children) => {
                let taffy_children = self
                    .tree
                    .children(taffy_id)
                    .expect("failed to get taffy children");
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

            Node::Overlay(overlay) => {
                // Overlay child is laid out for standalone rendering
                return self.node_to_layout_box(&overlay.child, taffy_id, offset_x, offset_y);
            }

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

pub fn build_layout_tree_with_engine(
    engine: &mut LayoutEngine,
    node: &Node,
    width: u16,
    height: u16,
) -> LayoutTree {
    engine.compute_layout_tree(node, width as f32, height as f32)
}

pub fn build_layout_tree(node: &Node, width: u16, height: u16) -> LayoutTree {
    let mut engine = LayoutEngine::new();
    build_layout_tree_with_engine(&mut engine, node, width, height)
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

    // Use textwrap to match the word-wrapping used by the renderer.
    use textwrap::{wrap, Options, WordSplitter};
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);

    let mut total = 0;
    for line in content.lines() {
        if line.is_empty() {
            total += 1;
        } else {
            total += wrap(line, &options).len();
        }
    }
    total.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;
    use crate::layout::{build_layout_tree, render_layout_tree_compact, LayoutContent};
    use crate::node::{col, flex as oil_flex, row, text};

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
        assert!(matches!(layout_tree.root.content, LayoutContent::Box { .. }));
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
        use crate::node::scrollback;
        let mut engine = LayoutEngine::new();
        let node = scrollback("msg-123", [text("Message content")]);
        let layout_tree = engine.compute_layout_tree(&node, 80.0, 24.0);
        assert_eq!(layout_tree.root.key, Some("msg-123".to_string()));
    }

    #[test]
    fn to_layout_tree_input_node() {
        use crate::node::text_input;
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
        use crate::node::spinner;
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
        use crate::node::{popup, popup_item};
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

    // -----------------------------------------------------------------------
    // Word-wrap measurement parity tests
    //
    // These tests verify that measure_text_lines (used by Taffy for height
    // allocation) agrees with the renderer's wrap_and_style_padded (which
    // uses textwrap::wrap). When these diverge, text gets clipped.
    // -----------------------------------------------------------------------

    /// Helper: count how many lines the OLD char-based measurement would produce.
    fn old_char_measure(content: &str, width: usize) -> usize {
        if content.is_empty() {
            return 0;
        }
        if width == 0 {
            return content.lines().count().max(1);
        }
        let mut total = 0;
        for line in content.lines() {
            if line.is_empty() {
                total += 1;
            } else {
                total += line.chars().count().div_ceil(width);
            }
        }
        total.max(1)
    }

    /// Helper: count how many lines the renderer actually produces for a text
    /// node at a given width, going through the full Taffy -> CellGrid pipeline.
    fn rendered_line_count(content: &str, width: u16) -> usize {
        let node = text(content);
        let layout_tree = build_layout_tree(&node, width, 500);
        let (rendered, _) = render_layout_tree_compact(&layout_tree);
        let plain = strip_ansi(&rendered);
        // Count non-empty lines (compact mode trims trailing blanks)
        plain.lines().count()
    }

    #[test]
    fn word_wrap_measurement_matches_renderer_short_word_then_long() {
        // "ab cdefgh" at width 5:
        //   Old char-wrap: ceil(9/5) = 2 lines
        //   Word-wrap:     "ab"  |  "cdefg"  |  "h"  = 3 lines
        // The old measurement underestimates by 1 line, causing clipping.
        let content = "ab cdefgh";
        let width: usize = 5;

        let old_lines = old_char_measure(content, width);
        let new_lines = measure_text_lines(content, width);
        let actual_rendered = rendered_line_count(content, width as u16);

        assert_eq!(old_lines, 2, "old char-based should say 2 lines");
        assert_eq!(new_lines, 3, "new word-wrap should say 3 lines");
        assert_eq!(
            actual_rendered, 3,
            "renderer produces 3 lines, confirming old code would clip"
        );
        assert_eq!(
            new_lines, actual_rendered,
            "measurement must match renderer output"
        );
    }

    #[test]
    fn word_wrap_measurement_matches_renderer_multiple_words() {
        // "The quick brown fox" at width 8:
        //   Old char-wrap: ceil(19/8) = 3
        //   Word-wrap:     "The"  |  "quick"  |  "brown"  |  "fox"  = 4
        let content = "The quick brown fox";
        let width: usize = 8;

        let old_lines = old_char_measure(content, width);
        let new_lines = measure_text_lines(content, width);
        let actual_rendered = rendered_line_count(content, width as u16);

        assert_eq!(old_lines, 3, "old char-based should say 3 lines");
        assert_eq!(new_lines, 4, "new word-wrap should say 4 lines");
        assert_eq!(
            new_lines, actual_rendered,
            "measurement must match renderer output"
        );
    }

    #[test]
    fn word_wrap_no_clipping_in_column_layout() {
        // End-to-end: a column with two text nodes. The second node's text
        // wraps to more lines under word-wrap than char-wrap. With the old
        // measurement, Taffy would allocate too few rows and the CellGrid
        // would clip the bottom of the second node.
        let node = col([
            text("Header line"),
            // "ab cdefgh" at width 20 fits on 1 line, no divergence.
            // But in a narrow viewport, the wrapping diverges.
            text("ab cdefgh"),
        ]);

        let width: u16 = 5;
        let height: u16 = 24;

        let layout_tree = build_layout_tree(&node, width, height);
        let (rendered, _) = render_layout_tree_compact(&layout_tree);
        let plain = strip_ansi(&rendered);
        let lines: Vec<&str> = plain.lines().collect();

        // "Header line" at width 5 word-wraps to: "Heade" | "r" | "line" = 3 lines
        // (textwrap breaks "Header" mid-word since it exceeds width)
        // "ab cdefgh" at width 5 word-wraps to: "ab" | "cdefg" | "h" = 3 lines
        // Total: 6 lines. All must be present (no clipping).
        assert!(
            lines.len() >= 6,
            "Expected at least 6 rendered lines, got {}: {:?}",
            lines.len(),
            lines
        );

        // Verify the last word-wrapped fragment is present (would be clipped
        // if Taffy allocated only 2 rows for the second text node).
        let all_text = lines.join(" ");
        assert!(
            all_text.contains('h'),
            "Final fragment 'h' from 'cdefgh' must not be clipped"
        );
    }

    #[test]
    fn word_wrap_no_excessive_blank_space() {
        // Verify the fix doesn't introduce excessive blank space. When text
        // fits on fewer lines than the maximum, compact rendering should
        // trim trailing blanks.
        let node = text("short");
        let width: u16 = 80;

        let layout_tree = build_layout_tree(&node, width, 24);
        let (rendered, _) = render_layout_tree_compact(&layout_tree);
        let plain = strip_ansi(&rendered);

        assert_eq!(
            plain.lines().count(),
            1,
            "Short text at wide width should render as exactly 1 line"
        );
    }

    #[test]
    fn word_wrap_multiline_content() {
        // Embedded newlines: each line is wrapped independently.
        // "hello world\nab cdefgh" at width 5:
        //   Line 1 "hello world": "hello" | "world" = 2
        //   Line 2 "ab cdefgh":   "ab" | "cdefg" | "h" = 3
        //   Total: 5 lines
        let content = "hello world\nab cdefgh";
        let width: usize = 5;

        let new_lines = measure_text_lines(content, width);
        let actual_rendered = rendered_line_count(content, width as u16);

        assert_eq!(new_lines, 5, "should measure 5 lines total");
        assert_eq!(
            new_lines, actual_rendered,
            "measurement must match renderer output"
        );
    }

    #[test]
    fn word_wrap_long_word_exceeding_width() {
        // A single word longer than the width gets force-broken by textwrap.
        // "abcdefghij" at width 4: "abcd" | "efgh" | "ij" = 3 lines
        let content = "abcdefghij";
        let width: usize = 4;

        let old_lines = old_char_measure(content, width);
        let new_lines = measure_text_lines(content, width);
        let actual_rendered = rendered_line_count(content, width as u16);

        // Both old and new agree when there are no spaces: pure char-division.
        assert_eq!(old_lines, 3);
        assert_eq!(new_lines, 3);
        assert_eq!(new_lines, actual_rendered);
    }

    #[test]
    fn word_wrap_agrees_when_text_fits() {
        // When text fits in one line, both approaches agree.
        let content = "Hi";
        let width: usize = 80;

        assert_eq!(old_char_measure(content, width), 1);
        assert_eq!(measure_text_lines(content, width), 1);
        assert_eq!(rendered_line_count(content, width as u16), 1);
    }

    #[test]
    fn measure_text_lines_empty_and_zero_width() {
        assert_eq!(measure_text_lines("", 10), 0);
        assert_eq!(measure_text_lines("hello", 0), 1);
        assert_eq!(measure_text_lines("a\nb\nc", 0), 3);
    }
}
