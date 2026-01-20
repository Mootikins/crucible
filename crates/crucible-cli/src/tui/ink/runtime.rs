use crate::tui::ink::node::{Node, StaticNode};
use crate::tui::ink::render::{render_to_string, RenderFilter};
use std::collections::HashSet;
use std::io::{self, Write};

pub struct GraduationState {
    graduated_keys: HashSet<String>,
    stdout_buffer: String,
    pending_newline: bool,
}

impl Default for GraduationState {
    fn default() -> Self {
        Self::new()
    }
}

impl GraduationState {
    pub fn new() -> Self {
        Self {
            graduated_keys: HashSet::new(),
            stdout_buffer: String::new(),
            pending_newline: false,
        }
    }

    pub fn is_graduated(&self, key: &str) -> bool {
        self.graduated_keys.contains(key)
    }

    pub fn graduated_count(&self) -> usize {
        self.graduated_keys.len()
    }

    pub fn clear(&mut self) {
        self.graduated_keys.clear();
        self.stdout_buffer.clear();
        self.pending_newline = false;
    }

    pub fn stdout_content(&self) -> &str {
        &self.stdout_buffer
    }

    pub fn graduate(&mut self, node: &Node, width: usize) -> io::Result<Vec<GraduatedContent>> {
        let mut graduated = Vec::new();
        self.collect_static_nodes(node, width, &mut graduated);
        Ok(graduated)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn collect_static_nodes(
        &mut self,
        node: &Node,
        width: usize,
        graduated: &mut Vec<GraduatedContent>,
    ) {
        match node {
            Node::Static(static_node) => {
                if !self.graduated_keys.contains(&static_node.key) {
                    // Use large width for graduation - let terminal handle wrapping
                    let graduation_width = 10000;
                    let content = render_to_string(
                        &Node::Fragment(static_node.children.clone()),
                        graduation_width,
                    );

                    if !content.is_empty() {
                        graduated.push(GraduatedContent {
                            key: static_node.key.clone(),
                            content,
                            newline: static_node.newline,
                        });
                        self.graduated_keys.insert(static_node.key.clone());
                    }
                }
            }

            Node::Box(boxnode) => {
                for child in &boxnode.children {
                    self.collect_static_nodes(child, width, graduated);
                }
            }

            Node::Fragment(children) => {
                for child in children {
                    self.collect_static_nodes(child, width, graduated);
                }
            }

            _ => {}
        }
    }

    pub fn flush_to_stdout(&mut self, graduated: &[GraduatedContent]) -> io::Result<()> {
        if graduated.is_empty() {
            return Ok(());
        }

        let mut stdout = io::stdout().lock();

        for item in graduated {
            if self.pending_newline && item.newline {
                writeln!(stdout)?;
                self.stdout_buffer.push('\n');
            }

            write!(stdout, "{}", item.content)?;
            self.stdout_buffer.push_str(&item.content);

            self.pending_newline = item.newline;
        }

        stdout.flush()
    }

    pub fn flush_to_buffer(&mut self, graduated: &[GraduatedContent]) {
        for item in graduated {
            if self.pending_newline && item.newline {
                self.stdout_buffer.push('\n');
            }

            self.stdout_buffer.push_str(&item.content);
            self.pending_newline = item.newline;
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
    width: u16,
    height: u16,
    graduation: GraduationState,
    viewport_buffer: String,
}

impl TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            graduation: GraduationState::new(),
            viewport_buffer: String::new(),
        }
    }

    pub fn render(&mut self, tree: &Node) {
        let graduated = self.graduation.graduate(tree, self.width as usize).unwrap();

        self.graduation.flush_to_buffer(&graduated);

        self.viewport_buffer = self.render_viewport(tree);
    }

    fn render_viewport(&self, tree: &Node) -> String {
        use crate::tui::ink::render::render_with_cursor_filtered;
        render_with_cursor_filtered(tree, self.width as usize, &self.graduation).content
    }

    pub fn stdout_content(&self) -> &str {
        self.graduation.stdout_content()
    }

    pub fn viewport_content(&self) -> &str {
        &self.viewport_buffer
    }

    pub fn graduated_count(&self) -> usize {
        self.graduation.graduated_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graduation_state_new_is_empty() {
        let state = GraduationState::new();

        assert_eq!(state.graduated_count(), 0);
        assert!(state.stdout_content().is_empty());
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
        use crate::tui::ink::node::{scrollback, text};

        let mut state = GraduationState::new();

        let tree1 = scrollback("msg-1", [text("First")]);
        state.graduate(&tree1, 80).unwrap();
        assert_eq!(state.graduated_count(), 1);

        let tree2 = scrollback("msg-2", [text("Second")]);
        state.graduate(&tree2, 80).unwrap();
        assert_eq!(state.graduated_count(), 2);
    }

    #[test]
    fn stdout_content_accumulates() {
        use crate::tui::ink::node::{scrollback, text};

        let mut state = GraduationState::new();

        let tree1 = scrollback("msg-1", [text("Hello")]);
        let graduated1 = state.graduate(&tree1, 80).unwrap();
        state.flush_to_buffer(&graduated1);

        assert!(state.stdout_content().contains("Hello"));

        let tree2 = scrollback("msg-2", [text("World")]);
        let graduated2 = state.graduate(&tree2, 80).unwrap();
        state.flush_to_buffer(&graduated2);

        let content = state.stdout_content();
        assert!(content.contains("Hello"));
        assert!(content.contains("World"));
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
        use crate::tui::ink::node::{col, scrollback, text};

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
}
