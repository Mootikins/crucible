use crate::tui::oil::node::{Node, StaticNode};
use crate::tui::oil::render::{render_children_to_string, RenderFilter};
use std::collections::VecDeque;
use std::io::{self, Write};

/// Width used for rendering graduated content. Large value lets terminal handle wrapping.
/// This matches `NATURAL_TEXT_WIDTH` in markdown.rs - graduated content uses "natural" style.
pub const GRADUATION_WIDTH: usize = 10000;

/// Maximum number of graduated keys to track. Once full, oldest keys are evicted.
/// This bounds memory usage and provides natural cleanup for long sessions.
/// 256 messages is ~10 screens of typical chat content.
const MAX_GRADUATED_KEYS: usize = 256;

/// Ring buffer for tracking graduated message keys.
/// Uses VecDeque for O(1) push/pop at both ends with good cache locality.
pub struct GraduationState {
    graduated_keys: VecDeque<String>,
}

impl Default for GraduationState {
    fn default() -> Self {
        Self::new()
    }
}

impl GraduationState {
    pub fn new() -> Self {
        Self {
            graduated_keys: VecDeque::with_capacity(MAX_GRADUATED_KEYS),
        }
    }

    pub fn is_graduated(&self, key: &str) -> bool {
        self.graduated_keys.iter().any(|k| k == key)
    }

    pub fn graduated_count(&self) -> usize {
        self.graduated_keys.len()
    }

    pub fn clear(&mut self) {
        self.graduated_keys.clear();
    }

    pub fn plan_graduation(&self, node: &Node) -> Vec<GraduatedContent> {
        let mut graduated = Vec::new();
        self.collect_static_nodes_readonly(node, &mut graduated);
        graduated
    }

    pub fn commit_graduation(&mut self, graduated: &[GraduatedContent]) {
        for item in graduated {
            if !self.is_graduated(&item.key) {
                if self.graduated_keys.len() >= MAX_GRADUATED_KEYS {
                    self.graduated_keys.pop_front();
                }
                self.graduated_keys.push_back(item.key.clone());
            }
        }
    }

    /// Builds the stdout delta string from graduated content.
    /// Returns (output_string, new_pending_newline_state).
    /// Uses \r\n for terminal compatibility.
    ///
    /// Newline logic: A newline is added BEFORE an item's content only if
    /// both the previous item had newline=true AND the current item has newline=true.
    /// This allows continuations (newline=false) to append without separator.
    pub fn format_stdout_delta(
        graduated: &[GraduatedContent],
        pending_newline: bool,
        boundary_lines: usize,
    ) -> (String, bool) {
        if graduated.is_empty() {
            return (String::new(), pending_newline);
        }

        let mut output = String::new();
        let mut current_pending = pending_newline;

        for item in graduated {
            if current_pending && item.newline {
                output.push_str("\r\n");
            }
            output.push_str(&item.content);
            current_pending = item.newline;
        }

        for _ in 0..boundary_lines {
            output.push_str("\r\n");
        }

        // If we added boundary newlines, the separator is already present,
        // so next frame shouldn't add another leading newline.
        let final_pending = if boundary_lines > 0 {
            false
        } else {
            current_pending
        };
        (output, final_pending)
    }

    /// Legacy method that both collects AND commits in one step.
    /// Prefer plan_graduation() + commit_graduation() for testability.
    pub fn graduate(&mut self, node: &Node, _width: usize) -> io::Result<Vec<GraduatedContent>> {
        let graduated = self.plan_graduation(node);
        self.commit_graduation(&graduated);
        Ok(graduated)
    }

    /// Read-only collection of static nodes (doesn't modify graduated_keys)
    fn collect_static_nodes_readonly(&self, node: &Node, graduated: &mut Vec<GraduatedContent>) {
        match node {
            Node::Static(static_node) => {
                if !self.graduated_keys.contains(&static_node.key) {
                    let content =
                        render_children_to_string(&static_node.children, GRADUATION_WIDTH);

                    if !content.is_empty() {
                        graduated.push(GraduatedContent {
                            key: static_node.key.clone(),
                            content,
                            newline: static_node.newline,
                        });
                    }
                }
            }

            Node::Box(boxnode) => {
                for child in &boxnode.children {
                    self.collect_static_nodes_readonly(child, graduated);
                }
            }

            Node::Fragment(children) => {
                for child in children {
                    self.collect_static_nodes_readonly(child, graduated);
                }
            }

            _ => {}
        }
    }
}

