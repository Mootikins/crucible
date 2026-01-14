use crate::tui::ink::node::{Node, StaticNode};
use crate::tui::ink::render::render_to_string;
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

    pub fn stdout_content(&self) -> &str {
        &self.stdout_buffer
    }

    pub fn graduate(&mut self, node: &Node, width: usize) -> io::Result<Vec<GraduatedContent>> {
        let mut graduated = Vec::new();
        self.collect_static_nodes(node, width, &mut graduated);
        Ok(graduated)
    }

    fn collect_static_nodes(
        &mut self,
        node: &Node,
        width: usize,
        graduated: &mut Vec<GraduatedContent>,
    ) {
        match node {
            Node::Static(static_node) => {
                if !self.graduated_keys.contains(&static_node.key) {
                    let content =
                        render_to_string(&Node::Fragment(static_node.children.clone()), width);

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
        let filtered = self.filter_graduated(tree);
        render_to_string(&filtered, self.width as usize)
    }

    fn filter_graduated(&self, node: &Node) -> Node {
        match node {
            Node::Static(s) if self.graduation.is_graduated(&s.key) => Node::Empty,

            Node::Static(s) => Node::Static(StaticNode {
                key: s.key.clone(),
                children: s
                    .children
                    .iter()
                    .map(|c| self.filter_graduated(c))
                    .collect(),
                newline: s.newline,
            }),

            Node::Box(b) => Node::Box(crate::tui::ink::node::BoxNode {
                children: b
                    .children
                    .iter()
                    .map(|c| self.filter_graduated(c))
                    .collect(),
                direction: b.direction,
                size: b.size,
                padding: b.padding,
                margin: b.margin,
                border: b.border,
                style: b.style,
                justify: b.justify,
                align: b.align,
                gap: b.gap,
            }),

            Node::Fragment(children) => {
                Node::Fragment(children.iter().map(|c| self.filter_graduated(c)).collect())
            }

            other => other.clone(),
        }
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
