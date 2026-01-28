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

    pub fn plan_graduation(&self, node: &Node, width: usize) -> Vec<GraduatedContent> {
        let mut graduated = Vec::new();
        self.collect_static_nodes_readonly(node, width, &mut graduated);
        graduated
    }

    pub fn commit_graduation(&mut self, graduated: &[GraduatedContent]) {
        self.pre_graduate_keys(graduated.iter().map(|g| g.key.clone()));
    }

    pub fn format_stdout_delta(
        graduated: &[GraduatedContent],
        last_kind: Option<ElementKind>,
    ) -> (String, Option<ElementKind>) {
        if graduated.is_empty() {
            return (String::new(), last_kind);
        }

        let mut output = String::new();
        let mut prev_kind = last_kind;

        for (i, item) in graduated.iter().enumerate() {
            let next_kind = graduated.get(i + 1).map(|g| g.kind);

            if item.kind.wants_blank_line_before(prev_kind) {
                output.push_str("\r\n");
            }
            output.push_str(&item.content);
            if needs_newline_after(item.kind, next_kind) {
                output.push_str("\r\n");
            }
            prev_kind = Some(item.kind);
        }

        (output, prev_kind)
    }

    /// Legacy method that both collects AND commits in one step.
    /// Prefer plan_graduation() + commit_graduation() for testability.
    pub fn graduate(&mut self, node: &Node, width: usize) -> io::Result<Vec<GraduatedContent>> {
        let graduated = self.plan_graduation(node, width);
        self.commit_graduation(&graduated);
        Ok(graduated)
    }

    fn collect_static_nodes_readonly(
        &self,
        node: &Node,
        width: usize,
        graduated: &mut Vec<GraduatedContent>,
    ) {
        match node {
            Node::Static(static_node) => {
                if !self.graduated_keys.contains(&static_node.key) {
                    let raw = render_children_to_string(&static_node.children, width);
                    let content = raw.trim_end_matches(['\r', '\n']).to_string();

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
                    self.collect_static_nodes_readonly(child, width, graduated);
                }
            }

            Node::Fragment(children) => {
                for child in children {
                    self.collect_static_nodes_readonly(child, width, graduated);
                }
            }

            _ => {}
        }
    }
}

