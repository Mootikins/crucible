//! Interactive chat interface for ACP sessions
//!
//! This module provides a user-facing chat interface that integrates
//! conversation history, context enrichment, and streaming responses.
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on chat session orchestration
//! - **Dependency Inversion**: Uses traits and types from other modules
//! - **Open/Closed**: Extensible chat strategies and handlers

use crate::{
    ConversationHistory, HistoryConfig, HistoryMessage,
    PromptEnricher, ContextConfig,
    StreamHandler, StreamConfig,
    AcpError, Result,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for chat sessions
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Configuration for conversation history
    pub history: HistoryConfig,

    /// Configuration for context enrichment
    pub context: ContextConfig,

    /// Configuration for response streaming
    pub streaming: StreamConfig,

    /// Whether to auto-prune history after each turn
    pub auto_prune: bool,

    /// Whether to enrich prompts with context
    pub enrich_prompts: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            history: HistoryConfig::default(),
            context: ContextConfig::default(),
            streaming: StreamConfig::default(),
            auto_prune: true,
            enrich_prompts: true,
        }
    }
}

/// Metadata about the conversation state
#[derive(Debug, Clone)]
pub struct ConversationState {
    /// Total number of turns (user message + agent response = 1 turn)
    pub turn_count: usize,

    /// Timestamp when the session started (Unix epoch seconds)
    pub started_at: u64,

    /// Timestamp of the last message (Unix epoch seconds)
    pub last_message_at: Option<u64>,

    /// Total number of tokens used in the conversation
    pub total_tokens_used: usize,

    /// Number of times history was pruned
    pub prune_count: usize,
}

impl ConversationState {
    /// Create a new conversation state
    fn new() -> Self {
        Self {
            turn_count: 0,
            started_at: current_timestamp(),
            last_message_at: None,
            total_tokens_used: 0,
            prune_count: 0,
        }
    }

    /// Get the duration of the conversation in seconds
    pub fn duration_secs(&self) -> u64 {
        current_timestamp() - self.started_at
    }

    /// Get the average tokens per turn
    pub fn avg_tokens_per_turn(&self) -> f64 {
        if self.turn_count == 0 {
            0.0
        } else {
            self.total_tokens_used as f64 / self.turn_count as f64
        }
    }
}

/// Get the current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Manages an interactive chat session
pub struct ChatSession {
    config: ChatConfig,
    history: ConversationHistory,
    enricher: PromptEnricher,
    stream_handler: StreamHandler,
    state: ConversationState,
}

impl ChatSession {
    /// Create a new chat session
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the chat session
    pub fn new(config: ChatConfig) -> Self {
        let history = ConversationHistory::new(config.history.clone());
        let enricher = PromptEnricher::new(config.context.clone());
        let stream_handler = StreamHandler::new(config.streaming.clone());
        let state = ConversationState::new();

        Self {
            config,
            history,
            enricher,
            stream_handler,
            state,
        }
    }

    /// Process a user message and generate a response
    ///
    /// # Arguments
    ///
    /// * `user_message` - The user's input message
    ///
    /// # Returns
    ///
    /// The agent's response
    ///
    /// # Errors
    ///
    /// Returns an error if message processing fails
    pub async fn send_message(&mut self, user_message: &str) -> Result<String> {
        // TDD Cycle 15 - GREEN: Implement message sending

        // Step 1: Add user message to history
        let user_msg = HistoryMessage::user(user_message.to_string());
        self.history.add_message(user_msg)?;

        // Step 2: Prepare prompt (with or without enrichment)
        let prompt = if self.config.enrich_prompts {
            self.enricher.enrich(user_message).await?
        } else {
            user_message.to_string()
        };

        // Step 3: Generate agent response (mock for now)
        // In a real implementation, this would call the actual agent
        let agent_response = self.generate_mock_response(&prompt).await?;

        // Step 4: Add agent response to history
        let agent_msg = HistoryMessage::agent(agent_response.clone());
        self.history.add_message(agent_msg)?;

        // Step 5: Auto-prune if enabled
        if self.config.auto_prune {
            let pruned = self.history.prune()?;
            if pruned > 0 {
                // TDD Cycle 16 - GREEN: Track prune count
                self.state.prune_count += 1;
            }
        }

        // TDD Cycle 16 - GREEN: Update conversation state
        self.update_state();

        Ok(agent_response)
    }

    /// Update conversation state after a turn
    fn update_state(&mut self) {
        // Increment turn count
        self.state.turn_count += 1;

        // Update timestamp
        self.state.last_message_at = Some(current_timestamp());

        // Update total tokens
        self.state.total_tokens_used = self.history.total_tokens();
    }

