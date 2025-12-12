//! Context management traits for LLM conversation history
//!
//! Defines abstractions for managing conversation context with support
//! for sliding window, token budgeting, and future stack operations.

use crate::traits::llm::LlmMessage;

/// Manages conversation context/history for LLM interactions
///
/// Implementations handle:
/// - Message storage and retrieval
/// - Token budget management
/// - Context trimming strategies
///
/// Designed to be Rune-accessible for future plugin context manipulation.
pub trait ContextManager: Send + Sync {
    /// Set the system prompt (never trimmed)
    fn set_system_prompt(&mut self, prompt: String);

    /// Get the system prompt if set
    fn get_system_prompt(&self) -> Option<&str>;

    /// Add a message to the context
    fn add_message(&mut self, msg: LlmMessage);

    /// Get all messages (including system prompt as first message if set)
    fn get_messages(&self) -> Vec<LlmMessage>;

    /// Trim context to fit within token budget
    /// Keeps system prompt, removes oldest messages first
    fn trim_to_budget(&mut self, max_tokens: usize);

    /// Clear all messages (keeps system prompt)
    fn clear(&mut self);

    /// Estimate current token usage
    fn token_estimate(&self) -> usize;

    /// Get message count (excluding system prompt)
    fn message_count(&self) -> usize;

    // Future stack operations (documented but not implemented yet)
    // These will be exposed to Rune plugins
    // fn checkpoint(&mut self, name: &str);
    // fn rollback(&mut self, name: &str) -> bool;
    // fn pop(&mut self, n: usize);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::llm::{LlmMessage, MessageRole};

    /// Mock implementation for testing
    struct MockContextManager {
        system_prompt: Option<String>,
        messages: Vec<LlmMessage>,
    }

    impl MockContextManager {
        fn new() -> Self {
            Self {
                system_prompt: None,
                messages: Vec::new(),
            }
        }

        // Simple token estimator: ~4 chars per token
        fn estimate_tokens(&self, text: &str) -> usize {
            (text.len() + 3) / 4
        }
    }

    impl ContextManager for MockContextManager {
        fn set_system_prompt(&mut self, prompt: String) {
            self.system_prompt = Some(prompt);
        }

        fn get_system_prompt(&self) -> Option<&str> {
            self.system_prompt.as_deref()
        }

        fn add_message(&mut self, msg: LlmMessage) {
            self.messages.push(msg);
        }

        fn get_messages(&self) -> Vec<LlmMessage> {
            let mut all_messages = Vec::new();

            // Include system prompt as first message if set
            if let Some(prompt) = &self.system_prompt {
                all_messages.push(LlmMessage::system(prompt.clone()));
            }

            all_messages.extend(self.messages.clone());
            all_messages
        }

        fn trim_to_budget(&mut self, max_tokens: usize) {
            let system_tokens = self
                .system_prompt
                .as_ref()
                .map(|p| self.estimate_tokens(p))
                .unwrap_or(0);

            // Calculate tokens for each message
            let msg_tokens: Vec<usize> = self
                .messages
                .iter()
                .map(|m| self.estimate_tokens(&m.content))
                .collect();

            // Find how many messages from the end fit in budget
            let mut total = system_tokens;
            let mut keep_count = 0;

            for tokens in msg_tokens.iter().rev() {
                if total + tokens <= max_tokens {
                    total += tokens;
                    keep_count += 1;
                } else {
                    break;
                }
            }

            // Keep only the last N messages that fit
            let remove_count = self.messages.len().saturating_sub(keep_count);
            if remove_count > 0 {
                self.messages.drain(0..remove_count);
            }
        }

        fn clear(&mut self) {
            self.messages.clear();
        }

        fn token_estimate(&self) -> usize {
            let system_tokens = self
                .system_prompt
                .as_ref()
                .map(|p| self.estimate_tokens(p))
                .unwrap_or(0);

            let message_tokens: usize = self
                .messages
                .iter()
                .map(|m| self.estimate_tokens(&m.content))
                .sum();

            system_tokens + message_tokens
        }