fn needs_newline_after(current: ElementKind, next: Option<ElementKind>) -> bool {
    current.wants_newline_after() && !matches!(next, Some(ElementKind::Continuation))
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
    fn graduated_content_has_no_trailing_newlines() {
        use crate::tui::oil::node::{col, scrollback, text};

        let cases: Vec<(&str, Node)> = vec![
            ("plain text", scrollback("a", [text("Hello")])),
            (
                "text with wrapper padding",
                scrollback("b", [col([text(""), text("Content"), text("")])]),
            ),
            (
                "multi-line content",
                scrollback("c", [col([text("Line 1"), text("Line 2")])]),
            ),
            (
                "content with trailing newlines",
                scrollback("d", [text("Trailing\r\n\r\n")]),
            ),
        ];

        for (label, tree) in cases {
            let state = GraduationState::new();
            let graduated = state.plan_graduation(&tree, 80);
            assert_eq!(graduated.len(), 1, "{}: should produce 1 item", label);
            let content = &graduated[0].content;
            assert!(
                !content.ends_with('\n') && !content.ends_with('\r'),
                "{}: content should have no trailing newlines, got: {:?}",
                label,
                content
            );
        }
    }

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

        let graduated1 = state.plan_graduation(&tree, 80);
        let graduated2 = state.plan_graduation(&tree, 80);

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

        let graduated = state.plan_graduation(&tree, 80);
        assert!(!state.is_graduated("key-1"));

        state.commit_graduation(&graduated);
        assert!(state.is_graduated("key-1"));

        let graduated_again = state.plan_graduation(&tree, 80);
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

        let (delta, last_kind) = GraduationState::format_stdout_delta(&graduated, None);
        assert_eq!(delta, "Hello\r\n\r\nWorld\r\n");
        assert_eq!(last_kind, Some(ElementKind::Block));
    }

    #[test]
    fn format_stdout_delta_block_spacing() {
        let graduated = vec![
            GraduatedContent {
                key: "user-0".to_string(),
                content: "user content".to_string(),
                kind: ElementKind::Block,
                newline: true,
            },
            GraduatedContent {
                key: "asst-0".to_string(),
                content: "assistant content".to_string(),
                kind: ElementKind::Block,
                newline: true,
            },
        ];

        let (delta, _) = GraduationState::format_stdout_delta(&graduated, None);
        assert_eq!(
            delta, "user content\r\n\r\nassistant content\r\n",
            "Block→Block: newline-after + blank-line-before = one blank line"
        );
    }

    #[test]
    fn format_stdout_delta_tool_then_block() {
        let graduated = vec![
            GraduatedContent {
                key: "tool-0".to_string(),
                content: " ✓ bash(ls)".to_string(),
                kind: ElementKind::ToolCall,
                newline: true,
            },
            GraduatedContent {
                key: "asst-0".to_string(),
                content: "response".to_string(),
                kind: ElementKind::Block,
                newline: true,
            },
        ];

        let (delta, _) = GraduationState::format_stdout_delta(&graduated, None);
        assert_eq!(
            delta, " ✓ bash(ls)\r\n\r\nresponse\r\n",
            "ToolCall→Block: newline-after + blank-line-before = one blank line"
        );
    }

    #[test]
    fn format_stdout_delta_single_block() {
        let graduated = vec![GraduatedContent {
            key: "k1".to_string(),
            content: "Content".to_string(),
            kind: ElementKind::Block,
            newline: true,
        }];

        let (delta, _) = GraduationState::format_stdout_delta(&graduated, None);
        assert_eq!(delta, "Content\r\n");
    }

    #[test]
    fn format_stdout_delta_empty_returns_unchanged_last_kind() {
        let (delta, last_kind) =
            GraduationState::format_stdout_delta(&[], Some(ElementKind::ToolCall));
        assert!(delta.is_empty());
        assert_eq!(last_kind, Some(ElementKind::ToolCall));

        let (delta2, last_kind2) = GraduationState::format_stdout_delta(&[], None);
        assert!(delta2.is_empty());
        assert_eq!(last_kind2, None);
    }

    #[test]
    fn format_stdout_delta_toolcall_then_block_across_frames() {
        let tool_graduated = vec![GraduatedContent {
            key: "tool-1".to_string(),
            content: "Tool output".to_string(),
            kind: ElementKind::ToolCall,
            newline: true,
        }];

        let (delta1, last_kind) = GraduationState::format_stdout_delta(&tool_graduated, None);
        assert_eq!(delta1, "Tool output\r\n");
        assert_eq!(last_kind, Some(ElementKind::ToolCall));

        let block_graduated = vec![GraduatedContent {
            key: "msg-1".to_string(),
            content: "Assistant text".to_string(),
            kind: ElementKind::Block,
            newline: true,
        }];

        let (delta2, _) = GraduationState::format_stdout_delta(&block_graduated, last_kind);
        assert_eq!(
            delta2, "\r\nAssistant text\r\n",
            "Block after ToolCall across frames: blank-line-before + content + newline-after"
        );
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
        let graduated = state.plan_graduation(&tree, 80);
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
        let graduated = state.plan_graduation(&streaming_tree, 80);
        assert_eq!(graduated.len(), 1);
        state.commit_graduation(&graduated);

        state.pre_graduate_keys(["assistant-1".to_string()]);

        let final_tree = col([
            scrollback("streaming-graduated-0", [text("Hello world")]),
            scrollback("assistant-1", [text("Hello world")]),
        ]);
        let graduated_final = state.plan_graduation(&final_tree, 80);
        assert!(
            graduated_final.is_empty(),
            "both streaming key and pre-graduated final key should be skipped"
        );
    }
}