impl RenderFilter for GraduationState {
    fn skip_static(&self, key: &str) -> bool {
        self.is_graduated(key)
    }
}

#[derive(Debug, Clone)]
pub struct GraduatedContent {
    pub key: String,
    pub content: String,
    pub newline: bool,
}

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graduation_state_new_is_empty() {
        let state = GraduationState::new();

        assert_eq!(state.graduated_count(), 0);
        assert!(!state.is_graduated("any-key"));
    }

    #[test]
    fn is_graduated_returns_false_for_unknown() {
        let state = GraduationState::new();

        assert!(!state.is_graduated("unknown"));
        assert!(!state.is_graduated(""));
        assert!(!state.is_graduated("msg-1"));
    }

    #[test]
    fn graduated_count_increments() {
        use crate::tui::oil::node::{scrollback, text};

        let mut state = GraduationState::new();

        let tree1 = scrollback("msg-1", [text("First")]);
        state.graduate(&tree1, 80).unwrap();
        assert_eq!(state.graduated_count(), 1);

        let tree2 = scrollback("msg-2", [text("Second")]);
        state.graduate(&tree2, 80).unwrap();
        assert_eq!(state.graduated_count(), 2);
    }

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

    #[test]
    fn plan_graduation_is_pure() {
        use crate::tui::oil::node::{scrollback, text};

        let state = GraduationState::new();
        let tree = scrollback("key-1", [text("Content")]);

        let graduated1 = state.plan_graduation(&tree);
        let graduated2 = state.plan_graduation(&tree);

        assert_eq!(graduated1.len(), 1);
        assert_eq!(graduated2.len(), 1);
        assert_eq!(graduated1[0].key, "key-1");
        assert_eq!(state.graduated_count(), 0);
    }

    #[test]
    fn commit_graduation_marks_keys() {
        use crate::tui::oil::node::{scrollback, text};

        let mut state = GraduationState::new();
        let tree = scrollback("key-1", [text("Content")]);

        let graduated = state.plan_graduation(&tree);
        assert!(!state.is_graduated("key-1"));

        state.commit_graduation(&graduated);
        assert!(state.is_graduated("key-1"));

        let graduated_again = state.plan_graduation(&tree);
        assert!(graduated_again.is_empty());
    }

    #[test]
    fn format_stdout_delta_builds_output() {
        let graduated = vec![
            GraduatedContent {
                key: "k1".to_string(),
                content: "Hello".to_string(),
                newline: true,
            },
            GraduatedContent {
                key: "k2".to_string(),
                content: "World".to_string(),
                newline: true,
            },
        ];

        let (delta, new_pending) = GraduationState::format_stdout_delta(&graduated, false, 1);
        assert!(delta.contains("Hello"));
        assert!(delta.contains("World"));
        assert!(delta.contains("\r\n"));
        assert!(delta.ends_with("\r\n"));
        // With boundary_lines > 0, pending_newline resets to false
        // because the boundary already provides the separator
        assert!(!new_pending);
    }

    #[test]
    fn format_stdout_delta_boundary_after_content() {
        let graduated = vec![GraduatedContent {
            key: "k1".to_string(),
            content: "Content".to_string(),
            newline: true,
        }];

        let (delta, _) = GraduationState::format_stdout_delta(&graduated, false, 1);
        assert_eq!(delta, "Content\r\n");
    }

    #[test]
    fn format_stdout_delta_empty_returns_unchanged_pending() {
        let (delta, pending) = GraduationState::format_stdout_delta(&[], true, 1);
        assert!(delta.is_empty());
        assert!(pending);

        let (delta2, pending2) = GraduationState::format_stdout_delta(&[], false, 1);
        assert!(delta2.is_empty());
        assert!(!pending2);
    }
}
