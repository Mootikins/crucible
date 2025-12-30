//! Sliding Window Context
//!
//! Manages conversation history with automatic truncation to stay within token budgets.

use async_trait::async_trait;
use crucible_core::traits::{
    ContextError, ContextMessage, ContextOps, MessagePredicate, Position, Range,
};
use std::collections::VecDeque;

/// Sliding window context manager
///
/// Maintains a conversation history that stays within token budget constraints
/// by automatically truncating older messages when the budget is exceeded.
/// The system prompt is never truncated.
#[derive(Debug, Default)]
pub struct SlidingWindowContext {
    /// Conversation history (excludes system prompt)
    messages: VecDeque<ContextMessage>,

    /// System prompt (never trimmed)
    system_prompt: Option<String>,

    /// Target token count for automatic truncation
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

    /// Get the target token budget
    pub fn target_tokens(&self) -> usize {
        self.target_tokens
    }

    /// Set the target token budget
    pub fn set_target_tokens(&mut self, tokens: usize) {
        self.target_tokens = tokens;
    }

    /// Resolve a Range to concrete indices
    fn resolve_range(&self, range: &Range) -> std::ops::Range<usize> {
        let len = self.messages.len();
        match range {
            Range::All => 0..len,
            Range::Last(n) => len.saturating_sub(*n)..len,
            Range::First(n) => 0..(*n).min(len),
            Range::Indices(r) => r.start.min(len)..r.end.min(len),
        }
    }

    /// Find the index of the last user message
    fn find_last_user_index(&self) -> Option<usize> {
        self.messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| matches!(m.role, crucible_core::traits::MessageRole::User))
            .map(|(i, _)| i)
    }
}

#[async_trait]
impl ContextOps for SlidingWindowContext {
    fn messages(&self) -> Vec<ContextMessage> {
        self.messages.iter().cloned().collect()
    }

    fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    fn take(&self, range: Range) -> Vec<ContextMessage> {
        let r = self.resolve_range(&range);
        self.messages.iter().skip(r.start).take(r.len()).cloned().collect()
    }

    fn find(&self, predicate: MessagePredicate) -> Vec<(usize, &ContextMessage)> {
        self.messages
            .iter()
            .enumerate()
            .filter(|(_, m)| predicate(m))
            .collect()
    }

    fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
    }

    fn drop_range(&mut self, range: Range) {
        let r = self.resolve_range(&range);
        // Remove from back to front to maintain indices
        for i in (r.start..r.end).rev() {
            self.messages.remove(i);
        }
    }

    fn drop_matching(&mut self, predicate: MessagePredicate) {
        self.messages.retain(|m| !predicate(m));
    }

    fn replace(&mut self, range: Range, messages: Vec<ContextMessage>) {
        let r = self.resolve_range(&range);

        // Remove the range
        for _ in r.clone() {
            if r.start < self.messages.len() {
                self.messages.remove(r.start);
            }
        }

        // Insert new messages at the start position
        for (i, msg) in messages.into_iter().enumerate() {
            let insert_pos = (r.start + i).min(self.messages.len());
            self.messages.insert(insert_pos, msg);
        }
    }

    fn inject(&mut self, position: Position, messages: Vec<ContextMessage>) {
        let insert_idx = match position {
            Position::Start => 0,
            Position::End => self.messages.len(),
            Position::Before(idx) => idx.min(self.messages.len()),
            Position::After(idx) => (idx + 1).min(self.messages.len()),
            Position::BeforeLastUser => self.find_last_user_index().unwrap_or(self.messages.len()),
        };

        for (i, msg) in messages.into_iter().enumerate() {
            self.messages.insert(insert_idx + i, msg);
        }
    }

    fn clear(&mut self) {
        self.messages.clear();
        // Keep system_prompt
    }

    async fn summarize(&mut self, _range: Range) -> Result<(), ContextError> {
        // TODO: Implement actual summarization via CompletionBackend
        // For now, return not supported
        Err(ContextError::NotSupported(
            "Summarization requires a CompletionBackend to be configured".to_string(),
        ))
    }

    fn token_estimate(&self) -> usize {
        let system_tokens = self
            .system_prompt
            .as_ref()
            .map(|p| p.len() / 4)
            .unwrap_or(0);

        let message_tokens: usize = self
            .messages
            .iter()
            .map(|m| m.metadata.token_estimate)
            .sum();

        system_tokens + message_tokens
    }

    fn trim_to_budget(&mut self, max_tokens: usize) {
        while self.token_estimate() > max_tokens && !self.messages.is_empty() {
            self.messages.pop_front();
        }
    }

    fn message_count(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::MessageRole;

    #[test]
    fn test_new_context_is_empty() {
        let context = SlidingWindowContext::new(1000);
        assert_eq!(context.message_count(), 0);
        assert_eq!(context.token_estimate(), 0);
        assert!(context.messages().is_empty());
        assert_eq!(context.system_prompt(), None);
    }

    #[test]
    fn test_inject_and_get_messages() {
        let mut context = SlidingWindowContext::new(1000);

        context.inject(Position::End, vec![ContextMessage::user("Hello")]);
        context.inject(Position::End, vec![ContextMessage::assistant("Hi there")]);

        assert_eq!(context.message_count(), 2);

        let messages = context.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert_eq!(messages[1].content, "Hi there");
    }

    #[test]
    fn test_system_prompt() {
        let mut context = SlidingWindowContext::new(1000);

        context.set_system_prompt("You are helpful".to_string());
        assert_eq!(context.system_prompt(), Some("You are helpful"));

        context.inject(Position::End, vec![ContextMessage::user("Hello")]);

        // System prompt is separate from messages
        assert_eq!(context.message_count(), 1);
    }

    #[test]
    fn test_trim_removes_oldest() {
        let mut context = SlidingWindowContext::new(1000);

        // Add several messages: ~1 token each (4 chars)
        context.inject(Position::End, vec![ContextMessage::user("MSG1")]);
        context.inject(Position::End, vec![ContextMessage::assistant("MSG2")]);
        context.inject(Position::End, vec![ContextMessage::user("MSG3")]);
        context.inject(Position::End, vec![ContextMessage::assistant("MSG4")]);

        assert_eq!(context.message_count(), 4);

        // Trim to fit only 2 messages (2 tokens)
        context.trim_to_budget(2);

        assert_eq!(context.message_count(), 2);

        let messages = context.messages();
        assert_eq!(messages.len(), 2);
        // Should keep the most recent messages (MSG3, MSG4)
        assert_eq!(messages[0].content, "MSG3");
        assert_eq!(messages[1].content, "MSG4");
    }

    #[test]
    fn test_drop_range() {
        let mut context = SlidingWindowContext::new(1000);

        context.inject(Position::End, vec![
            ContextMessage::user("MSG1"),
            ContextMessage::assistant("MSG2"),
            ContextMessage::user("MSG3"),
        ]);

        // Drop first message
        context.drop_range(Range::First(1));

        assert_eq!(context.message_count(), 2);
        assert_eq!(context.messages()[0].content, "MSG2");
    }

    #[test]
    fn test_inject_before_last_user() {
        let mut context = SlidingWindowContext::new(1000);

        context.inject(Position::End, vec![
            ContextMessage::user("First question"),
            ContextMessage::assistant("First answer"),
            ContextMessage::user("Second question"),
        ]);

        // Inject context before the last user message
        context.inject(
            Position::BeforeLastUser,
            vec![ContextMessage::system("Context: relevant info")],
        );

        let messages = context.messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].content, "Context: relevant info");
        assert_eq!(messages[3].content, "Second question");
    }

    #[test]
    fn test_take() {
        let mut context = SlidingWindowContext::new(1000);

        context.inject(Position::End, vec![
            ContextMessage::user("MSG1"),
            ContextMessage::assistant("MSG2"),
            ContextMessage::user("MSG3"),
        ]);

        let last_two = context.take(Range::Last(2));
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].content, "MSG2");
        assert_eq!(last_two[1].content, "MSG3");

        // Original unchanged
        assert_eq!(context.message_count(), 3);
    }

    #[test]
    fn test_replace() {
        let mut context = SlidingWindowContext::new(1000);

        context.inject(Position::End, vec![
            ContextMessage::user("MSG1"),
            ContextMessage::assistant("MSG2"),
            ContextMessage::user("MSG3"),
        ]);

        // Replace middle message
        context.replace(
            Range::Indices(1..2),
            vec![ContextMessage::assistant("REPLACED")],
        );

        let messages = context.messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].content, "REPLACED");
    }

    #[test]
    fn test_clear_preserves_system_prompt() {
        let mut context = SlidingWindowContext::new(1000);

        context.set_system_prompt("System".to_string());
        context.inject(Position::End, vec![ContextMessage::user("Hello")]);

        context.clear();

        assert_eq!(context.system_prompt(), Some("System"));
        assert_eq!(context.message_count(), 0);
    }
}
