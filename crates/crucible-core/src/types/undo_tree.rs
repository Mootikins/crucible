//! Generic undo tree for vim-style branching history
//!
//! Stores history as a tree where branching occurs when rewinding and adding
//! new items. Used for conversation history, document editing, etc.

use std::collections::HashSet;

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub struct TreeNode<T> {
    pub item: T,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug)]
pub struct UndoTree<T> {
    nodes: Vec<TreeNode<T>>,
    current: Option<NodeId>,
    roots: Vec<NodeId>,
}

impl<T: Clone> Default for UndoTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> UndoTree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            current: None,
            roots: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.current_path().len()
    }

    pub fn total_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn push(&mut self, item: T) -> NodeId {
        let id = self.nodes.len();
        let node = TreeNode {
            item,
            parent: self.current,
            children: Vec::new(),
        };

        self.nodes.push(node);

        if let Some(parent_id) = self.current {
            self.nodes[parent_id].children.push(id);
        } else {
            self.roots.push(id);
        }

        self.current = Some(id);
        id
    }

    pub fn rewind(&mut self, n: usize) -> usize {
        let mut rewound = 0;
        for _ in 0..n {
            if let Some(current_id) = self.current {
                self.current = self.nodes[current_id].parent;
                rewound += 1;
            } else {
                break;
            }
        }
        rewound
    }

    pub fn forward(&mut self, n: usize) -> usize {
        let mut moved = 0;
        for _ in 0..n {
            if let Some(current_id) = self.current {
                if let Some(&first_child) = self.nodes[current_id].children.first() {
                    self.current = Some(first_child);
                    moved += 1;
                } else {
                    break;
                }
            } else if let Some(&first_root) = self.roots.first() {
                self.current = Some(first_root);
                moved += 1;
            } else {
                break;
            }
        }
        moved
    }

    pub fn current_path(&self) -> Vec<&T> {
        self.current_path_ids()
            .into_iter()
            .map(|id| &self.nodes[id].item)
            .collect()
    }

    pub fn current_item(&self) -> Option<&T> {
        self.current.map(|id| &self.nodes[id].item)
    }

    pub fn current_item_mut(&mut self) -> Option<&mut T> {
        self.current.map(|id| &mut self.nodes[id].item)
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.current = None;
        self.roots.clear();
    }

    pub fn has_branches(&self) -> bool {
        self.nodes.iter().any(|n| n.children.len() > 1)
    }

    pub fn depth(&self) -> usize {
        self.current_path_ids().len()
    }

    pub fn tree_summary(&self) -> TreeSummary {
        TreeSummary {
            total_nodes: self.nodes.len(),
            current_depth: self.depth(),
            branch_points: self.count_branch_points(),
            current_node: self.current,
        }
    }

    fn count_branch_points(&self) -> usize {
        self.nodes.iter().filter(|n| n.children.len() > 1).count()
    }

    pub fn current_path_ids(&self) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut node_id = self.current;
        while let Some(id) = node_id {
            path.push(id);
            node_id = self.nodes[id].parent;
        }
        path.reverse();
        path
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = (NodeId, &TreeNode<T>)> {
        self.nodes.iter().enumerate()
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn node(&self, id: NodeId) -> Option<&TreeNode<T>> {
        self.nodes.get(id)
    }

    pub fn current_id(&self) -> Option<NodeId> {
        self.current
    }
}

pub trait TreeNodeLabel {
    fn tree_label(&self) -> String;
}

impl<T: Clone + TreeNodeLabel> UndoTree<T> {
    pub fn render_ascii(&self, max_lines: usize) -> String {
        if self.nodes.is_empty() {
            return String::from("(empty)");
        }

        let mut lines = Vec::new();
        let current_path: HashSet<_> = self.current_path_ids().into_iter().collect();

        for &root_id in &self.roots {
            self.render_node_ascii(root_id, "", true, &current_path, &mut lines, max_lines);
            if lines.len() >= max_lines {
                break;
            }
        }

        if lines.len() >= max_lines {
            lines.push("...".to_string());
        }

        lines.join("\n")
    }

