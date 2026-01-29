use crate::tui::oil::ansi::visual_rows;
use crate::tui::oil::graduation::{GraduatedContent, GraduationState};
use crate::tui::oil::layout::{build_layout_tree, render_layout_tree_filtered};
use crate::tui::oil::node::{ElementKind, Node, OverlayNode};
use crate::tui::oil::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::tui::oil::render::{
    render_to_string, render_with_cursor_filtered, CursorInfo, RenderResult,
};

#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_no: u64,
    pub graduated_keys: Vec<String>,
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
    pub graduated: Vec<GraduatedContent>,
    pub viewport: RenderResult,
    pub overlays: Vec<RenderedOverlay>,
    pub trace: FrameTrace,
}

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub plan: FramePlan,
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
        use crate::tui::oil::overlay::{composite_overlays, Overlay};

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

    /// Get screen content with overlays composited (stdout + viewport + overlays)
    pub fn screen_with_overlays(&self, width: usize) -> String {
        let viewport_with_overlays = self.viewport_with_overlays(width);
        format!("{}{}", self.stdout_delta, viewport_with_overlays)
    }
}

/// Orchestrates frame rendering with graduation-first ordering.
///
/// # Execution Order (critical for no-duplication invariant)
///
/// 1. `plan_graduation()` - identify content to graduate
/// 2. `format_stdout_delta()` - build stdout output
/// 3. `commit_graduation()` - mark keys as graduated
/// 4. `render_with_filter()` - render viewport (skips graduated keys)
///
/// This order ensures content is written to stdout BEFORE being filtered from viewport.
pub struct FramePlanner {
    width: u16,
    height: u16,
    frame_no: u64,
    graduation: GraduationState,
    last_graduated_kind: Option<ElementKind>,
}

impl FramePlanner {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            frame_no: 0,
            graduation: GraduationState::new(),
            last_graduated_kind: None,
        }
    }

    pub fn plan(&mut self, tree: &Node) -> FrameSnapshot {
        #[cfg(feature = "taffy-render")]
        return self.plan_with_layout_tree(tree);

        #[cfg(not(feature = "taffy-render"))]
        return self.plan_legacy(tree);
    }

    pub fn plan_with_layout_tree(&mut self, tree: &Node) -> FrameSnapshot {
        self.frame_no += 1;

        let overlay_nodes = extract_overlays(tree);
        let main_tree = filter_overlays(tree.clone());

        let graduated = self
            .graduation
            .plan_graduation(&main_tree, self.width as usize);

        let (stdout_delta, new_last_kind) =
            GraduationState::format_stdout_delta(&graduated, self.last_graduated_kind);
        self.last_graduated_kind = new_last_kind;

        self.graduation.commit_graduation(&graduated);

        let layout_tree = build_layout_tree(&main_tree, self.width, self.height);
        let graduated_keys = self.graduation.graduated_keys();
        let content = render_layout_tree_filtered(&layout_tree, |key| {
            graduated_keys.iter().any(|k| k == key)
        });

        let viewport = RenderResult {
            content,
            cursor: CursorInfo::default(),
        };


        let rendered_overlays = self.render_overlays(&overlay_nodes);

        let trace = FrameTrace {
            frame_no: self.frame_no,
            graduated_keys: graduated.iter().map(|g| g.key.clone()).collect(),
            viewport_visual_rows: visual_rows(&viewport.content, self.width as usize),
        };

        FrameSnapshot {
            plan: FramePlan {
                frame_no: self.frame_no,
                graduated,
                viewport,
                overlays: rendered_overlays,
                trace,
            },
            stdout_delta,
        }
    }

    pub fn plan_legacy(&mut self, tree: &Node) -> FrameSnapshot {
        self.frame_no += 1;

        let overlay_nodes = extract_overlays(tree);
        let main_tree = filter_overlays(tree.clone());

        let graduated = self
            .graduation
            .plan_graduation(&main_tree, self.width as usize);

        let (stdout_delta, new_last_kind) =
            GraduationState::format_stdout_delta(&graduated, self.last_graduated_kind);
        self.last_graduated_kind = new_last_kind;

        self.graduation.commit_graduation(&graduated);

        let viewport =
            render_with_cursor_filtered(&main_tree, self.width as usize, &self.graduation);


        let rendered_overlays = self.render_overlays(&overlay_nodes);

        let trace = FrameTrace {
            frame_no: self.frame_no,
            graduated_keys: graduated.iter().map(|g| g.key.clone()).collect(),
            viewport_visual_rows: visual_rows(&viewport.content, self.width as usize),
        };

        FrameSnapshot {
            plan: FramePlan {
                frame_no: self.frame_no,
                graduated,
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
                let content = render_to_string(&overlay_node.child, self.width as usize);
                let lines: Vec<String> = content.lines().map(String::from).collect();
                RenderedOverlay {
                    lines,
                    anchor: overlay_node.anchor,
                }
            })
            .collect()
    }

    pub fn graduation(&self) -> &GraduationState {
        &self.graduation
    }

    pub fn reset_graduation(&mut self) {
        self.graduation.clear();
        self.last_graduated_kind = None;
    }

    pub fn pre_graduate_keys(&mut self, keys: impl IntoIterator<Item = String>) {
        self.graduation.pre_graduate_keys(keys);
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
    use crate::tui::oil::node::{col, scrollback, text};

    #[test]
    fn plan_with_layout_tree_renders_text() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Hello, World!")]);
        let snapshot = planner.plan_with_layout_tree(&tree);

        assert!(snapshot.viewport_content().contains("Hello, World!"));
    }

    #[test]
    fn plan_with_layout_tree_filters_graduated_keys() {
        let mut planner = FramePlanner::new(80, 24);

        let tree = col([
            scrollback("msg-1", [text("First message")]),
            scrollback("msg-2", [text("Second message")]),
        ]);

        let snapshot1 = planner.plan_with_layout_tree(&tree);
        assert_eq!(snapshot1.plan.graduated.len(), 2);
        assert!(snapshot1.plan.graduated[0]
            .content
            .contains("First message"));
        assert!(snapshot1.plan.graduated[1]
            .content
            .contains("Second message"));
        assert!(
            snapshot1.viewport_content().is_empty(),
            "Viewport should be empty after graduation"
        );

        let snapshot2 = planner.plan_with_layout_tree(&tree);
        assert!(
            snapshot2.plan.graduated.is_empty(),
            "No new graduation on second render"
        );
        assert!(
            snapshot2.viewport_content().is_empty(),
            "Viewport still empty - content already graduated"
        );
    }

    #[test]
    fn plan_legacy_still_works() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Hello, World!")]);
        let snapshot = planner.plan_legacy(&tree);

        assert!(snapshot.viewport_content().contains("Hello, World!"));
    }

    #[test]
    fn plan_defaults_to_legacy() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Test content")]);

        let legacy_snapshot = planner.plan_legacy(&tree);
        planner.reset_graduation();
        let default_snapshot = planner.plan(&tree);

        assert_eq!(
            legacy_snapshot.viewport_content(),
            default_snapshot.viewport_content()
        );
    }
}
