//! Sliding Window Context
//!
//! Manages conversation history with automatic truncation to stay within token budgets.

use crucible_core::traits::context::ContextManager;
use crucible_core::traits::llm::LlmMessage;
use std::collections::VecDeque;

/// Sliding window context manager
///
/// Maintains a conversation history that stays within token budget constraints
/// by automatically truncating older messages when the budget is exceeded.
/// The system prompt is never truncated.
#[derive(Debug)]
pub struct SlidingWindowContext {
    /// Conversation history (excludes system prompt)
    messages: VecDeque<LlmMessage>,

    /// System prompt (never trimmed)
    system_prompt: Option<String>,

    /// Target token count for automatic truncation (reserved for future use)
    #[allow(dead_code)]
    target_tokens: usize,
}

impl SlidingWindowContext {
    /// Create a new sliding window context
    ///
    /// # Arguments
    ///
    /// * `target_tokens` - Target token budget for the context window
    pub fn new(target_tokens: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            system_prompt: None,
            target_tokens,
        }
    }

    /// Estimate tokens for a single message
    ///
    /// Uses a simple heuristic: ~4 characters per token
    /// This is a rough approximation suitable for sliding window management.
    fn estimate_message_tokens(msg: &LlmMessage) -> usize {
        msg.content.len() / 4
    }
}

