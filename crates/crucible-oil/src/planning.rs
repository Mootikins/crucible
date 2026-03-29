use crate::ansi::visual_rows;
use crate::graduation::{GraduatedContent, GraduationState};
use crate::layout::{
    build_layout_tree, build_layout_tree_with_engine, render_layout_tree,
    render_layout_tree_compact, LayoutEngine,
};
use crate::node::{ElementKind, Node, OverlayNode};
use crate::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::render::RenderResult;

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
    layout_engine: LayoutEngine,
}

impl FramePlanner {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            frame_no: 0,
            graduation: GraduationState::new(),
            last_graduated_kind: None,
            layout_engine: LayoutEngine::new(),
        }
    }

    pub fn plan(&mut self, tree: &Node) -> FrameSnapshot {
        self.plan_with_layout_tree(tree)
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

        // Filter graduated Static nodes from the tree BEFORE layout so Taffy
        // doesn't allocate space for them. Without this, graduated nodes take
        // up rows in the layout that become blank gaps after the renderer skips them.
        let viewport_tree = filter_graduated_static_nodes(&main_tree, &self.graduation);

        let layout_tree = build_layout_tree_with_engine(
            &mut self.layout_engine,
            &viewport_tree,
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
                // Use Taffy compact mode: strips trailing padding per line so overlays
                // get tight output suitable for compositing at anchored positions.
                let layout_tree =
                    build_layout_tree(&overlay_node.child, self.width, self.height);
                let (content, _) = render_layout_tree_compact(&layout_tree);
                // Trim trailing blank lines from the full-height CellGrid
                let lines: Vec<String> = content
                    .lines()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .skip_while(|l| l.is_empty())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(String::from)
                    .collect();
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

/// Remove graduated Static nodes from the tree so Taffy doesn't allocate
/// layout space for them. Replaces graduated nodes with Empty.
fn filter_graduated_static_nodes(tree: &Node, graduation: &GraduationState) -> Node {
    match tree {
        Node::Static(s) if graduation.is_graduated(&s.key) => Node::Empty,
        Node::Box(b) => Node::Box(crate::node::BoxNode {
            children: b
                .children
                .iter()
                .map(|c| filter_graduated_static_nodes(c, graduation))
                .collect(),
            ..b.clone()
        }),
        Node::Fragment(cs) => Node::Fragment(
            cs.iter()
                .map(|c| filter_graduated_static_nodes(c, graduation))
                .collect(),
        ),
        Node::Static(s) => Node::Static(crate::node::StaticNode {
            children: s
                .children
                .iter()
                .map(|c| filter_graduated_static_nodes(c, graduation))
                .collect(),
            ..s.clone()
        }),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{col, scrollback, text, text_input};

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
    fn plan_with_layout_tree_tracks_cursor_for_focused_input() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text_input("hello", 3)]);

        let snapshot = planner.plan_with_layout_tree(&tree);

        assert!(snapshot.plan.viewport.cursor.visible);
        assert_eq!(snapshot.plan.viewport.cursor.col, 3);
    }

    #[test]
    fn plan_with_layout_tree_is_stable_across_frames_for_non_graduated_content() {
        let mut planner = FramePlanner::new(80, 24);
        let tree = col([text("Hello"), text_input("hello", 2)]);

        let first = planner.plan_with_layout_tree(&tree);
        let second = planner.plan_with_layout_tree(&tree);

        assert_eq!(first.plan.viewport.content, second.plan.viewport.content);
        assert_eq!(
            first.plan.viewport.cursor.visible,
            second.plan.viewport.cursor.visible
        );
        assert_eq!(
            first.plan.viewport.cursor.col,
            second.plan.viewport.cursor.col
        );
        assert_eq!(
            first.plan.viewport.cursor.row_from_end,
            second.plan.viewport.cursor.row_from_end
        );
    }

    /// Verify that `text(" ")` spacers produce visually clean blank lines
    /// in the production viewport rendering path (FramePlanner → Taffy → CellGrid → non-compact).
    ///
    /// The concern: `to_string_joined()` does NOT strip trailing spaces, so a `text(" ")`
    /// spacer might render as a full-width line of spaces rather than a clean blank line.
    /// This test checks whether the spacer line is identical to lines that were never
    /// written to (pure CellGrid padding).
    #[test]
    fn text_space_spacer_produces_clean_blank_line_in_viewport() {
        use crate::ansi::strip_ansi;

        let mut planner = FramePlanner::new(80, 24);

        // Build a tree with text(" ") spacers between paragraphs
        let tree = col([text("Para 1"), text(" "), text("Para 2")]);

        let snapshot = planner.plan(&tree);
        let content = snapshot.viewport_content();

        // Sanity: both paragraphs render
        assert!(content.contains("Para 1"), "Para 1 should be in viewport");
        assert!(content.contains("Para 2"), "Para 2 should be in viewport");

        // Find the lines
        let lines: Vec<&str> = content.lines().collect();

        // Find Para 1 and Para 2 line indices
        let para1_idx = lines
            .iter()
            .position(|l| l.contains("Para 1"))
            .expect("Para 1 line not found");
        let para2_idx = lines
            .iter()
            .position(|l| l.contains("Para 2"))
            .expect("Para 2 line not found");

        // There should be at least one line between them (the spacer)
        assert!(
            para2_idx > para1_idx + 1,
            "Expected spacer line between Para 1 (line {}) and Para 2 (line {})",
            para1_idx,
            para2_idx
        );

        // Check the spacer line(s) between the two paragraphs
        for spacer_idx in (para1_idx + 1)..para2_idx {
            let spacer_line = lines[spacer_idx];
            let stripped = strip_ansi(spacer_line);

            // The spacer line should be visually empty — only spaces allowed,
            // no styled/colored content
            assert!(
                stripped.chars().all(|c| c == ' '),
                "Spacer line {} should contain only spaces after stripping ANSI, got: {:?}",
                spacer_idx,
                stripped
            );

            // Check for ANSI styling on the spacer line. A clean spacer should have
            // NO ANSI escape sequences — it should be pure padding spaces from CellGrid.
            // If there ARE escape sequences, the space character was styled, meaning
            // text(" ") is being treated as styled content rather than a blank separator.
            let has_ansi = spacer_line.contains('\x1b');
            assert!(
                !has_ansi,
                "Spacer line {} has ANSI styling — text(\" \") is producing styled content \
                 in the viewport, not a clean blank line. Line content: {:?}",
                spacer_idx,
                spacer_line
            );

            // The spacer line should be exactly 80 spaces (grid width) with no
            // ANSI codes — identical to what CellGrid produces for untouched rows.
            let expected_padding = " ".repeat(80);
            assert_eq!(
                spacer_line, expected_padding.as_str(),
                "Spacer line should be identical to an untouched CellGrid padding line (80 spaces).\n\
                 Spacer:  {:?} (len={})\n\
                 Expected: {:?} (len={})",
                spacer_line, spacer_line.len(),
                expected_padding, expected_padding.len()
            );
        }
    }
}
