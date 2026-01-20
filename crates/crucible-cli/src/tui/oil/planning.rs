use crate::tui::oil::ansi::visual_rows;
use crate::tui::oil::node::{Node, OverlayNode};
use crate::tui::oil::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crate::tui::oil::render::{render_to_string, render_with_cursor_filtered, RenderResult};
use crate::tui::oil::runtime::{GraduatedContent, GraduationState};

#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_no: u64,
    pub graduated_keys: Vec<String>,
    pub boundary_lines: usize,
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
    pub boundary_lines: usize,
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
}

pub struct FramePlanner {
    width: u16,
    height: u16,
    frame_no: u64,
    graduation: GraduationState,
    boundary_default: usize,
    pending_newline: bool,
}

impl FramePlanner {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            frame_no: 0,
            graduation: GraduationState::new(),
            boundary_default: 1,
            pending_newline: false,
        }
    }

    pub fn plan(&mut self, tree: &Node) -> FrameSnapshot {
        self.frame_no += 1;

        let overlay_nodes = extract_overlays(tree);
        let main_tree = filter_overlays(tree.clone());

        let graduated = self.graduation.plan_graduation(&main_tree);

        let boundary_lines = if graduated.is_empty() {
            0
        } else {
            self.boundary_default
        };

        let (stdout_delta, new_pending) =
            GraduationState::format_stdout_delta(&graduated, self.pending_newline, boundary_lines);
        self.pending_newline = new_pending;

        self.graduation.commit_graduation(&graduated);

        let viewport =
            render_with_cursor_filtered(&main_tree, self.width as usize, &self.graduation);

        let rendered_overlays = self.render_overlays(&overlay_nodes);

        let trace = FrameTrace {
            frame_no: self.frame_no,
            graduated_keys: graduated.iter().map(|g| g.key.clone()).collect(),
            boundary_lines,
            viewport_visual_rows: visual_rows(&viewport.content, self.width as usize),
        };

        FrameSnapshot {
            plan: FramePlan {
                frame_no: self.frame_no,
                graduated,
                boundary_lines,
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
        self.pending_newline = false;
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
