use crate::layout::{
    build_layout_tree, build_layout_tree_with_engine, render_layout_tree,
    render_layout_tree_compact, LayoutEngine,
};

use crate::node::{Node, OverlayNode};
use crate::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::render::{trim_trailing_blank_lines, RenderResult};


/// Graduated content ready for terminal output.
///
/// A thin wrapper around a Node tree and the width it should be rendered at.
/// The terminal layer renders this to a string and writes it to scrollback.
/// Spacing is encoded in the node tree via Gap and Padding (no out-of-band flags).
#[derive(Debug, Clone)]
pub struct Graduation {
    /// The rendered node tree for graduated content.
    pub node: Node,
    /// Terminal width for rendering.
    pub width: u16,
}

impl Graduation {
    /// Render graduated content to an ANSI string (compact, trailing blanks trimmed).
    ///
    /// This is the single render function for graduation content. Both the
    /// production TUI (via `plan_frame`) and tests use this.
    pub fn render(&self) -> String {
        crate::render::render_to_string(&self.node, self.width as usize)
    }
}

#[derive(Debug, Clone)]
pub struct RenderedOverlay {
    pub lines: Vec<String>,
    pub anchor: OverlayAnchor,
}

#[derive(Debug, Clone)]
pub struct FramePlan {
    pub frame_no: u64,
    pub viewport: RenderResult,
    pub overlays: Vec<RenderedOverlay>,
}

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub plan: FramePlan,
    /// Content to write to stdout before rendering viewport.
    /// Produced by rendering graduation content (Graduation::render()).
    pub stdout_delta: String,
}

impl FrameSnapshot {
    pub fn viewport_content(&self) -> &str {
        &self.plan.viewport.content
    }

    pub fn viewport_with_overlays(&self, width: usize) -> String {
        use crate::overlay::{composite_overlays, Overlay};

        if self.plan.overlays.is_empty() {
            return self.plan.viewport.content.clone();
        }

        let base_lines: Vec<String> = self
            .plan
            .viewport
            .content
            .lines()
            .map(String::from)
            .collect();
        let overlay_refs: Vec<Overlay> = self
            .plan
            .overlays
            .iter()
            .map(|o| Overlay {
                lines: o.lines.clone(),
                anchor: o.anchor,
            })
            .collect();
        let composited = composite_overlays(&base_lines, &overlay_refs, width);
        composited.join("\r\n")
    }

    /// Stdout content (graduated scrollback text).
    fn stdout_content(&self) -> &str {
        &self.stdout_delta
    }

    pub fn screen(&self) -> String {
        format!("{}{}", self.stdout_content(), self.plan.viewport.content)
    }

    pub fn screen_with_overlays(&self, width: usize) -> String {
        let viewport_with_overlays = self.viewport_with_overlays(width);
        format!("{}{}", self.stdout_content(), viewport_with_overlays)
    }
}

/// Orchestrates frame rendering.
///
/// Renders the node tree through a single Taffy pass. Graduation (moving
/// completed content to stdout) is handled at the app layer — the planner
/// just renders what it's given.
pub struct FramePlanner {
    width: u16,
    height: u16,
    frame_no: u64,
    layout_engine: LayoutEngine,
}

