use super::content_block::StreamBlock;
use super::conversation::ConversationItem;
use crucible_core::types::{TreeNodeLabel, UndoTree};

pub type ConversationTree = UndoTree<ConversationItem>;

pub use crucible_core::types::TreeNode;
pub use crucible_core::types::TreeSummary;
pub use crucible_core::types::UndoNodeId as NodeId;

impl TreeNodeLabel for ConversationItem {
    fn tree_label(&self) -> String {
        match self {
            ConversationItem::UserMessage { content } => {
                let preview: String = content.chars().take(30).collect();
                let ellipsis = if content.len() > 30 { "..." } else { "" };
                format!("You: {}{}", preview, ellipsis)
            }
            ConversationItem::AssistantMessage { blocks, .. } => {
                let text = blocks
                    .iter()
                    .filter_map(|b| {
                        if let StreamBlock::Prose { text, .. } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .next()
                    .unwrap_or("...");
                let preview: String = text.chars().take(30).collect();
                let ellipsis = if text.len() > 30 { "..." } else { "" };
                format!("AI: {}{}", preview, ellipsis)
            }
            ConversationItem::ToolCall(tool) => format!("Tool: {}", tool.name),
            ConversationItem::Status(_) => "...".to_string(),
        }
    }
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
        ConversationItem::AssistantMessage {
            blocks: vec![StreamBlock::prose(s.to_string())],
            is_streaming: false,
        }
    }

    #[test]
    fn linear_conversation() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.push(user_msg("bye"));

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.total_nodes(), 3);
        assert!(!tree.has_branches());
    }

    #[test]
    fn rewind_and_branch() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));
        tree.push(assistant_msg("hi"));
        tree.push(user_msg("question 1"));

        tree.rewind(2);
        assert_eq!(tree.len(), 1);

        tree.push(user_msg("question 2"));
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.total_nodes(), 4);
        assert!(tree.has_branches());
    }

    #[test]
    fn rewind_past_root() {
        let mut tree = ConversationTree::new();
        tree.push(user_msg("hello"));

        let rewound = tree.rewind(10);
        assert_eq!(rewound, 1);
        assert_eq!(tree.len(), 0);
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
        assert_eq!(moved, 1);
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
