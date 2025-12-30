//! Conversation history management for ACP sessions
//!
//! This module manages conversation history across multiple turns, including
//! message storage, history pruning, and token limit management.
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on history management
//! - **Open/Closed**: Extensible for different storage strategies
//! - **Dependency Inversion**: Uses simple types, adaptable to different backends

use crate::Result;

/// Configuration for conversation history
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Maximum number of messages to retain
    pub max_messages: usize,

    /// Maximum total tokens to retain (approximate)
    pub max_tokens: usize,

    /// Whether to enable history persistence
    pub enable_persistence: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_messages: 100,
            max_tokens: 4000, // Reasonable default for context window
            enable_persistence: false,
        }
    }
}

/// A message in the conversation history
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryMessage {
    /// Role of the message sender (user, agent, system)
    pub role: MessageRole,

    /// Content of the message
    pub content: String,

    /// Approximate token count (for pruning)
    pub token_count: usize,
}

/// Role of a message sender in ACP conversation history.
///
/// Uses ACP protocol terminology (`Agent` instead of `Assistant`).
/// This type is intentionally separate from `crucible_core::traits::MessageRole`
/// to preserve the semantic distinction of "Agent" in ACP protocol contexts.
///
/// Use `From`/`Into` to convert between types when bridging ACP and LLM layers:
/// - `Agent` ↔ `Assistant` (bidirectional)
/// - `User` ↔ `User` (identity)
/// - `System` ↔ `System` (identity)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// Message from the user
    User,
    /// Message from the agent (ACP terminology for assistant)
    Agent,
    /// System message (e.g., context, instructions)
    System,
}

/// Convert ACP MessageRole to core LLM MessageRole
impl From<MessageRole> for crucible_core::traits::MessageRole {
    fn from(role: MessageRole) -> Self {
        match role {
            MessageRole::User => crucible_core::traits::MessageRole::User,
            MessageRole::Agent => crucible_core::traits::MessageRole::Assistant,
            MessageRole::System => crucible_core::traits::MessageRole::System,
        }
    }
}

/// Convert core LLM MessageRole to ACP MessageRole
///
/// Note: `Function` and `Tool` roles map to `Agent` since they represent
/// assistant-side operations in the ACP model.
impl From<crucible_core::traits::MessageRole> for MessageRole {
    fn from(role: crucible_core::traits::MessageRole) -> Self {
        match role {
            crucible_core::traits::MessageRole::User => MessageRole::User,
            crucible_core::traits::MessageRole::Assistant => MessageRole::Agent,
            crucible_core::traits::MessageRole::System => MessageRole::System,
            // Function and Tool are assistant-side operations
            crucible_core::traits::MessageRole::Function => MessageRole::Agent,
            crucible_core::traits::MessageRole::Tool => MessageRole::Agent,
        }
    }
}

impl HistoryMessage {
    /// Create a new history message
    pub fn new(role: MessageRole, content: String) -> Self {
        let token_count = estimate_tokens(&content);
        Self {
            role,
            content,
            token_count,
        }
    }

    /// Create a user message
    pub fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an agent message
    pub fn agent(content: String) -> Self {
        Self::new(MessageRole::Agent, content)
    }

    /// Create a system message
    pub fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }
}

/// Manages conversation history
pub struct ConversationHistory {
    config: HistoryConfig,
    messages: Vec<HistoryMessage>,
}

impl ConversationHistory {
    /// Create a new conversation history
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for history management
    pub fn new(config: HistoryConfig) -> Self {
        Self {
            config,
            messages: Vec::new(),
        }
    }

    /// Add a message to the history
    ///
    /// # Arguments
    ///
    /// * `message` - The message to add
    pub fn add_message(&mut self, message: HistoryMessage) -> Result<()> {
        self.messages.push(message);
        Ok(())
    }

    /// Get all messages in the history
    pub fn messages(&self) -> &[HistoryMessage] {
        &self.messages
    }

    /// Get the number of messages
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get the total token count
    pub fn total_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.token_count).sum()
    }

    /// Prune history to fit within configured limits
    ///
    /// Removes oldest messages first until both message count
    /// and token count are within limits.
    ///
    /// # Returns
    ///
    /// Number of messages pruned
    pub fn prune(&mut self) -> Result<usize> {
        let original_count = self.messages.len();

        // Prune by message count first
        if self.messages.len() > self.config.max_messages {
            let to_remove = self.messages.len() - self.config.max_messages;
            self.messages.drain(0..to_remove);
        }

        // Then prune by token count
        while self.total_tokens() > self.config.max_tokens && !self.messages.is_empty() {
            self.messages.remove(0);
        }

        let pruned = original_count - self.messages.len();
        Ok(pruned)
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Get the configuration
    pub fn config(&self) -> &HistoryConfig {
        &self.config
    }
}