impl FramePlanner {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            frame_no: 0,
            layout_engine: LayoutEngine::new(),
        }
    }

    pub fn plan(&mut self, tree: &Node) -> FrameSnapshot {
        self.plan_frame(tree, None)
    }

    pub fn plan_frame(
        &mut self,
        tree: &Node,
        graduation: Option<Graduation>,
    ) -> FrameSnapshot {
        self.frame_no += 1;

        let stdout_delta = graduation
            .as_ref()
            .map(|g| g.render())
            .unwrap_or_default();

        let overlay_nodes = extract_overlays(tree);
        let main_tree = filter_overlays(tree.clone());

        let layout_tree = build_layout_tree_with_engine(
            &mut self.layout_engine,
            &main_tree,
            self.width,
            self.height,
        );
        let (content, cursor_info) = render_layout_tree(&layout_tree);

        let viewport = RenderResult {
            content,
            cursor: cursor_info,
        };

        let rendered_overlays = self.render_overlays(&overlay_nodes);

        FrameSnapshot {
            plan: FramePlan {
                frame_no: self.frame_no,
                viewport,
                overlays: rendered_overlays,
            },
            stdout_delta,
        }
    }

    fn render_overlays(&self, overlay_nodes: &[OverlayNode]) -> Vec<RenderedOverlay> {
        overlay_nodes
            .iter()
            .map(|overlay_node| {
                let layout_tree = build_layout_tree(&overlay_node.child, self.width, self.height);
                let (content, _) = render_layout_tree_compact(&layout_tree);
                let trimmed = trim_trailing_blank_lines(&content);
                let lines: Vec<String> = trimmed.lines().map(String::from).collect();
                RenderedOverlay {
                    lines,
                    anchor: overlay_node.anchor,
                }
            })
            .collect()
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{col, text, text_input};

    #[test]
    fn plan_renders_text() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Hello, World!")]);
        let snapshot = planner.plan(&tree);

        assert!(snapshot.viewport_content().contains("Hello, World!"));
    }

    #[test]
    fn plan_tracks_cursor_for_focused_input() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text_input("hello", 3)]);

        let snapshot = planner.plan(&tree);

        assert!(snapshot.plan.viewport.cursor.visible);
        assert_eq!(snapshot.plan.viewport.cursor.col, 3);
    }

    #[test]
    fn plan_is_stable_across_frames() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Hello"), text_input("hello", 2)]);

        let first = planner.plan(&tree);
        let second = planner.plan(&tree);

        assert_eq!(first.plan.viewport.content, second.plan.viewport.content);
        assert_eq!(
            first.plan.viewport.cursor.visible,
            second.plan.viewport.cursor.visible
        );
        assert_eq!(
            first.plan.viewport.cursor.col,
            second.plan.viewport.cursor.col
        );
    }

    #[test]
    fn plan_frame_with_graduation_includes_delta() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live content")]);
        let grad = Graduation {
            node: col([text("Graduated")]),
            width: 80,
        };
        let snapshot = planner.plan_frame(&tree, Some(grad));

        assert!(snapshot.stdout_delta.contains("Graduated"));
        assert!(snapshot.viewport_content().contains("Live content"));
    }

    #[test]
    fn text_space_spacer_produces_clean_blank_line_in_viewport() {
        use crate::ansi::strip_ansi;

        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Para 1"), text(" "), text("Para 2")]);

        let snapshot = planner.plan(&tree);
        let content = snapshot.viewport_content();

        assert!(content.contains("Para 1"));
        assert!(content.contains("Para 2"));

        let lines: Vec<&str> = content.lines().collect();
        let para1_idx = lines.iter().position(|l| l.contains("Para 1")).unwrap();
        let para2_idx = lines.iter().position(|l| l.contains("Para 2")).unwrap();

        assert!(para2_idx > para1_idx + 1);

        for spacer_idx in (para1_idx + 1)..para2_idx {
            let spacer_line = lines[spacer_idx];
            let stripped = strip_ansi(spacer_line);
            assert!(stripped.chars().all(|c| c == ' '));
            assert!(!spacer_line.contains('\x1b'));
        }
    }

    #[test]
    fn graduation_renders_node_to_string() {
        let node = col([text("Hello"), text("World")]);
        let grad = Graduation { node, width: 80 };
        let rendered = grad.render();
        assert!(rendered.contains("Hello"));
        assert!(rendered.contains("World"));
    }

    #[test]
    fn graduation_gap_produces_blank_line() {
        use crate::ansi::strip_ansi;
        use crate::style::Gap;
        // Two groups with Gap::row(1) between them
        let node = col([text("Group A"), text("Group B")]).gap(Gap::row(1));
        let grad = Graduation { node, width: 80 };
        let rendered = strip_ansi(&grad.render());
        let lines: Vec<&str> = rendered.lines().collect();
        eprintln!("Lines: {:?}", lines);
        assert!(lines.len() >= 3, "Expected at least 3 lines (A, blank, B), got: {:?}", lines);
        assert!(lines[0].contains("Group A"));
        assert!(lines[1].trim().is_empty(), "Gap line should be blank, got: {:?}", lines[1]);
        assert!(lines[2].contains("Group B"));
    }

    #[test]
    fn plan_frame_graduation_produces_stdout_delta() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live content")]);
        let graduation = Graduation {
            node: col([text("Graduated")]),
            width: 80,
        };
        let snapshot = planner.plan_frame(&tree, Some(graduation));

        assert!(snapshot.stdout_delta.contains("Graduated"));
        assert!(snapshot.viewport_content().contains("Live content"));
    }

    #[test]
    fn plan_frame_no_graduation_empty_delta() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live content")]);
        let snapshot = planner.plan_frame(&tree, None);

        assert!(snapshot.stdout_delta.is_empty());
        assert!(snapshot.viewport_content().contains("Live content"));
    }

    #[test]
    fn graduation_screen_combines_graduation_and_viewport() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live")]);
        let graduation = Graduation {
            node: col([text("Graduated")]),
            width: 80,
        };
        let snapshot = planner.plan_frame(&tree, Some(graduation));

        let screen = snapshot.screen();
        assert!(screen.contains("Graduated"));
        assert!(screen.contains("Live"));
    }
}
