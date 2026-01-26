use crate::tui::oil::node::Node;

// Re-export graduation types for backward compatibility
pub use crate::tui::oil::graduation::{GraduatedContent, GraduationState};

pub struct TestRuntime {
    planner: crate::tui::oil::planning::FramePlanner,
    last_snapshot: Option<crate::tui::oil::planning::FrameSnapshot>,
    stdout_buffer: String,
}

impl TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            planner: crate::tui::oil::planning::FramePlanner::new(width, height),
            last_snapshot: None,
            stdout_buffer: String::new(),
        }
    }

    pub fn render(&mut self, tree: &Node) {
        let snapshot = self.planner.plan(tree);
        self.stdout_buffer.push_str(&snapshot.stdout_delta);
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

    pub fn graduated_count(&self) -> usize {
        self.planner.graduation().graduated_count()
    }

    pub fn trace(&self) -> Option<&crate::tui::oil::planning::FrameTrace> {
        self.last_snapshot.as_ref().map(|s| s.trace())
    }

    pub fn last_snapshot(&self) -> Option<&crate::tui::oil::planning::FrameSnapshot> {
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

    pub fn last_graduated_keys(&self) -> Vec<String> {
        self.last_snapshot
            .as_ref()
            .map(|s| s.trace().graduated_keys.clone())
            .unwrap_or_default()
    }

    pub fn pre_graduate_keys(&mut self, keys: impl IntoIterator<Item = String>) {
        self.planner.pre_graduate_keys(keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_new() {
        let runtime = TestRuntime::new(80, 24);

        assert_eq!(runtime.graduated_count(), 0);
        assert!(runtime.stdout_content().is_empty());
        assert!(runtime.viewport_content().is_empty());
    }

    #[test]
    fn test_runtime_filters_graduated_from_viewport() {
        use crate::tui::oil::node::{col, scrollback, text};

        let mut runtime = TestRuntime::new(80, 24);

        let tree = col([
            scrollback("old", [text("Old message")]),
            text("Current content"),
        ]);

        runtime.render(&tree);

        assert!(runtime.stdout_content().contains("Old message"));
        assert!(!runtime.viewport_content().contains("Old message"));
        assert!(runtime.viewport_content().contains("Current content"));
    }

    #[test]
    fn trace_captures_graduated_keys_per_frame() {
        use crate::tui::oil::node::{col, scrollback, text};

        let mut runtime = TestRuntime::new(80, 24);

        // Frame 1: two static nodes graduate
        let tree1 = col([
            scrollback("msg-1", [text("First")]),
            scrollback("msg-2", [text("Second")]),
            text("Viewport"),
        ]);
        runtime.render(&tree1);

        let trace1 = runtime.trace().expect("should have trace after render");
        assert_eq!(trace1.frame_no, 1);
        assert_eq!(trace1.graduated_keys, vec!["msg-1", "msg-2"]);
        assert!(trace1.boundary_lines > 0);

        // Frame 2: one new static node, previous two already graduated
        let tree2 = col([
            scrollback("msg-1", [text("First")]),
            scrollback("msg-2", [text("Second")]),
            scrollback("msg-3", [text("Third")]),
            text("Viewport"),
        ]);
        runtime.render(&tree2);

        let trace2 = runtime.trace().expect("should have trace");
        assert_eq!(trace2.frame_no, 2);
        assert_eq!(trace2.graduated_keys, vec!["msg-3"]);

        // Frame 3: no new static nodes
        let tree3 = col([
            scrollback("msg-1", [text("First")]),
            scrollback("msg-2", [text("Second")]),
            scrollback("msg-3", [text("Third")]),
            text("New viewport"),
        ]);
        runtime.render(&tree3);

        let trace3 = runtime.trace().expect("should have trace");
        assert_eq!(trace3.frame_no, 3);
        assert!(trace3.graduated_keys.is_empty());
        assert_eq!(trace3.boundary_lines, 0);
    }
}
