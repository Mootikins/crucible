//! Conversation tree for vim-style undo/redo
//!
//! Stores conversation history as a tree structure where branching occurs
//! when rewinding and adding new messages. Similar to vim's undo tree.

use super::conversation::ConversationItem;
use std::collections::VecDeque;

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub item: ConversationItem,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug)]
pub struct ConversationTree {
    nodes: Vec<TreeNode>,
    /// Current position in the tree (points to most recent item on current branch)
    current: Option<NodeId>,
    /// Root nodes (conversations can have multiple starting points, though typically one)
    roots: Vec<NodeId>,
}

impl Default for ConversationTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationTree {
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

    pub fn push(&mut self, item: ConversationItem) -> NodeId {
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

    /// Rewind n steps back from current position.
    /// Returns the number of steps actually rewound.
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

    /// Move forward to the most recent child (follows first child at each branch).
    /// Returns the number of steps moved forward.
    pub fn forward(&mut self, n: usize) -> usize {
        let mut moved = 0;
        for _ in 0..n {
            if let Some(current_id) = self.current {
                if let Some(&child_id) = self.nodes[current_id].children.last() {
                    self.current = Some(child_id);
                    moved += 1;
                } else {
                    break;
                }
            } else if let Some(&root_id) = self.roots.first() {
                self.current = Some(root_id);
                moved += 1;
            } else {
                break;
            }
        }
        moved
    }

    /// Get the current path from root to current position.
    pub fn current_path(&self) -> Vec<&ConversationItem> {
        let mut path = VecDeque::new();
        let mut node_id = self.current;

        while let Some(id) = node_id {
            path.push_front(&self.nodes[id].item);
            node_id = self.nodes[id].parent;
        }

        path.into_iter().collect()
    }

    /// Get mutable reference to current node's item.
    pub fn current_item_mut(&mut self) -> Option<&mut ConversationItem> {
        self.current.map(|id| &mut self.nodes[id].item)
    }

    /// Get reference to current node's item.
    pub fn current_item(&self) -> Option<&ConversationItem> {
        self.current.map(|id| &self.nodes[id].item)
    }

    pub fn has_branches(&self) -> bool {
        self.roots.len() > 1 || self.nodes.iter().any(|n| n.children.len() > 1)
    }

    /// Get number of branches at current node's parent.
    pub fn branch_count(&self) -> usize {
        if let Some(current_id) = self.current {
            if let Some(parent_id) = self.nodes[current_id].parent {
                return self.nodes[parent_id].children.len();
            }
        }
        self.roots.len()
    }

    /// Get depth from root to current position.
    pub fn depth(&self) -> usize {
        self.current_path().len()
    }

    /// Clear all nodes and reset state.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.current = None;
        self.roots.clear();
    }

    /// Get total node count (for stats/debugging).
    pub fn total_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Get a summary of the tree structure for :undo display.
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

    /// Iterate over nodes for tree visualization.
    pub fn iter_nodes(&self) -> impl Iterator<Item = (NodeId, &TreeNode)> {
        self.nodes.iter().enumerate()
    }

    pub fn render_ascii(&self, max_lines: usize) -> String {
        if self.nodes.is_empty() {
            return String::from("(empty)");
        }

        let mut lines = Vec::new();
        let current_path: std::collections::HashSet<_> =
            self.current_path_ids().into_iter().collect();

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

    fn current_path_ids(&self) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut node_id = self.current;
        while let Some(id) = node_id {
            path.push(id);
            node_id = self.nodes[id].parent;
        }
        path.reverse();
        path
    }

    fn render_node_ascii(
        &self,
        node_id: NodeId,
        prefix: &str,
        is_last: bool,
        current_path: &std::collections::HashSet<NodeId>,
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

        let label = match &node.item {
            ConversationItem::UserMessage { content } => {
                let preview = content.chars().take(30).collect::<String>();
                let ellipsis = if content.len() > 30 { "..." } else { "" };
                format!("You: {}{}", preview, ellipsis)
            }
            ConversationItem::AssistantMessage { blocks, .. } => {
                let text = blocks
                    .iter()
                    .filter_map(|b| {
                        if let crate::tui::content_block::StreamBlock::Prose { text, .. } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .next()
                    .unwrap_or("...");
                let preview = text.chars().take(30).collect::<String>();
                let ellipsis = if text.len() > 30 { "..." } else { "" };
                format!("AI: {}{}", preview, ellipsis)
            }
            ConversationItem::ToolCall(tool) => format!("Tool: {}", tool.name),
            ConversationItem::Status(_) => "...".to_string(),
        };

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

    fn user_msg(s: &str) -> ConversationItem {
        ConversationItem::UserMessage {
            content: s.to_string(),
        }
    }

    fn assistant_msg(s: &str) -> ConversationItem {
        use crate::tui::content_block::StreamBlock;
        ConversationItem::AssistantMessage {
            blocks: vec![StreamBlock::Prose {
                text: s.to_string(),
                is_complete: true,
            }],
            is_streaming: false,
        }
    }

    #[test]
    fn linear_conversation() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi there"));
        tree.push(user_msg("how are you"));
        tree.push(assistant_msg("i'm good"));

        assert_eq!(tree.len(), 4);
        assert_eq!(tree.total_nodes(), 4);
        assert!(!tree.has_branches());
    }

    #[test]
    fn rewind_and_branch() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.push(user_msg("question 1"));
        tree.push(assistant_msg("answer 1"));

        // Rewind 2 steps (back to "hi")
        let rewound = tree.rewind(2);
        assert_eq!(rewound, 2);
        assert_eq!(tree.len(), 2);

        // Add new branch
        tree.push(user_msg("question 2"));
        tree.push(assistant_msg("answer 2"));

        // Current path should show the new branch
        assert_eq!(tree.len(), 4);
        assert_eq!(tree.total_nodes(), 6); // Original 4 + 2 new
        assert!(tree.has_branches());
    }

    #[test]
    fn rewind_past_root() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));

        let rewound = tree.rewind(10);
        assert_eq!(rewound, 2);
        assert_eq!(tree.len(), 0);
        assert!(tree.current.is_none());
    }

    #[test]
    fn forward_navigation() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.push(user_msg("bye"));

        tree.rewind(2);
        assert_eq!(tree.len(), 1);

        let moved = tree.forward(1);
        assert_eq!(moved, 1);
        assert_eq!(tree.len(), 2);

        let moved = tree.forward(10);
        assert_eq!(moved, 1); // Only one more step available
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn empty_tree() {
        let tree = ConversationTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(!tree.has_branches());
    }

    #[test]
    fn clear_tree() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.clear();

        assert!(tree.is_empty());
        assert_eq!(tree.total_nodes(), 0);
    }

    #[test]
    fn render_ascii_empty() {
        let tree = ConversationTree::new();
        assert_eq!(tree.render_ascii(10), "(empty)");
    }

    #[test]
    fn render_ascii_linear() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi there"));

        let ascii = tree.render_ascii(10);
        assert!(ascii.contains("You: hello"));
        assert!(ascii.contains("AI: hi there"));
        assert!(ascii.contains("●"));
    }

    #[test]
    fn render_ascii_with_branch() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.push(user_msg("question 1"));
        tree.push(assistant_msg("answer 1"));

        tree.rewind(2);
        tree.push(user_msg("question 2"));

        let ascii = tree.render_ascii(20);
        assert!(ascii.contains("question 1"));
        assert!(ascii.contains("question 2"));
        assert!(ascii.contains("├─") || ascii.contains("└─"));
    }
}