    /// Generate a mock response (placeholder for real agent integration)
    ///
    /// In a real implementation, this would send the prompt to an actual
    /// agent and stream back the response.
    async fn generate_mock_response(&self, _prompt: &str) -> Result<String> {
        // Simple mock response for testing
        Ok("This is a mock agent response. In a real implementation, this would come from the actual agent.".to_string())
    }

    /// Get the conversation history
    pub fn history(&self) -> &ConversationHistory {
        &self.history
    }

    /// Clear the conversation history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get the configuration
    pub fn config(&self) -> &ChatConfig {
        &self.config
    }

    /// Get the conversation state
    pub fn state(&self) -> &ConversationState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TDD Cycle 15 - RED: Test expects chat session creation
    #[test]
    fn test_chat_session_creation() {
        let config = ChatConfig::default();
        let session = ChatSession::new(config);

        assert_eq!(session.history().message_count(), 0);
        assert!(session.config().auto_prune);
        assert!(session.config().enrich_prompts);
    }

    // TDD Cycle 15 - RED: Test expects custom configuration
    #[test]
    fn test_custom_chat_config() {
        let config = ChatConfig {
            auto_prune: false,
            enrich_prompts: false,
            ..Default::default()
        };

        let session = ChatSession::new(config);
        assert!(!session.config().auto_prune);
        assert!(!session.config().enrich_prompts);
    }

    // TDD Cycle 15 - RED: Test expects message sending
    #[tokio::test]
    async fn test_send_message() {
        let mut session = ChatSession::new(ChatConfig::default());

        let response = session.send_message("Hello, agent!").await;

        // This should fail because send_message is not yet implemented
        // Once implemented, it should:
        // - Add user message to history
        // - Optionally enrich with context
        // - Generate agent response
        // - Add agent response to history
        // - Return the response
        assert!(response.is_ok(), "Should send message successfully");

        let response_text = response.unwrap();
        assert!(!response_text.is_empty(), "Response should not be empty");

        // History should contain both user and agent messages
        assert_eq!(session.history().message_count(), 2);
    }