        fn message_count(&self) -> usize {
            self.messages.len()
        }
    }

    #[test]
    fn test_system_prompt() {
        let mut ctx = MockContextManager::new();
        assert_eq!(ctx.get_system_prompt(), None);

        ctx.set_system_prompt("You are helpful".to_string());
        assert_eq!(ctx.get_system_prompt(), Some("You are helpful"));

        // System prompt appears in messages
        let messages = ctx.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::System);
    }

    #[test]
    fn test_add_messages() {
        let mut ctx = MockContextManager::new();

        ctx.add_message(LlmMessage::user("Hello"));
        ctx.add_message(LlmMessage::assistant("Hi there"));

        assert_eq!(ctx.message_count(), 2);

        let messages = ctx.get_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_clear() {
        let mut ctx = MockContextManager::new();
        ctx.set_system_prompt("System".to_string());
        ctx.add_message(LlmMessage::user("Hello"));
        ctx.add_message(LlmMessage::assistant("Hi"));

        ctx.clear();

        // System prompt remains
        assert_eq!(ctx.get_system_prompt(), Some("System"));
        // Messages cleared
        assert_eq!(ctx.message_count(), 0);
        // Only system prompt in messages
        assert_eq!(ctx.get_messages().len(), 1);
    }

    #[test]
    fn test_token_estimate() {
        let mut ctx = MockContextManager::new();

        // Empty context
        assert_eq!(ctx.token_estimate(), 0);

        // Add system prompt (~4 chars per token)
        ctx.set_system_prompt("1234".to_string()); // ~1 token
        assert_eq!(ctx.token_estimate(), 1);

        // Add message
        ctx.add_message(LlmMessage::user("12345678".to_string())); // ~2 tokens
        assert_eq!(ctx.token_estimate(), 3);
    }

    #[test]
    fn test_trim_to_budget() {
        let mut ctx = MockContextManager::new();
        ctx.set_system_prompt("SYSPROMPT".to_string()); // 9 chars = 3 tokens

        // Add several messages
        ctx.add_message(LlmMessage::user("MSG1".to_string())); // 4 chars = 1 token
        ctx.add_message(LlmMessage::assistant("MSG2".to_string())); // 4 chars = 1 token
        ctx.add_message(LlmMessage::user("MSG3".to_string())); // 4 chars = 1 token
        ctx.add_message(LlmMessage::assistant("MSG4".to_string())); // 4 chars = 1 token

        assert_eq!(ctx.message_count(), 4);

        // Trim to fit only system + last 2 messages
        // Budget: 5 = system(3) + msg3(1) + msg4(1)
        ctx.trim_to_budget(5);

        assert_eq!(ctx.message_count(), 2);

        let messages = ctx.get_messages();
        // System + 2 messages
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[1].content, "MSG3");
        assert_eq!(messages[2].content, "MSG4");
    }

    #[test]
    fn test_get_messages_includes_system() {
        let mut ctx = MockContextManager::new();

        ctx.set_system_prompt("System prompt".to_string());
        ctx.add_message(LlmMessage::user("User message".to_string()));

        let messages = ctx.get_messages();

        // Should be: system + user
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[0].content, "System prompt");
        assert_eq!(messages[1].role, MessageRole::User);
        assert_eq!(messages[1].content, "User message");
    }

    #[test]
    fn test_message_count_excludes_system() {
        let mut ctx = MockContextManager::new();

        ctx.set_system_prompt("System".to_string());
        assert_eq!(ctx.message_count(), 0);

        ctx.add_message(LlmMessage::user("Hello".to_string()));
        assert_eq!(ctx.message_count(), 1);

        ctx.add_message(LlmMessage::assistant("Hi".to_string()));
        assert_eq!(ctx.message_count(), 2);
    }
}