impl ContextManager for SlidingWindowContext {
    fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
    }

    fn get_system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    fn add_message(&mut self, msg: LlmMessage) {
        self.messages.push_back(msg);
    }

    fn get_messages(&self) -> Vec<LlmMessage> {
        let mut result = Vec::new();

        // Prepend system prompt as first message if set
        if let Some(ref prompt) = self.system_prompt {
            result.push(LlmMessage::system(prompt.clone()));
        }

        // Add all conversation messages
        result.extend(self.messages.iter().cloned());
        result
    }

    fn trim_to_budget(&mut self, max_tokens: usize) {
        // Keep system prompt (not counted in messages)
        // Remove oldest messages until under budget
        while self.token_estimate() > max_tokens && !self.messages.is_empty() {
            self.messages.pop_front();
        }
    }

    fn clear(&mut self) {
        self.messages.clear();
        // Keep system_prompt
    }

    fn token_estimate(&self) -> usize {
        // Estimate system prompt tokens
        let system_tokens = self
            .system_prompt
            .as_ref()
            .map(|p| p.len() / 4)
            .unwrap_or(0);

        // Estimate message tokens
        let message_tokens: usize = self
            .messages
            .iter()
            .map(Self::estimate_message_tokens)
            .sum();

        system_tokens + message_tokens
    }

    fn message_count(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::llm::MessageRole;

    #[test]
    fn test_new_context_is_empty() {
        let context = SlidingWindowContext::new(1000);
        assert_eq!(context.message_count(), 0);
        assert_eq!(context.token_estimate(), 0);
        assert!(context.get_messages().is_empty());
        assert_eq!(context.get_system_prompt(), None);
    }

    #[test]
    fn test_add_and_get_messages() {
        let mut context = SlidingWindowContext::new(1000);

        context.add_message(LlmMessage::user("Hello"));
        context.add_message(LlmMessage::assistant("Hi there"));

        assert_eq!(context.message_count(), 2);

        let messages = context.get_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert_eq!(messages[1].content, "Hi there");
    }

    #[test]
    fn test_system_prompt_prepended() {
        let mut context = SlidingWindowContext::new(1000);

        context.set_system_prompt("You are helpful".to_string());
        assert_eq!(context.get_system_prompt(), Some("You are helpful"));

        context.add_message(LlmMessage::user("Hello"));

        let messages = context.get_messages();
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[0].content, "You are helpful");
        assert_eq!(messages[1].role, MessageRole::User);
        assert_eq!(messages[1].content, "Hello");

        // message_count should exclude system prompt
        assert_eq!(context.message_count(), 1);
    }

    #[test]
    fn test_trim_removes_oldest() {
        let mut context = SlidingWindowContext::new(1000);

        // Add several messages: ~1 token each (4 chars)
        context.add_message(LlmMessage::user("MSG1".to_string()));
        context.add_message(LlmMessage::assistant("MSG2".to_string()));
        context.add_message(LlmMessage::user("MSG3".to_string()));
        context.add_message(LlmMessage::assistant("MSG4".to_string()));

        assert_eq!(context.message_count(), 4);

        // Trim to fit only 2 messages (2 tokens)
        context.trim_to_budget(2);

        assert_eq!(context.message_count(), 2);

        let messages = context.get_messages();
        assert_eq!(messages.len(), 2);
        // Should keep the most recent messages (MSG3, MSG4)
        assert_eq!(messages[0].content, "MSG3");
        assert_eq!(messages[1].content, "MSG4");
    }

    #[test]
    fn test_trim_preserves_system_prompt() {
        let mut context = SlidingWindowContext::new(1000);

        context.set_system_prompt("SYSPROMPT".to_string()); // 9 chars = ~2 tokens

        // Add messages
        context.add_message(LlmMessage::user("MSG1".to_string())); // 4 chars = 1 token
        context.add_message(LlmMessage::assistant("MSG2".to_string())); // 4 chars = 1 token
        context.add_message(LlmMessage::user("MSG3".to_string())); // 4 chars = 1 token

        // Trim to budget that includes system + 1 message
        // Budget: 3 tokens = system(2) + msg3(1)
        context.trim_to_budget(3);

        // System prompt should still be present
        assert_eq!(context.get_system_prompt(), Some("SYSPROMPT"));

        // Should have only 1 message left
        assert_eq!(context.message_count(), 1);

        let messages = context.get_messages();
        assert_eq!(messages.len(), 2); // system + 1 message
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[0].content, "SYSPROMPT");
        assert_eq!(messages[1].content, "MSG3");
    }

    #[test]
    fn test_clear_preserves_system_prompt() {
        let mut context = SlidingWindowContext::new(1000);

        context.set_system_prompt("System".to_string());
        context.add_message(LlmMessage::user("Hello"));
        context.add_message(LlmMessage::assistant("Hi"));

        assert_eq!(context.message_count(), 2);

        context.clear();

        // System prompt should remain
        assert_eq!(context.get_system_prompt(), Some("System"));

        // Messages should be cleared
        assert_eq!(context.message_count(), 0);

        // get_messages should only return system prompt
        let messages = context.get_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::System);
    }

    #[test]
    fn test_token_estimate() {
        let mut context = SlidingWindowContext::new(1000);

        // Empty context
        assert_eq!(context.token_estimate(), 0);

        // Add system prompt: 8 chars = 2 tokens
        context.set_system_prompt("12345678".to_string());
        assert_eq!(context.token_estimate(), 2);

        // Add message: 4 chars = 1 token
        context.add_message(LlmMessage::user("1234".to_string()));
        assert_eq!(context.token_estimate(), 3);

        // Add another message: 8 chars = 2 tokens
        context.add_message(LlmMessage::assistant("12345678".to_string()));
        assert_eq!(context.token_estimate(), 5);
    }

    #[test]
    fn test_trim_to_zero_removes_all_messages() {
        let mut context = SlidingWindowContext::new(1000);

        // Use messages that are at least 4 chars so they register as tokens
        context.add_message(LlmMessage::user("HELLO".to_string())); // 5 chars = 1 token
        context.add_message(LlmMessage::assistant("WORLD".to_string())); // 5 chars = 1 token

        assert_eq!(context.message_count(), 2);

        // Trim to 0 should remove all messages that have token cost
        context.trim_to_budget(0);

        assert_eq!(context.message_count(), 0);
        assert!(context.get_messages().is_empty());
    }

    #[test]
    fn test_message_count_excludes_system_prompt() {
        let mut context = SlidingWindowContext::new(1000);

        // System prompt alone doesn't count
        context.set_system_prompt("System".to_string());
        assert_eq!(context.message_count(), 0);

        // Add user messages
        context.add_message(LlmMessage::user("Hello".to_string()));
        assert_eq!(context.message_count(), 1);

        context.add_message(LlmMessage::assistant("Hi".to_string()));
        assert_eq!(context.message_count(), 2);
    }

    #[test]
    fn test_large_messages_trim_correctly() {
        let mut context = SlidingWindowContext::new(1000);

        // Create a large message: 400 chars = 100 tokens
        let large_msg = "x".repeat(400);
        context.add_message(LlmMessage::user(large_msg.clone()));

        // Create small messages: 4 chars = 1 token each
        context.add_message(LlmMessage::assistant("MSG2".to_string()));
        context.add_message(LlmMessage::user("MSG3".to_string()));

        assert_eq!(context.message_count(), 3);

        // Trim to 2 tokens - should remove the large message
        context.trim_to_budget(2);

        assert_eq!(context.message_count(), 2);
        let messages = context.get_messages();
        assert_eq!(messages[0].content, "MSG2");
        assert_eq!(messages[1].content, "MSG3");
    }
}
