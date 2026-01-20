use crate::tui::ink::ansi::visual_rows;
use crate::tui::ink::node::Node;
use crate::tui::ink::render::{render_with_cursor_filtered, RenderResult};
use crate::tui::ink::runtime::{GraduatedContent, GraduationState};

#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_no: u64,
    pub graduated_keys: Vec<String>,
    pub boundary_lines: usize,
    pub viewport_visual_rows: usize,
}

#[derive(Debug, Clone)]
pub struct FramePlan {
    pub frame_no: u64,
    pub graduated: Vec<GraduatedContent>,
    pub boundary_lines: usize,
    pub viewport: RenderResult,
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

        let graduated = self.graduation.plan_graduation(tree);

        let boundary_lines = if graduated.is_empty() {
            0
        } else {
            self.boundary_default
        };

        let (stdout_delta, new_pending) =
            GraduationState::format_stdout_delta(&graduated, self.pending_newline, boundary_lines);
        self.pending_newline = new_pending;

        self.graduation.commit_graduation(&graduated);

        let viewport = render_with_cursor_filtered(tree, self.width as usize, &self.graduation);

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
                trace,
            },
            stdout_delta,
        }
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