    fn render_node_ascii(
        &self,
        node_id: NodeId,
        prefix: &str,
        is_last: bool,
        current_path: &HashSet<NodeId>,
        lines: &mut Vec<String>,
        max_lines: usize,
    ) {
        if lines.len() >= max_lines {
            return;
        }

        let node = &self.nodes[node_id];
        let is_current = Some(node_id) == self.current;
        let is_on_path = current_path.contains(&node_id);

        let marker = if is_current {
            "●"
        } else if is_on_path {
            "◐"
        } else {
            "○"
        };

        let label = node.item.tree_label();

        let connector = if is_last { "└─" } else { "├─" };
        let line = if prefix.is_empty() {
            format!("{} {}", marker, label)
        } else {
            format!("{}{}{} {}", prefix, connector, marker, label)
        };
        lines.push(line);

        let child_prefix = if prefix.is_empty() {
            String::new()
        } else if is_last {
            format!("{}  ", prefix)
        } else {
            format!("{}│ ", prefix)
        };

        for (i, &child_id) in node.children.iter().enumerate() {
            let child_is_last = i == node.children.len() - 1;
            let next_prefix = if prefix.is_empty() {
                "  ".to_string()
            } else {
                child_prefix.clone()
            };
            self.render_node_ascii(
                child_id,
                &next_prefix,
                child_is_last,
                current_path,
                lines,
                max_lines,
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeSummary {
    pub total_nodes: usize,
    pub current_depth: usize,
    pub branch_points: usize,
    pub current_node: Option<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestItem(String);

    impl TreeNodeLabel for TestItem {
        fn tree_label(&self) -> String {
            self.0.clone()
        }
    }

    #[test]
    fn empty_tree() {
        let tree: UndoTree<TestItem> = UndoTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(!tree.has_branches());
    }

    #[test]
    fn linear_conversation() {
        let mut tree = UndoTree::new();
        tree.push(TestItem("a".into()));
        tree.push(TestItem("b".into()));
        tree.push(TestItem("c".into()));

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.total_nodes(), 3);
        assert!(!tree.has_branches());
    }

    #[test]
    fn rewind_and_branch() {
        let mut tree = UndoTree::new();
        tree.push(TestItem("a".into()));
        tree.push(TestItem("b".into()));
        tree.push(TestItem("c".into()));

        tree.rewind(2);
        assert_eq!(tree.len(), 1);

        tree.push(TestItem("d".into()));
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.total_nodes(), 4);
        assert!(tree.has_branches());
    }

    #[test]
    fn render_ascii_empty() {
        let tree: UndoTree<TestItem> = UndoTree::new();
        assert_eq!(tree.render_ascii(10), "(empty)");
    }

    #[test]
    fn render_ascii_linear() {
        let mut tree = UndoTree::new();
        tree.push(TestItem("hello".into()));
        tree.push(TestItem("world".into()));

        let ascii = tree.render_ascii(10);
        assert!(ascii.contains("hello"));
        assert!(ascii.contains("world"));
        assert!(ascii.contains("●"));
    }

    #[test]
    fn render_ascii_with_branch() {
        let mut tree = UndoTree::new();
        tree.push(TestItem("root".into()));
        tree.push(TestItem("branch1".into()));

        tree.rewind(1);
        tree.push(TestItem("branch2".into()));

        let ascii = tree.render_ascii(20);
        assert!(ascii.contains("branch1"));
        assert!(ascii.contains("branch2"));
        assert!(ascii.contains("├─") || ascii.contains("└─"));
    }

    #[test]
    fn clear_tree() {
        let mut tree = UndoTree::new();
        tree.push(TestItem("a".into()));
        tree.push(TestItem("b".into()));
        tree.clear();

        assert!(tree.is_empty());
        assert_eq!(tree.total_nodes(), 0);
    }
}