    // TDD Cycle 15 - RED: Test expects context enrichment
    #[tokio::test]
    async fn test_context_enrichment_in_chat() {
        let config = ChatConfig {
            enrich_prompts: true,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        let response = session.send_message("What is semantic search?").await;

        assert!(response.is_ok());
        // When enrichment is enabled, the enricher should be used
        // (We can't easily verify this without mocking, but the integration should work)
    }

    // TDD Cycle 15 - RED: Test expects history auto-pruning
    #[tokio::test]
    async fn test_auto_prune() {
        let config = ChatConfig {
            history: HistoryConfig {
                max_messages: 4, // Very small limit for testing
                max_tokens: 10000,
                enable_persistence: false,
            },
            auto_prune: true,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);

        // Send multiple messages to exceed limit
        session.send_message("Message 1").await.unwrap();
        session.send_message("Message 2").await.unwrap();
        session.send_message("Message 3").await.unwrap();

        // Should have auto-pruned to stay within limits
        // 3 user messages + 3 agent responses = 6 messages
        // After pruning, should have max 4 messages
        assert!(session.history().message_count() <= 4,
            "Should auto-prune to stay within message limit");
    }

    // TDD Cycle 15 - RED: Test expects no auto-pruning when disabled
    #[tokio::test]
    async fn test_no_auto_prune() {
        let config = ChatConfig {
            history: HistoryConfig {
                max_messages: 2,
                max_tokens: 10000,
                enable_persistence: false,
            },
            auto_prune: false,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);

        // Send messages that exceed limit
        session.send_message("Message 1").await.unwrap();
        session.send_message("Message 2").await.unwrap();

        // Should NOT auto-prune, so we have 4 messages (2 user + 2 agent)
        assert_eq!(session.history().message_count(), 4,
            "Should not auto-prune when disabled");
    }

    // TDD Cycle 15 - RED: Test expects history clearing
    #[tokio::test]
    async fn test_clear_history() {
        let mut session = ChatSession::new(ChatConfig::default());

        session.send_message("First message").await.unwrap();
        session.send_message("Second message").await.unwrap();

        assert!(session.history().message_count() > 0);

        session.clear_history();
        assert_eq!(session.history().message_count(), 0);
    }

    // TDD Cycle 15 - RED: Test expects enrichment can be disabled
    #[tokio::test]
    async fn test_enrichment_disabled() {
        let config = ChatConfig {
            enrich_prompts: false,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        let response = session.send_message("Test query").await;

        assert!(response.is_ok());
        // When enrichment is disabled, the original query should be used directly
    }

    // TDD Cycle 16 - RED: Test expects state tracking initialization
    #[test]
    fn test_conversation_state_initialization() {
        let session = ChatSession::new(ChatConfig::default());
        let state = session.state();

        assert_eq!(state.turn_count, 0, "Should start with zero turns");
        assert_eq!(state.total_tokens_used, 0, "Should start with zero tokens");
        assert_eq!(state.prune_count, 0, "Should start with zero prunes");
        assert!(state.last_message_at.is_none(), "Should have no last message initially");
        assert!(state.started_at > 0, "Should have a start timestamp");
    }

    // TDD Cycle 16 - RED: Test expects turn counting
    #[tokio::test]
    async fn test_turn_counting() {
        let mut session = ChatSession::new(ChatConfig::default());

        // Initial state
        assert_eq!(session.state().turn_count, 0);

        // Send first message
        session.send_message("First message").await.unwrap();
        assert_eq!(session.state().turn_count, 1, "Should have 1 turn after first exchange");

        // Send second message
        session.send_message("Second message").await.unwrap();
        assert_eq!(session.state().turn_count, 2, "Should have 2 turns after second exchange");

        // Send third message
        session.send_message("Third message").await.unwrap();
        assert_eq!(session.state().turn_count, 3, "Should have 3 turns after third exchange");
    }

    // TDD Cycle 16 - RED: Test expects token tracking
    #[tokio::test]
    async fn test_token_tracking() {
        let mut session = ChatSession::new(ChatConfig::default());

        // Initial tokens
        assert_eq!(session.state().total_tokens_used, 0);

        // Send messages
        session.send_message("Hello").await.unwrap();

        // Tokens should be tracked from history
        let tokens_after_turn1 = session.state().total_tokens_used;
        assert!(tokens_after_turn1 > 0, "Should track tokens after first turn");

        session.send_message("How are you?").await.unwrap();
        let tokens_after_turn2 = session.state().total_tokens_used;
        assert!(tokens_after_turn2 > tokens_after_turn1, "Tokens should increase with more turns");
    }

    // TDD Cycle 16 - RED: Test expects timestamp tracking
    #[tokio::test]
    async fn test_timestamp_tracking() {
        let mut session = ChatSession::new(ChatConfig::default());

        // Initially no last message
        assert!(session.state().last_message_at.is_none());

        // After first message, should have timestamp
        session.send_message("Test").await.unwrap();
        let timestamp1 = session.state().last_message_at;
        assert!(timestamp1.is_some(), "Should have timestamp after first message");

        // Wait a tiny bit and send another
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        session.send_message("Another test").await.unwrap();
        let timestamp2 = session.state().last_message_at;

        assert!(timestamp2.is_some(), "Should have timestamp after second message");
        assert!(timestamp2.unwrap() >= timestamp1.unwrap(), "Timestamp should not go backwards");
    }

    // TDD Cycle 16 - RED: Test expects prune count tracking
    #[tokio::test]
    async fn test_prune_count_tracking() {
        let config = ChatConfig {
            history: HistoryConfig {
                max_messages: 4, // Very small to trigger pruning
                max_tokens: 10000,
                enable_persistence: false,
            },
            auto_prune: true,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        assert_eq!(session.state().prune_count, 0);

        // Send messages to trigger pruning
        session.send_message("Message 1").await.unwrap();
        session.send_message("Message 2").await.unwrap();
        session.send_message("Message 3").await.unwrap(); // Should trigger prune

        assert!(session.state().prune_count > 0, "Should have pruned at least once");
    }

    // TDD Cycle 16 - RED: Test expects duration calculation
    #[tokio::test]
    async fn test_conversation_duration() {
        let mut session = ChatSession::new(ChatConfig::default());

        // Duration should be very small initially
        let initial_duration = session.state().duration_secs();
        assert!(initial_duration < 5, "Initial duration should be very small");

        // Send a message
        session.send_message("Test").await.unwrap();

        // Duration should still be small but non-zero
        let duration_after = session.state().duration_secs();
        assert!(duration_after >= initial_duration, "Duration should not decrease");
    }

    // TDD Cycle 16 - RED: Test expects average tokens per turn
    #[tokio::test]
    async fn test_avg_tokens_per_turn() {
        let mut session = ChatSession::new(ChatConfig::default());

        // No turns yet
        assert_eq!(session.state().avg_tokens_per_turn(), 0.0);

        // After some turns
        session.send_message("Short").await.unwrap();
        session.send_message("A longer message here").await.unwrap();

        let avg = session.state().avg_tokens_per_turn();
        assert!(avg > 0.0, "Should have positive average");
        assert_eq!(session.state().turn_count, 2);
    }
}
