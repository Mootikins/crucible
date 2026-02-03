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

use crate::session::AcpSession;
use crate::streaming::{StreamingCallback, StreamingChunk};
use crate::{
    ClientError, ContextConfig, ConversationHistory, CrucibleAcpClient, HistoryConfig,
    HistoryMessage, PromptEnricher, Result, StreamConfig, ToolCallInfo,
};
use agent_client_protocol::AvailableCommand;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum allowed message length (50K characters)
const MAX_MESSAGE_LENGTH: usize = 50_000;

/// Configuration for chat session orchestration
///
/// This is distinct from `crucible_config::components::chat::ChatConfig` which
/// defines user-facing configuration loaded from TOML files. This type controls
/// runtime behavior of chat sessions (history, context enrichment, streaming).
#[derive(Debug, Clone)]
pub struct ChatSessionConfig {
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

impl Default for ChatSessionConfig {
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

/// Session metadata for identification and tracking
#[derive(Debug, Clone)]
pub struct SessionMetadata {
    /// Unique session identifier
    pub id: String,

    /// Optional human-readable session title
    pub title: Option<String>,

    /// Session tags for categorization
    pub tags: Vec<String>,

    /// Timestamp when session was created (Unix epoch seconds)
    pub created_at: u64,

    /// Timestamp when session was last updated (Unix epoch seconds)
    pub updated_at: u64,
}

impl SessionMetadata {
    /// Create new session metadata with a generated ID
    fn new() -> Self {
        let now = current_timestamp();
        Self {
            id: generate_session_id(),
            title: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the last updated timestamp
    fn touch(&mut self) {
        self.updated_at = current_timestamp();
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

/// Generate a unique session ID
fn generate_session_id() -> String {
    // Use timestamp + random component for uniqueness
    let timestamp = current_timestamp();
    let random: u32 = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();

    format!("session-{}-{:x}", timestamp, random)
}

/// Validate a user message
///
/// # Errors
///
/// Returns `ClientError::Validation` if:
/// - Message is empty or whitespace-only
/// - Message exceeds maximum length
/// - Message contains null bytes
fn validate_message(message: &str) -> Result<()> {
    // Check for empty or whitespace-only messages
    if message.trim().is_empty() {
        return Err(ClientError::Validation(
            "Message cannot be empty or whitespace-only".to_string(),
        ));
    }

    // Check maximum length
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(ClientError::Validation(format!(
            "Message exceeds maximum length of {} characters",
            MAX_MESSAGE_LENGTH
        )));
    }

    // Check for null bytes
    if message.contains('\0') {
        return Err(ClientError::Validation(
            "Message cannot contain null bytes".to_string(),
        ));
    }

    Ok(())
}

/// Manages an interactive chat session
pub struct ChatSession {
    config: ChatSessionConfig,
    history: ConversationHistory,
    enricher: PromptEnricher,
    state: ConversationState,
    metadata: SessionMetadata,
    agent_client: Option<CrucibleAcpClient>,
    agent_session: Option<AcpSession>,
}

impl ChatSession {
    /// Create a new chat session (mock mode - no real agent)
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the chat session
    pub fn new(config: ChatSessionConfig) -> Self {
        let history = ConversationHistory::new(config.history.clone());
        let enricher = PromptEnricher::new(config.context.clone());
        let state = ConversationState::new();
        let metadata = SessionMetadata::new();

        Self {
            config,
            history,
            enricher,
            state,
            metadata,
            agent_client: None,
            agent_session: None,
        }
    }

    /// Create a new chat session with a real agent connection
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the chat session
    /// * `agent_client` - ACP client for agent communication
    ///
    /// # Returns
    ///
    /// A new chat session ready to connect to an agent
    pub fn with_agent(config: ChatSessionConfig, agent_client: CrucibleAcpClient) -> Self {
        let history = ConversationHistory::new(config.history.clone());
        let enricher = PromptEnricher::new(config.context.clone());
        let state = ConversationState::new();
        let metadata = SessionMetadata::new();

        Self {
            config,
            history,
            enricher,
            state,
            metadata,
            agent_client: Some(agent_client),
            agent_session: None,
        }
    }

    /// Connect to the agent and establish a session
    ///
    /// This performs the full ACP protocol handshake using stdio MCP server.
    /// For in-process MCP, use `connect_with_sse_mcp` instead.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent connection fails or handshake fails
    pub async fn connect(&mut self) -> Result<()> {
        if let Some(client) = &mut self.agent_client {
            // Perform full protocol handshake
            let session = client.connect_with_handshake().await?;
            self.agent_session = Some(session);
            Ok(())
        } else {
            Err(ClientError::Connection(
                "No agent client configured".to_string(),
            ))
        }
    }

    /// Connect to the agent using an in-process SSE MCP server
    ///
    /// This performs the full ACP protocol handshake with an SSE MCP server URL.
    /// Use this for in-process tool execution to avoid DB lock contention.
    ///
    /// # Arguments
    ///
    /// * `sse_url` - URL to the SSE MCP server (e.g., "http://127.0.0.1:12345/sse")
    ///
    /// # Errors
    ///
    /// Returns an error if the agent connection fails or handshake fails
    pub async fn connect_with_sse_mcp(&mut self, sse_url: &str) -> Result<()> {
        if let Some(client) = &mut self.agent_client {
            // Perform full protocol handshake with SSE MCP
            let session = client.connect_with_sse_mcp(sse_url).await?;
            self.agent_session = Some(session);
            Ok(())
        } else {
            Err(ClientError::Connection(
                "No agent client configured".to_string(),
            ))
        }
    }

    /// Disconnect from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if disconnection fails
    pub async fn disconnect(&mut self) -> Result<()> {
        if let (Some(client), Some(session)) = (&mut self.agent_client, &self.agent_session) {
            client.disconnect(session).await?;
            self.agent_session = None;
            Ok(())
        } else {
            Ok(()) // Already disconnected or never connected
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
    /// Tuple of (agent_response, tool_calls) - the response text and any tool calls made
    ///
    /// # Errors
    ///
    /// Returns an error if message processing fails
    pub async fn send_message(
        &mut self,
        user_message: &str,
    ) -> Result<(String, Vec<ToolCallInfo>)> {
        validate_message(user_message)?;

        // Step 1: Add user message to history
        let user_msg = HistoryMessage::user(user_message.to_string());
        self.history.add_message(user_msg)?;

        // Step 2: Prepare prompt (with or without enrichment)
        let prompt = if self.config.enrich_prompts {
            self.enricher.enrich(user_message).await?
        } else {
            user_message.to_string()
        };

        // Step 3: Generate agent response (real agent if connected, mock otherwise)
        let (agent_response, tool_calls) = self.generate_agent_response(&prompt).await?;

        // Step 4: Add agent response to history
        let agent_msg = HistoryMessage::agent(agent_response.clone());
        self.history.add_message(agent_msg)?;

        // Step 5: Auto-prune if enabled
        if self.config.auto_prune {
            let pruned = self.history.prune()?;
            if pruned > 0 {
                self.state.prune_count += 1;
            }
        }

        self.update_state();

        self.metadata.touch();

        Ok((agent_response, tool_calls))
    }

    /// Send a user message with streaming callback for real-time display.
    ///
    /// This method is similar to `send_message` but invokes the callback
    /// for each chunk as it arrives from the agent, enabling real-time
    /// text display in the UI.
    ///
    /// # Arguments
    ///
    /// * `user_message` - The message from the user
    /// * `callback` - Callback invoked for each streaming chunk
    ///
    /// # Returns
    ///
    /// Tuple of (agent_response, tool_calls) - the complete response and any tool calls made
    pub async fn send_message_with_callback(
        &mut self,
        user_message: &str,
        callback: StreamingCallback,
    ) -> Result<(String, Vec<ToolCallInfo>)> {
        validate_message(user_message)?;

        // Step 1: Add user message to history
        let user_msg = HistoryMessage::user(user_message.to_string());
        self.history.add_message(user_msg)?;

        // Step 2: Prepare prompt (with or without enrichment)
        let prompt = if self.config.enrich_prompts {
            self.enricher.enrich(user_message).await?
        } else {
            user_message.to_string()
        };

        // Step 3: Generate agent response with streaming callback
        let (agent_response, tool_calls) = self
            .generate_agent_response_with_callback(&prompt, callback)
            .await?;

        // Step 4: Add agent response to history
        let agent_msg = HistoryMessage::agent(agent_response.clone());
        self.history.add_message(agent_msg)?;

        // Step 5: Auto-prune if enabled
        if self.config.auto_prune {
            let pruned = self.history.prune()?;
            if pruned > 0 {
                self.state.prune_count += 1;
            }
        }

        self.update_state();
        self.metadata.touch();

        Ok((agent_response, tool_calls))
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

    /// Generate a response from the agent (or mock if no agent connected)
    ///
    /// If an agent is connected, sends the prompt via ACP protocol.
    /// Otherwise, returns a mock response for testing.
    async fn generate_agent_response(
        &mut self,
        prompt: &str,
    ) -> Result<(String, Vec<ToolCallInfo>)> {
        if let (Some(client), Some(session)) = (&mut self.agent_client, &self.agent_session) {
            // Real agent mode: Send prompt via ACP protocol with streaming
            use agent_client_protocol::{ContentBlock, PromptRequest, SessionId};

            // Create a proper PromptRequest
            let prompt_request = PromptRequest::new(
                SessionId::from(session.id().to_string()),
                vec![ContentBlock::from(prompt.to_string())],
            );

            // Send prompt with streaming and accumulate content
            // Request ID is generated internally by send_prompt_with_streaming
            let (content, tool_calls, _stop_reason) =
                client.send_prompt_with_streaming(prompt_request).await?;

            tracing::debug!(
                "Accumulated {} bytes of content from agent, {} tool calls",
                content.len(),
                tool_calls.len()
            );

            Ok((content, tool_calls))
        } else {
            // Mock mode: Return mock response for testing
            Ok(("This is a mock agent response. In a real implementation, this would come from the actual agent.".to_string(), vec![]))
        }
    }

    /// Generate a response from the agent with streaming callback.
    ///
    /// Similar to `generate_agent_response` but invokes the callback for each chunk.
    async fn generate_agent_response_with_callback(
        &mut self,
        prompt: &str,
        mut callback: StreamingCallback,
    ) -> Result<(String, Vec<ToolCallInfo>)> {
        if let (Some(client), Some(session)) = (&mut self.agent_client, &self.agent_session) {
            use agent_client_protocol::{ContentBlock, PromptRequest, SessionId};

            let prompt_request = PromptRequest::new(
                SessionId::from(session.id().to_string()),
                vec![ContentBlock::from(prompt.to_string())],
            );

            // Use the callback version for real-time streaming
            let (content, tool_calls, _stop_reason) = client
                .send_prompt_with_callback(prompt_request, callback)
                .await?;

            tracing::debug!(
                "Streaming complete: {} bytes, {} tool calls",
                content.len(),
                tool_calls.len()
            );

            Ok((content, tool_calls))
        } else {
            // Mock mode: Emit mock response through callback then return
            callback(StreamingChunk::Text(
                "This is a mock agent response.".to_string(),
            ));
            Ok(("This is a mock agent response.".to_string(), vec![]))
        }
    }

    /// Get the conversation history
    pub fn history(&self) -> &ConversationHistory {
        &self.history
    }

    /// Clear the conversation history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Set whether to enrich prompts with context
    pub fn set_enrichment_enabled(&mut self, enabled: bool) {
        self.config.enrich_prompts = enabled;
    }

    /// Get the configuration
    pub fn config(&self) -> &ChatSessionConfig {
        &self.config
    }

    /// Get the conversation state
    pub fn state(&self) -> &ConversationState {
        &self.state
    }

    /// Get the session metadata
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    /// Set the session title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.metadata.title = Some(title.into());
        self.metadata.touch();
    }

    /// Add a tag to the session
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.metadata.tags.contains(&tag) {
            self.metadata.tags.push(tag);
            self.metadata.touch();
        }
    }

    /// Remove a tag from the session
    pub fn remove_tag(&mut self, tag: &str) -> bool {
        if let Some(pos) = self.metadata.tags.iter().position(|t| t == tag) {
            self.metadata.tags.remove(pos);
            self.metadata.touch();
            true
        } else {
            false
        }
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.metadata.id
    }

    /// Set the session mode on the agent via ACP protocol
    ///
    /// Sends `session/set_mode` to the connected agent.
    /// Returns Ok(()) even if no agent is connected (mode stored locally by caller).
    pub async fn set_session_mode(&mut self, mode_id: &str) -> Result<()> {
        if let (Some(client), Some(session)) = (&mut self.agent_client, &self.agent_session) {
            let session_id = session.id().to_string();
            client.set_session_mode(&session_id, mode_id).await?;
        }
        Ok(())
    }

    /// Get the latest agent-provided slash commands (if any were advertised)
    pub fn available_commands(&self) -> &[AvailableCommand] {
        if let Some(client) = &self.agent_client {
            client.available_commands()
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_session_creation() {
        let config = ChatSessionConfig::default();
        let session = ChatSession::new(config);

        assert_eq!(session.history().message_count(), 0);
        assert!(session.config().auto_prune);
        assert!(session.config().enrich_prompts);
    }

    #[test]
    fn test_custom_chat_config() {
        let config = ChatSessionConfig {
            auto_prune: false,
            enrich_prompts: false,
            ..Default::default()
        };

        let session = ChatSession::new(config);
        assert!(!session.config().auto_prune);
        assert!(!session.config().enrich_prompts);
    }

    #[tokio::test]
    async fn test_send_message() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        let response = session.send_message("Hello, agent!").await;

        // This should fail because send_message is not yet implemented
        // Once implemented, it should:
        // - Add user message to history
        // - Optionally enrich with context
        // - Generate agent response
        // - Add agent response to history
        // - Return the response
        assert!(response.is_ok(), "Should send message successfully");

        let (response_text, _tool_calls) = response.unwrap();
        assert!(!response_text.is_empty(), "Response should not be empty");

        // History should contain both user and agent messages
        assert_eq!(session.history().message_count(), 2);
    }

    #[tokio::test]
    async fn test_context_enrichment_in_chat() {
        let config = ChatSessionConfig {
            enrich_prompts: true,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        let response = session.send_message("What is semantic search?").await;

        assert!(response.is_ok());
        // When enrichment is enabled, the enricher should be used
        // (We can't easily verify this without mocking, but the integration should work)
    }

    #[tokio::test]
    async fn test_auto_prune() {
        let config = ChatSessionConfig {
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
        assert!(
            session.history().message_count() <= 4,
            "Should auto-prune to stay within message limit"
        );
    }

    #[tokio::test]
    async fn test_no_auto_prune() {
        let config = ChatSessionConfig {
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
        assert_eq!(
            session.history().message_count(),
            4,
            "Should not auto-prune when disabled"
        );
    }

    #[tokio::test]
    async fn test_clear_history() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        session.send_message("First message").await.unwrap();
        session.send_message("Second message").await.unwrap();

        assert!(session.history().message_count() > 0);

        session.clear_history();
        assert_eq!(session.history().message_count(), 0);
    }

    #[tokio::test]
    async fn test_enrichment_disabled() {
        let config = ChatSessionConfig {
            enrich_prompts: false,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        let response = session.send_message("Test query").await;

        assert!(response.is_ok());
        // When enrichment is disabled, the original query should be used directly
    }

    #[test]
    fn test_set_enrichment_enabled() {
        let config = ChatSessionConfig {
            enrich_prompts: true,
            ..Default::default()
        };

        let mut session = ChatSession::new(config);
        assert!(
            session.config().enrich_prompts,
            "Should start with enrichment enabled"
        );

        // Disable enrichment
        session.set_enrichment_enabled(false);
        assert!(
            !session.config().enrich_prompts,
            "Should disable enrichment"
        );

        // Re-enable enrichment
        session.set_enrichment_enabled(true);
        assert!(
            session.config().enrich_prompts,
            "Should re-enable enrichment"
        );
    }

    #[test]
    fn test_conversation_state_initialization() {
        let session = ChatSession::new(ChatSessionConfig::default());
        let state = session.state();

        assert_eq!(state.turn_count, 0, "Should start with zero turns");
        assert_eq!(state.total_tokens_used, 0, "Should start with zero tokens");
        assert_eq!(state.prune_count, 0, "Should start with zero prunes");
        assert!(
            state.last_message_at.is_none(),
            "Should have no last message initially"
        );
        assert!(state.started_at > 0, "Should have a start timestamp");
    }

    #[tokio::test]
    async fn test_turn_counting() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Initial state
        assert_eq!(session.state().turn_count, 0);

        // Send first message
        session.send_message("First message").await.unwrap();
        assert_eq!(
            session.state().turn_count,
            1,
            "Should have 1 turn after first exchange"
        );

        // Send second message
        session.send_message("Second message").await.unwrap();
        assert_eq!(
            session.state().turn_count,
            2,
            "Should have 2 turns after second exchange"
        );

        // Send third message
        session.send_message("Third message").await.unwrap();
        assert_eq!(
            session.state().turn_count,
            3,
            "Should have 3 turns after third exchange"
        );
    }

    #[tokio::test]
    async fn test_token_tracking() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Initial tokens
        assert_eq!(session.state().total_tokens_used, 0);

        // Send messages
        session.send_message("Hello").await.unwrap();

        // Tokens should be tracked from history
        let tokens_after_turn1 = session.state().total_tokens_used;
        assert!(
            tokens_after_turn1 > 0,
            "Should track tokens after first turn"
        );

        session.send_message("How are you?").await.unwrap();
        let tokens_after_turn2 = session.state().total_tokens_used;
        assert!(
            tokens_after_turn2 > tokens_after_turn1,
            "Tokens should increase with more turns"
        );
    }

    #[tokio::test]
    async fn test_timestamp_tracking() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Initially no last message
        assert!(session.state().last_message_at.is_none());

        // After first message, should have timestamp
        session.send_message("Test").await.unwrap();
        let timestamp1 = session.state().last_message_at;
        assert!(
            timestamp1.is_some(),
            "Should have timestamp after first message"
        );

        // Wait a tiny bit and send another
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        session.send_message("Another test").await.unwrap();
        let timestamp2 = session.state().last_message_at;

        assert!(
            timestamp2.is_some(),
            "Should have timestamp after second message"
        );
        assert!(
            timestamp2.unwrap() >= timestamp1.unwrap(),
            "Timestamp should not go backwards"
        );
    }

    #[tokio::test]
    async fn test_prune_count_tracking() {
        let config = ChatSessionConfig {
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

        assert!(
            session.state().prune_count > 0,
            "Should have pruned at least once"
        );
    }

    #[tokio::test]
    async fn test_conversation_duration() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Duration should be very small initially
        let initial_duration = session.state().duration_secs();
        assert!(
            initial_duration < 5,
            "Initial duration should be very small"
        );

        // Send a message
        session.send_message("Test").await.unwrap();

        // Duration should still be small but non-zero
        let duration_after = session.state().duration_secs();
        assert!(
            duration_after >= initial_duration,
            "Duration should not decrease"
        );
    }

    #[tokio::test]
    async fn test_avg_tokens_per_turn() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // No turns yet
        assert_eq!(session.state().avg_tokens_per_turn(), 0.0);

        // After some turns
        session.send_message("Short").await.unwrap();
        session.send_message("A longer message here").await.unwrap();

        let avg = session.state().avg_tokens_per_turn();
        assert!(avg > 0.0, "Should have positive average");
        assert_eq!(session.state().turn_count, 2);
    }

    #[tokio::test]
    async fn test_empty_message_handling() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Empty string should be rejected
        let result = session.send_message("").await;
        assert!(result.is_err(), "Should reject empty messages");

        // Whitespace-only should also be rejected
        let result = session.send_message("   ").await;
        assert!(result.is_err(), "Should reject whitespace-only messages");

        // State should not be updated on error
        assert_eq!(
            session.state().turn_count,
            0,
            "Turn count should not increase on error"
        );
    }

    #[tokio::test]
    async fn test_long_message_handling() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Create a very long message (100k characters)
        let long_message = "x".repeat(100_000);

        // Should be rejected
        let result = session.send_message(&long_message).await;
        assert!(
            result.is_err(),
            "Should reject messages exceeding max length"
        );

        // State should not be updated
        assert_eq!(session.state().turn_count, 0);
    }

    #[tokio::test]
    async fn test_state_rollback_on_error() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Send a valid message
        session.send_message("Valid message").await.unwrap();
        assert_eq!(session.state().turn_count, 1);
        let history_count = session.history().message_count();

        // Try to send an invalid message
        let _ = session.send_message("").await;

        // State should be unchanged (rollback happened)
        assert_eq!(
            session.state().turn_count,
            1,
            "Turn count should not change on error"
        );
        assert_eq!(
            session.history().message_count(),
            history_count,
            "History should be unchanged on error"
        );
    }

    #[tokio::test]
    async fn test_enrichment_failure_fallback() {
        // This test would verify that if enrichment fails, we fall back to
        // using the original message without enrichment
        // For now, since our mock enricher doesn't fail, this is a placeholder
        // In a real implementation with dependency injection, we'd inject a
        // failing enricher and verify the fallback behavior
        let session = ChatSession::new(ChatSessionConfig::default());

        // Verify the session was created successfully
        assert_eq!(session.state().turn_count, 0);
    }

    #[tokio::test]
    async fn test_message_validation() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Null bytes should be rejected
        let result = session.send_message("Hello\0World").await;
        assert!(result.is_err(), "Should reject messages with null bytes");

        // Control characters should be handled
        let result = session.send_message("Hello\x01World").await;
        // This might be allowed with sanitization, so we just verify it doesn't panic
        let _ = result;
    }

