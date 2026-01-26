//! Graduation system - "Water" that flows to stdout
//!
//! In the oil/water metaphor:
//! - **Water** (this module): Content that has "settled" and graduates to terminal scrollback
//! - **Oil** (viewport): Live content that "floats" on top in the viewport
//!
//! Content starts as oil (rendered in viewport), then graduates to water (written to stdout)
//! when it becomes static. This separation enables efficient terminal rendering by only
//! re-rendering the dynamic viewport while graduated content remains in scrollback.
//!
//! # Invariants (tested in graduation_invariant_tests.rs)
//!
//! 1. **XOR Placement**: Content appears in exactly one of stdout OR viewport, never both
//! 2. **Monotonic**: Graduated count never decreases
//! 3. **Atomic**: Graduation commits before viewport filtering (no "flash" of missing content)
//! 4. **Stable**: Resize operations preserve content (no loss during height changes)

use crate::tui::oil::node::{ElementKind, Node};
use crate::tui::oil::render::{render_children_to_string, RenderFilter};
use std::collections::VecDeque;
use std::io;

/// Width used for rendering graduated content. Large value lets terminal handle wrapping.
/// This matches `NATURAL_TEXT_WIDTH` in markdown.rs - graduated content uses "natural" style.
pub const GRADUATION_WIDTH: usize = 10000;

/// Maximum number of graduated keys to track. Once full, oldest keys are evicted.
/// This bounds memory usage and provides natural cleanup for long sessions.
/// 256 messages is ~10 screens of typical chat content.
const MAX_GRADUATED_KEYS: usize = 256;

/// Tracks which messages have graduated from viewport to stdout.
///
/// # Invariants (tested in graduation_invariant_tests.rs)
///
/// 1. **XOR Placement**: Content appears in exactly one of stdout OR viewport, never both
/// 2. **Monotonic**: Graduated count never decreases
/// 3. **Atomic**: Graduation commits before viewport filtering (no "flash" of missing content)
/// 4. **Stable**: Resize operations preserve content (no loss during height changes)
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

    /// Pre-register keys as already graduated.
    ///
    /// This is used when content has already been written to stdout under different keys
    /// (e.g., streaming keys like `streaming-graduated-0`) and we're about to render
    /// the same content under new keys (e.g., `assistant-2`). By pre-registering the
    /// new keys, we prevent the content from being written to stdout twice.
    pub fn pre_graduate_keys(&mut self, keys: impl IntoIterator<Item = String>) {
        for key in keys {
            if !self.is_graduated(&key) {
                if self.graduated_keys.len() >= MAX_GRADUATED_KEYS {
                    self.graduated_keys.pop_front();
                }
                self.graduated_keys.push_back(key);
            }
        }
    }

    pub fn plan_graduation(&self, node: &Node) -> Vec<GraduatedContent> {
        let mut graduated = Vec::new();
        self.collect_static_nodes_readonly(node, &mut graduated);
        graduated
    }

    pub fn commit_graduation(&mut self, graduated: &[GraduatedContent]) {
        self.pre_graduate_keys(graduated.iter().map(|g| g.key.clone()));
    }

    pub fn format_stdout_delta(
        graduated: &[GraduatedContent],
        pending_newline: bool,
        boundary_lines: usize,
    ) -> (String, bool) {
        if graduated.is_empty() {
            return (String::new(), pending_newline);
        }

        let mut output = String::new();
        let mut prev_kind: Option<ElementKind> = if pending_newline {
            Some(ElementKind::Block)
        } else {
            None
        };

        for item in graduated {
            if item.kind.wants_blank_line_before(prev_kind) {
                output.push_str("\r\n");
            }
            output.push_str(&item.content);
            prev_kind = Some(item.kind);
        }

        for _ in 0..boundary_lines {
            output.push_str("\r\n");
        }

        let final_pending = if boundary_lines > 0 {
            false
        } else {
            prev_kind.map(|k| k.wants_newline_after()).unwrap_or(false)
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
                            kind: static_node.kind,
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

/// Content that has graduated from viewport to stdout (water).
#[derive(Debug, Clone)]
pub struct GraduatedContent {
    pub key: String,
    pub content: String,
    pub kind: ElementKind,
    pub newline: bool,
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
                kind: ElementKind::Block,
                newline: true,
            },
            GraduatedContent {
                key: "k2".to_string(),
                content: "World".to_string(),
                kind: ElementKind::Block,
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
            kind: ElementKind::Block,
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

    #[test]
    fn pre_graduate_keys_marks_keys_without_content() {
        use crate::tui::oil::node::{scrollback, text};

        let mut state = GraduationState::new();

        state.pre_graduate_keys(["assistant-1".to_string(), "assistant-2".to_string()]);

        assert!(state.is_graduated("assistant-1"));
        assert!(state.is_graduated("assistant-2"));
        assert_eq!(state.graduated_count(), 2);

        let tree = scrollback("assistant-1", [text("Content")]);
        let graduated = state.plan_graduation(&tree);
        assert!(
            graduated.is_empty(),
            "pre-graduated key should not graduate again"
        );
    }

    #[test]
    fn pre_graduate_keys_prevents_double_graduation() {
        use crate::tui::oil::node::{col, scrollback, text};

        let mut state = GraduationState::new();

        let streaming_tree = scrollback("streaming-graduated-0", [text("Hello world")]);
        let graduated = state.plan_graduation(&streaming_tree);
        assert_eq!(graduated.len(), 1);
        state.commit_graduation(&graduated);

        state.pre_graduate_keys(["assistant-1".to_string()]);

        let final_tree = col([
            scrollback("streaming-graduated-0", [text("Hello world")]),
            scrollback("assistant-1", [text("Hello world")]),
        ]);
        let graduated_final = state.plan_graduation(&final_tree);
        assert!(
            graduated_final.is_empty(),
            "both streaming key and pre-graduated final key should be skipped"
        );
    }
}
