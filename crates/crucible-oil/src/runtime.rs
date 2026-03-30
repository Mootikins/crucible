use crate::node::Node;

/// Shared interface for rendering a frame. Implemented by Terminal (real I/O)
/// and TestRuntime (in-memory buffer for tests).
pub trait FrameRenderer {
    /// Render a Node tree to the viewport, writing any stdout_delta first.
    fn render_frame(&mut self, tree: &Node, stdout_delta: &str);

    /// Force a full redraw on the next render (clear all cached state).
    fn force_full_redraw(&mut self);

    /// Current terminal dimensions (width, height).
    fn size(&self) -> (u16, u16);

    /// Set scroll offset (lines from bottom). 0 = pinned to bottom.
    fn set_scroll_offset(&mut self, _offset: usize) {}
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
    fn render_frame(&mut self, tree: &Node, stdout_delta: &str) {
        self.render_with_stdout(tree, stdout_delta);
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
}
