//! Sliding Window Context
//!
//! Manages conversation history with automatic truncation to stay within token budgets.

use crate::handle::Message;
use crate::token::TokenBudget;
use serde::{Deserialize, Serialize};

/// Strategy for selecting which messages to keep when truncating
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TruncationStrategy {
    /// Keep most recent messages
    KeepRecent,

    /// Keep system messages + most recent user/assistant pairs
    KeepSystemAndRecent,

    /// Keep important messages (pinned) + recent
    KeepImportantAndRecent,
}

/// Sliding window context manager
///
/// Maintains a conversation history that stays within token budget constraints
/// by automatically truncating older messages when the budget is exceeded.
#[derive(Debug)]
pub struct SlidingWindowContext {
    /// Conversation history
    messages: Vec<Message>,

    /// Token budget tracker
    budget: TokenBudget,

    /// Target token count for truncation
    target_tokens: usize,

    /// Truncation strategy
    strategy: TruncationStrategy,

    /// Message indices that should be pinned (never truncated)
    pinned: Vec<usize>,
}

impl SlidingWindowContext {
    /// Create a new sliding window context
    pub fn new(max_tokens: usize, target_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            budget: TokenBudget::new(max_tokens),
            target_tokens,
            strategy: TruncationStrategy::KeepSystemAndRecent,
            pinned: Vec::new(),
        }
    }

    /// Add a message to the context
    ///
    /// If this exceeds the budget, older messages will be truncated.
    pub fn add_message(&mut self, _message: Message) {
        todo!("Implement SlidingWindowContext::add_message")
    }

    /// Get all messages in the current context
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the current token count
    pub fn token_count(&self) -> usize {
        self.budget.current()
    }

    /// Get remaining token budget
    pub fn remaining_budget(&self) -> usize {
        self.budget.remaining()
    }

    /// Pin a message so it won't be truncated
    pub fn pin_message(&mut self, index: usize) {
        if !self.pinned.contains(&index) {
            self.pinned.push(index);
        }
    }

    /// Unpin a message
    pub fn unpin_message(&mut self, index: usize) {
        self.pinned.retain(|&i| i != index);
    }

    /// Set the truncation strategy
    pub fn set_strategy(&mut self, strategy: TruncationStrategy) {
        self.strategy = strategy;
    }

    /// Truncate messages to fit within target token budget
    fn truncate(&mut self) {
        todo!("Implement SlidingWindowContext::truncate")
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.pinned.clear();
        self.budget.reset();
    }

    /// Correct token estimates based on actual provider counts
    pub fn correct_tokens(&mut self, actual_tokens: usize) {
        self.budget.correct(actual_tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Role;

    fn create_test_message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            metadata: None,
        }
    }

    #[test]
    fn test_new_context() {
        let context = SlidingWindowContext::new(1000, 800);
        assert_eq!(context.token_count(), 0);
        assert_eq!(context.remaining_budget(), 1000);
        assert!(context.messages().is_empty());
    }

    #[test]
    fn test_add_message() {
        // TODO: Implement when add_message is ready
    }

    #[test]
    fn test_pin_unpin() {
        let mut context = SlidingWindowContext::new(1000, 800);
        context.pin_message(0);
        assert!(context.pinned.contains(&0));

        context.unpin_message(0);
        assert!(!context.pinned.contains(&0));
    }

    #[test]
    fn test_clear() {
        let mut context = SlidingWindowContext::new(1000, 800);
        context.pin_message(0);

        context.clear();
        assert!(context.messages().is_empty());
        assert!(context.pinned.is_empty());
        assert_eq!(context.token_count(), 0);
    }

    #[test]
    fn test_strategy_setting() {
        let mut context = SlidingWindowContext::new(1000, 800);
        context.set_strategy(TruncationStrategy::KeepRecent);
        // Strategy is set, behavior tested when truncate is implemented
    }
}