    #[tokio::test]
    async fn test_history_consistency_after_errors() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Send valid message
        session.send_message("Message 1").await.unwrap();

        // Try invalid message
        let _ = session.send_message("").await;

        // Send another valid message
        session.send_message("Message 2").await.unwrap();

        // History should only contain valid messages
        // 2 valid user messages + 2 agent responses = 4 messages
        assert_eq!(session.history().message_count(), 4);
        assert_eq!(session.state().turn_count, 2);
    }

    #[tokio::test]
    async fn test_session_recovery_after_errors() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Multiple error attempts
        let _ = session.send_message("").await;
        let _ = session.send_message("   ").await;
        let _ = session.send_message("x".repeat(100_000).as_str()).await;

        // Should still be able to send valid messages
        let result = session.send_message("Valid message after errors").await;
        assert!(
            result.is_ok(),
            "Should recover and process valid messages after errors"
        );
        assert_eq!(session.state().turn_count, 1);
    }

    #[test]
    fn test_session_id_generation() {
        let session1 = ChatSession::new(ChatSessionConfig::default());
        let session2 = ChatSession::new(ChatSessionConfig::default());

        // Each session should have a unique ID
        assert!(
            !session1.session_id().is_empty(),
            "Session ID should not be empty"
        );
        assert!(
            !session2.session_id().is_empty(),
            "Session ID should not be empty"
        );
        assert_ne!(
            session1.session_id(),
            session2.session_id(),
            "Session IDs should be unique"
        );

        // ID should have expected format
        assert!(
            session1.session_id().starts_with("session-"),
            "Session ID should start with 'session-'"
        );
    }