/// Estimate token count for a string
///
/// This is a simple approximation. A real implementation would use
/// a proper tokenizer for the specific model being used.
fn estimate_tokens(text: &str) -> usize {
    // Rough approximation: ~4 characters per token
    text.len().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_creation() {
        let config = HistoryConfig::default();
        let history = ConversationHistory::new(config);

        assert_eq!(history.message_count(), 0);
        assert_eq!(history.total_tokens(), 0);
    }

    #[test]
    fn test_add_message() {
        let mut history = ConversationHistory::new(HistoryConfig::default());

        let message = HistoryMessage::user("Hello, how are you?".to_string());
        let result = history.add_message(message.clone());

        assert!(result.is_ok(), "Should add message successfully");
        assert_eq!(history.message_count(), 1);
        assert_eq!(history.messages()[0], message);
    }

    #[test]
    fn test_multiple_messages() {
        let mut history = ConversationHistory::new(HistoryConfig::default());

        history
            .add_message(HistoryMessage::user("Hello".to_string()))
            .unwrap();
        history
            .add_message(HistoryMessage::agent("Hi there!".to_string()))
            .unwrap();
        history
            .add_message(HistoryMessage::user("How can you help?".to_string()))
            .unwrap();

        assert_eq!(history.message_count(), 3);
        assert_eq!(history.messages()[0].role, MessageRole::User);
        assert_eq!(history.messages()[1].role, MessageRole::Agent);
        assert_eq!(history.messages()[2].role, MessageRole::User);
    }

    #[test]
    fn test_token_counting() {
        let mut history = ConversationHistory::new(HistoryConfig::default());

        let msg1 = HistoryMessage::user("Hello".to_string()); // ~2 tokens
        let msg2 = HistoryMessage::agent("Hi there, how can I help you?".to_string()); // ~8 tokens

        history.add_message(msg1).unwrap();
        history.add_message(msg2).unwrap();

        assert!(history.total_tokens() > 0, "Should count tokens");
        assert!(
            history.total_tokens() < 20,
            "Token estimate should be reasonable"
        );
    }

    #[test]
    fn test_prune_by_message_count() {
        let config = HistoryConfig {
            max_messages: 3,
            max_tokens: 10000,
            enable_persistence: false,
        };

        let mut history = ConversationHistory::new(config);

        // Add 5 messages
        for i in 0..5 {
            history
                .add_message(HistoryMessage::user(format!("Message {}", i)))
                .unwrap();
        }

        assert_eq!(history.message_count(), 5);

        // Prune should remove oldest 2 messages
        let pruned = history.prune().unwrap();
        assert_eq!(pruned, 2, "Should prune 2 messages");
        assert_eq!(history.message_count(), 3, "Should have 3 messages left");

        // Check that oldest messages were removed
        assert_eq!(history.messages()[0].content, "Message 2");
        assert_eq!(history.messages()[2].content, "Message 4");
    }

    #[test]
    fn test_prune_by_token_count() {
        let config = HistoryConfig {
            max_messages: 100,
            max_tokens: 50, // Very small token limit
            enable_persistence: false,
        };

        let mut history = ConversationHistory::new(config);

        // Add messages that will exceed token limit (50 tokens)
        // With ~4 chars per token, we need about 200+ characters total
        history.add_message(HistoryMessage::user("This is the first message with quite a bit of content to ensure we exceed the token limit".to_string())).unwrap();
        history
            .add_message(HistoryMessage::user(
                "This is the second message also with significant content to make sure we go over"
                    .to_string(),
            ))
            .unwrap();
        history
            .add_message(HistoryMessage::user(
                "Third message here with even more text for good measure".to_string(),
            ))
            .unwrap();
        history
            .add_message(HistoryMessage::user(
                "Final message to push us way over the limit".to_string(),
            ))
            .unwrap();

        let total_before = history.total_tokens();
        assert!(total_before > 50, "Should exceed token limit");

        // Prune should remove messages until under limit
        let pruned = history.prune().unwrap();
        assert!(pruned > 0, "Should prune at least one message");
        assert!(
            history.total_tokens() <= 50,
            "Should be under token limit after pruning"
        );
    }

    #[test]
    fn test_clear_history() {
        let mut history = ConversationHistory::new(HistoryConfig::default());

        history
            .add_message(HistoryMessage::user("Test".to_string()))
            .unwrap();
        history
            .add_message(HistoryMessage::agent("Response".to_string()))
            .unwrap();

        assert_eq!(history.message_count(), 2);

        history.clear();
        assert_eq!(history.message_count(), 0);
        assert_eq!(history.total_tokens(), 0);
    }

    #[test]
    fn test_message_role_helpers() {
        let user_msg = HistoryMessage::user("User message".to_string());
        let agent_msg = HistoryMessage::agent("Agent message".to_string());
        let system_msg = HistoryMessage::system("System message".to_string());

        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(agent_msg.role, MessageRole::Agent);
        assert_eq!(system_msg.role, MessageRole::System);
    }
}
