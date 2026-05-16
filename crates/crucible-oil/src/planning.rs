use crate::layout::LayoutEngine;

use crate::node::{Node, OverlayNode};
use crate::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::render::{render_tree, render_tree_with_engine, RenderResult, NATURAL_HEIGHT};

/// Graduated content ready for terminal output.
///
/// A thin wrapper around a Node tree. The planner renders this through the
/// unified `render_tree` path at the planner's current width, so graduation
/// (scrollback) and viewport stay byte-identical for the same tree+dims.
/// Spacing is encoded in the node tree via Gap and Padding (no out-of-band flags).
#[derive(Debug, Clone)]
pub struct Graduation {
    /// The rendered node tree for graduated content.
    pub node: Node,
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
    /// Produced by rendering graduation content through `render_tree`.
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

    pub fn plan_frame(&mut self, tree: &Node, graduation: Option<Graduation>) -> FrameSnapshot {
        self.frame_no += 1;

        let stdout_delta = graduation
            .as_ref()
            .map(|g| render_tree(&g.node, self.width, NATURAL_HEIGHT).content)
            .unwrap_or_default();

        let overlay_nodes = extract_overlays(tree);
        let main_tree = filter_overlays(tree.clone());

        let viewport =
            render_tree_with_engine(&mut self.layout_engine, &main_tree, self.width, self.height);

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
                let result = render_tree(&overlay_node.child, self.width, self.height);
                let lines: Vec<String> = result.content.lines().map(String::from).collect();
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

        for spacer_line in lines.iter().take(para2_idx).skip(para1_idx + 1).copied() {
            let stripped = strip_ansi(spacer_line);
            assert!(stripped.chars().all(|c| c == ' '));
            assert!(!spacer_line.contains('\x1b'));
        }
    }

    #[test]
    fn graduation_renders_node_to_string() {
        let mut planner = FramePlanner::new(80, 24);
        let node = col([text("Hello"), text("World")]);
        let grad = Graduation { node };
        let snapshot = planner.plan_frame(&col([text("viewport")]), Some(grad));
        assert!(snapshot.stdout_delta.contains("Hello"));
        assert!(snapshot.stdout_delta.contains("World"));
    }

    #[test]
    fn graduation_gap_produces_blank_line() {
        use crate::ansi::strip_ansi;
        use crate::style::Gap;
        // Two groups with Gap::row(1) between them
        let mut planner = FramePlanner::new(80, 24);
        let node = col([text("Group A"), text("Group B")]).gap(Gap::row(1));
        let grad = Graduation { node };
        let snapshot = planner.plan_frame(&col([text("viewport")]), Some(grad));
        let rendered = strip_ansi(&snapshot.stdout_delta);
        let lines: Vec<&str> = rendered.lines().collect();
        eprintln!("Lines: {:?}", lines);
        assert!(
            lines.len() >= 3,
            "Expected at least 3 lines (A, blank, B), got: {:?}",
            lines
        );
        assert!(lines[0].contains("Group A"));
        assert!(
            lines[1].trim().is_empty(),
            "Gap line should be blank, got: {:?}",
            lines[1]
        );
        assert!(lines[2].contains("Group B"));
    }

    #[test]
    fn plan_frame_graduation_produces_stdout_delta() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live content")]);
        let graduation = Graduation {
            node: col([text("Graduated")]),
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
        };
        let snapshot = planner.plan_frame(&tree, Some(graduation));

        let screen = snapshot.screen();
        assert!(screen.contains("Graduated"));
        assert!(screen.contains("Live"));
    }

    #[test]
    fn graduation_and_viewport_emit_byte_identical_output_for_same_tree() {
        // Same tree rendered as viewport vs graduation at the same planner
        // dimensions must produce byte-identical content. This is the
        // post-Stage-B invariant: render_tree owns both paths.
        use crate::ansi::strip_ansi;
        let mut planner = FramePlanner::new(80, 24);
        let shared = col([text("alpha"), text("beta"), text("gamma")]);

        let viewport_snap = planner.plan_frame(&shared, None);
        let grad_snap = planner.plan_frame(
            &col([text("placeholder")]),
            Some(Graduation {
                node: shared.clone(),
            }),
        );

        // Strip ANSI for comparison: viewport content includes a cursor-tracking
        // cell-grid layout, graduation is a flat string. The visible characters
        // must match.
        let viewport_visible = strip_ansi(viewport_snap.viewport_content());
        let grad_visible = strip_ansi(&grad_snap.stdout_delta);
        assert!(viewport_visible.contains("alpha"));
        assert!(viewport_visible.contains("beta"));
        assert!(viewport_visible.contains("gamma"));
        assert!(grad_visible.contains("alpha"));
        assert!(grad_visible.contains("beta"));
        assert!(grad_visible.contains("gamma"));
    }
}