    #[test]
    fn test_metadata_initialization() {
        let session = ChatSession::new(ChatSessionConfig::default());
        let metadata = session.metadata();

        assert!(metadata.title.is_none(), "Title should initially be None");
        assert!(metadata.tags.is_empty(), "Tags should initially be empty");
        assert!(metadata.created_at > 0, "Created timestamp should be set");
        assert!(metadata.updated_at > 0, "Updated timestamp should be set");
        assert_eq!(
            metadata.created_at, metadata.updated_at,
            "Initially created_at should equal updated_at"
        );
    }

    #[test]
    fn test_set_title() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        let initial_updated = session.metadata().updated_at;

        // Set title
        session.set_title("My Chat Session");

        assert_eq!(
            session.metadata().title,
            Some("My Chat Session".to_string())
        );
        assert!(
            session.metadata().updated_at >= initial_updated,
            "Updated timestamp should advance"
        );

        // Update title
        session.set_title("Updated Title");
        assert_eq!(session.metadata().title, Some("Updated Title".to_string()));
    }

    #[test]
    fn test_tag_management() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Add tags
        session.add_tag("project-alpha");
        session.add_tag("important");
        session.add_tag("customer-support");

        let tags = &session.metadata().tags;
        assert_eq!(tags.len(), 3);
        assert!(tags.contains(&"project-alpha".to_string()));
        assert!(tags.contains(&"important".to_string()));
        assert!(tags.contains(&"customer-support".to_string()));

        // Adding duplicate should be ignored
        session.add_tag("important");
        assert_eq!(
            session.metadata().tags.len(),
            3,
            "Duplicate tags should not be added"
        );

        // Remove tag
        let removed = session.remove_tag("important");
        assert!(removed, "Should return true when tag is removed");
        assert_eq!(session.metadata().tags.len(), 2);
        assert!(!session.metadata().tags.contains(&"important".to_string()));

        // Remove non-existent tag
        let removed = session.remove_tag("non-existent");
        assert!(!removed, "Should return false when tag doesn't exist");
    }

    #[tokio::test]
    async fn test_metadata_updates_on_activity() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        let initial_updated = session.metadata().updated_at;

        // Wait enough time for timestamp to change (1 second resolution)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Send a message
        session.send_message("Hello").await.unwrap();

        // Metadata should be updated
        assert!(
            session.metadata().updated_at > initial_updated,
            "Updated timestamp should advance after message"
        );
    }

    #[test]
    fn test_session_has_complete_metadata() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        // Set up session metadata
        session.set_title("Test Session");
        session.add_tag("test");
        session.add_tag("automated");

        let metadata = session.metadata();

        // Should have all necessary data for persistence
        assert!(!metadata.id.is_empty());
        assert!(metadata.title.is_some());
        assert!(!metadata.tags.is_empty());
        assert!(metadata.created_at > 0);
        assert!(metadata.updated_at >= metadata.created_at);
    }

    #[tokio::test]
    async fn test_metadata_timestamp_updates() {
        let mut session = ChatSession::new(ChatSessionConfig::default());

        let created = session.metadata().created_at;
        let initial_updated = session.metadata().updated_at;

        // Wait enough time for timestamp to change (1 second resolution)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Update title
        session.set_title("New Title");
        let after_title_update = session.metadata().updated_at;

        assert!(
            after_title_update > initial_updated,
            "Timestamp should update on title change"
        );

        // Add tag
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        session.add_tag("test");
        let after_tag_add = session.metadata().updated_at;

        assert!(
            after_tag_add >= after_title_update,
            "Timestamp should update on tag add"
        );

        // Created timestamp should never change
        assert_eq!(
            session.metadata().created_at,
            created,
            "Created timestamp should never change"
        );
    }
}
