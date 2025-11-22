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

/// Manages an interactive chat session
pub struct ChatSession {
    config: ChatConfig,
    history: ConversationHistory,
    enricher: PromptEnricher,
    stream_handler: StreamHandler,
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

        Self {
            config,
            history,
            enricher,
            stream_handler,
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
            self.history.prune()?;
        }

        Ok(agent_response)
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
}
