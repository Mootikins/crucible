use crate::layout::{
    build_layout_tree, build_layout_tree_with_engine, render_layout_tree,
    render_layout_tree_compact, LayoutEngine,
};
use crate::node::{Node, OverlayNode};
use crate::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::render::{trim_trailing_blank_lines, RenderResult};

use crate::ansi::visual_rows;

#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_no: u64,
    pub viewport_visual_rows: usize,
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
    pub trace: FrameTrace,
}

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub plan: FramePlan,
    /// Content to write to stdout before rendering viewport.
    /// Set by the app layer (drain-based container graduation), not by the planner.
    pub stdout_delta: String,
}

impl FrameSnapshot {
    pub fn trace(&self) -> &FrameTrace {
        &self.plan.trace
    }

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

    pub fn screen(&self) -> String {
        format!("{}{}", self.stdout_delta, self.plan.viewport.content)
    }

    pub fn screen_with_overlays(&self, width: usize) -> String {
        let viewport_with_overlays = self.viewport_with_overlays(width);
        format!("{}{}", self.stdout_delta, viewport_with_overlays)
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
        self.plan_with_stdout(tree, String::new())
    }

    pub fn plan_with_stdout(&mut self, tree: &Node, stdout_delta: String) -> FrameSnapshot {
        self.frame_no += 1;

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

        let trace = FrameTrace {
            frame_no: self.frame_no,
            viewport_visual_rows: visual_rows(&viewport.content, self.width as usize),
        };

        FrameSnapshot {
            plan: FramePlan {
                frame_no: self.frame_no,
                viewport,
                overlays: rendered_overlays,
                trace,
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
    fn plan_with_stdout_includes_delta() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Live content")]);
        let snapshot = planner.plan_with_stdout(&tree, "Graduated\r\n".to_string());

        assert_eq!(snapshot.stdout_delta, "Graduated\r\n");
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
}
