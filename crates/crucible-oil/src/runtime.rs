use crate::node::Node;
use crate::planning::Graduation;

/// Shared interface for rendering a frame. Implemented by Terminal (real I/O)
/// and TestRuntime (in-memory buffer for tests).
pub trait FrameRenderer {
    /// Render a Node tree to the viewport, writing any graduated content first.
    fn render_frame(&mut self, tree: &Node, graduation: Option<&Graduation>);

    /// Force a full redraw on the next render (clear all cached state).
    fn force_full_redraw(&mut self);

    /// Current terminal dimensions (width, height).
    fn size(&self) -> (u16, u16);
}

pub struct TestRuntime {
    planner: crate::planning::FramePlanner,
    last_snapshot: Option<crate::planning::FrameSnapshot>,
    stdout_buffer: String,
}

impl TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            planner: crate::planning::FramePlanner::new(width, height),
            last_snapshot: None,
            stdout_buffer: String::new(),
        }
    }

    pub fn render(&mut self, tree: &Node) {
        self.render_with_stdout(tree, "");
    }

    pub fn render_with_stdout(&mut self, tree: &Node, stdout_delta: &str) {
        self.stdout_buffer.push_str(stdout_delta);
        let snapshot = self
            .planner
            .plan_with_stdout(tree, stdout_delta.to_string());
        self.last_snapshot = Some(snapshot);
    }

    pub fn render_with_graduation(&mut self, tree: &Node, graduation: Option<&Graduation>) {
        if let Some(grad) = graduation {
            self.stdout_buffer.push_str(&grad.render());
            // Mirror Terminal::apply() which writes \r\n after graduation
            // to separate scrollback from viewport
            self.stdout_buffer.push_str("\r\n");
        }
        let snapshot = self
            .planner
            .plan_with_graduation(tree, graduation.cloned());
        self.last_snapshot = Some(snapshot);
    }

    pub fn stdout_content(&self) -> &str {
        &self.stdout_buffer
    }

    pub fn viewport_content(&self) -> &str {
        self.last_snapshot
            .as_ref()
            .map(|s| s.viewport_content())
            .unwrap_or("")
    }

    pub fn trace(&self) -> Option<&crate::planning::FrameTrace> {
        self.last_snapshot.as_ref().map(|s| s.trace())
    }

    pub fn last_snapshot(&self) -> Option<&crate::planning::FrameSnapshot> {
        self.last_snapshot.as_ref()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.planner.set_size(width, height);
    }

    pub fn width(&self) -> u16 {
        self.planner.width()
    }

    pub fn height(&self) -> u16 {
        self.planner.height()
    }
}

impl FrameRenderer for TestRuntime {
    fn render_frame(&mut self, tree: &Node, graduation: Option<&Graduation>) {
        self.render_with_graduation(tree, graduation);
    }

    fn force_full_redraw(&mut self) {
        // No-op for test runtime
    }

    fn size(&self) -> (u16, u16) {
        (self.width(), self.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_new() {
        let runtime = TestRuntime::new(80, 24);

        assert!(runtime.stdout_content().is_empty());
        assert!(runtime.viewport_content().is_empty());
    }

    #[test]
    fn test_runtime_renders_viewport() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Current content")]);
        runtime.render(&tree);

        assert!(runtime.viewport_content().contains("Current content"));
    }

    #[test]
    fn test_runtime_accumulates_stdout() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        runtime.render_with_stdout(&tree, "Graduated content\r\n");

        assert!(runtime.stdout_content().contains("Graduated content"));
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_renders_graduation_node() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        let grad = Graduation {
            node: col([text("Graduated via node")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad));

        assert!(runtime.stdout_content().contains("Graduated via node"));
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_graduation_none_no_stdout() {
        use crate::node::{col, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);
        runtime.render_with_graduation(&tree, None);

        assert!(runtime.stdout_content().is_empty());
        assert!(runtime.viewport_content().contains("Live"));
    }

    #[test]
    fn test_runtime_graduation_accumulates_across_frames() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Live")]);

        // Frame 1: graduate content A
        let grad_a = Graduation {
            node: col([text("Content A")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad_a));

        // Frame 2: graduate content B
        let grad_b = Graduation {
            node: col([text("Content B")]),
            width: 80,
        };
        runtime.render_with_graduation(&tree, Some(&grad_b));

        let stdout = runtime.stdout_content();
        assert!(stdout.contains("Content A"));
        assert!(stdout.contains("Content B"));
    }

    #[test]
    fn test_frame_renderer_with_graduation() {
        use crate::node::{col, text};
        use crate::planning::Graduation;

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([text("Viewport")]);
        let grad = Graduation {
            node: col([text("Scrollback")]),
            width: 80,
        };

        // Use the FrameRenderer trait method
        FrameRenderer::render_frame(&mut runtime, &tree, Some(&grad));

        assert!(runtime.stdout_content().contains("Scrollback"));
        assert!(runtime.viewport_content().contains("Viewport"));
    }
}
