//! Rig Agent Handle
//!
//! Implements `AgentHandle` using Rig's agent abstractions.
//! This replaces `InternalAgentHandle` with Rig-based agent execution.
//!
//! ## Design Principles
//!
//! - **Stateless agents**: Conversation history is managed externally for multi-agent compatibility
//! - **Streaming-first**: Uses Rig's `stream_prompt()` for real-time responses
//! - **Mode-aware**: Tracks plan/act/auto modes (Rig doesn't have this natively)
//! - **Tool bridging**: Crucible tools are bridged to Rig's tool system

use async_trait::async_trait;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall};
use crucible_core::types::acp::schema::SessionModeState;
use crucible_core::types::mode::default_internal_modes;
use futures::stream::BoxStream;
use futures::StreamExt;
use rig::agent::Agent;
use rig::completion::CompletionModel;
use rig::message::Message;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Rig-based agent handle implementing `AgentHandle` trait.
///
/// This provides a bridge between Rig's agent abstraction and Crucible's
/// `AgentHandle` trait, enabling Rig agents to be used in the CLI and
/// other Crucible infrastructure.
///
/// ## Multi-Agent Compatibility
///
/// Conversation history is stored externally in `chat_history`, allowing:
/// - Session state to persist across agent switches
/// - History to be transferred between agents
/// - Clean separation of agent execution from state management
pub struct RigAgentHandle<M>
where
    M: CompletionModel + 'static,
{
    /// The Rig agent (wrapped in Arc for streaming)
    agent: Arc<Agent<M>>,

    /// Conversation history (external to agent for multi-agent support)
    chat_history: Arc<RwLock<Vec<Message>>>,

    /// Mode state (plan/act/auto) - managed by us, not Rig
    mode_state: SessionModeState,

    /// Current mode ID
    current_mode_id: String,

    /// Maximum tool call depth (prevents infinite loops)
    max_tool_depth: usize,
}

impl<M> RigAgentHandle<M>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: Clone + Send + Sync + rig::completion::GetTokenUsage,
{
    /// Create a new Rig agent handle
    ///
    /// # Arguments
    ///
    /// * `agent` - The Rig agent to wrap
    pub fn new(agent: Agent<M>) -> Self {
        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            agent: Arc::new(agent),
            chat_history: Arc::new(RwLock::new(Vec::new())),
            mode_state,
            current_mode_id,
            max_tool_depth: 10,
        }
    }

    /// Set the maximum tool call depth
    ///
    /// This prevents infinite loops when the LLM keeps requesting tool calls.
    pub fn with_max_tool_depth(mut self, depth: usize) -> Self {
        self.max_tool_depth = depth;
        self
    }

    /// Set initial conversation history
    ///
    /// Useful for resuming sessions or multi-agent handoff.
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.chat_history = Arc::new(RwLock::new(history));
        self
    }

    /// Get a copy of the current conversation history
    ///
    /// Useful for session persistence or agent handoff.
    pub async fn get_history(&self) -> Vec<Message> {
        self.chat_history.read().await.clone()
    }

    /// Clear conversation history
    pub async fn clear_history(&self) {
        self.chat_history.write().await.clear();
    }
}

#[async_trait]
impl<M> AgentHandle for RigAgentHandle<M>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: Clone + Send + Sync + Unpin + rig::completion::GetTokenUsage,
{
    fn send_message_stream(&mut self, message: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
        use rig::agent::MultiTurnStreamItem;
        use rig::streaming::{StreamedAssistantContent, StreamingPrompt};

        let agent = Arc::clone(&self.agent);
        let history = Arc::clone(&self.chat_history);
        let max_depth = self.max_tool_depth;

        Box::pin(async_stream::stream! {
            // Get current history
            let current_history = history.read().await.clone();

            // Add user message to history
            {
                let mut h = history.write().await;
                h.push(Message::user(&message));
            }

            // Create streaming request with history
            // stream_prompt().await returns the stream directly
            let mut stream = agent
                .stream_prompt(&message)
                .multi_turn(max_depth)
                .with_history(current_history)
                .await;

            let mut accumulated_text = String::new();
            let mut tool_calls: Vec<ChatToolCall> = Vec::new();

            while let Some(item) = stream.next().await {
                match item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => {
                        match content {
                            StreamedAssistantContent::Text(text) => {
                                accumulated_text.push_str(&text.text);
                                yield Ok(ChatChunk {
                                    delta: text.text,
                                    done: false,
                                    tool_calls: None,
                                });
                            }
                            StreamedAssistantContent::ToolCall(tc) => {
                                // Accumulate tool call info
                                tool_calls.push(ChatToolCall {
                                    name: tc.function.name.clone(),
                                    arguments: Some(tc.function.arguments.clone()),
                                    id: tc.call_id.clone(),
                                });

                                // Emit tool call as delta for visibility
                                yield Ok(ChatChunk {
                                    delta: format!("\n[Tool: {}]\n", tc.function.name),
                                    done: false,
                                    tool_calls: None,
                                });
                            }
                            StreamedAssistantContent::Reasoning(r) => {
                                // Emit reasoning as delta
                                let reasoning_text = r.reasoning.join("");
                                if !reasoning_text.is_empty() {
                                    yield Ok(ChatChunk {
                                        delta: format!("<thinking>{}</thinking>", reasoning_text),
                                        done: false,
                                        tool_calls: None,
                                    });
                                }
                            }
                            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                                yield Ok(ChatChunk {
                                    delta: reasoning,
                                    done: false,
                                    tool_calls: None,
                                });
                            }
                            StreamedAssistantContent::ToolCallDelta { .. } => {
                                // Ignore deltas, we get full tool call above
                            }
                            StreamedAssistantContent::Final(_) => {
                                // Final response marker, will handle in FinalResponse
                            }
                        }
                    }
                    Ok(MultiTurnStreamItem::StreamUserItem(_)) => {
                        // Tool results - Rig handles internally
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
                        // Add assistant response to history
                        {
                            let mut h = history.write().await;
                            h.push(Message::assistant(final_resp.response()));
                        }

                        // Emit final chunk
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls.clone())
                            },
                        });
                    }
                    Err(e) => {
                        yield Err(ChatError::Communication(format!("Stream error: {}", e)));
                        return;
                    }
                    // Catch-all for future MultiTurnStreamItem variants (non-exhaustive enum)
                    Ok(_) => {
                        // Ignore unknown variants for forward compatibility
                    }
                }
            }
        })
    }

    fn is_connected(&self) -> bool {
        // Rig agents are always "connected" (no persistent connection)
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    fn get_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        // Validate mode exists in our advertised modes
        let exists = self
            .mode_state
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == mode_id);

        if !exists {
            return Err(ChatError::InvalidMode(format!(
                "Unknown mode '{}'. Available: {:?}",
                mode_id,
                self.mode_state
                    .available_modes
                    .iter()
                    .map(|m| m.id.0.as_ref())
                    .collect::<Vec<_>>()
            )));
        }

        self.current_mode_id = mode_id.to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::client::{CompletionClient, Nothing};
    use rig::providers::ollama;

    fn create_test_agent() -> Agent<ollama::CompletionModel> {
        let client = ollama::Client::builder().api_key(Nothing).build().unwrap();

        client
            .agent("llama3.2")
            .preamble("You are a test assistant.")
            .build()
    }

    #[tokio::test]
    async fn test_rig_agent_handle_creation() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent);

        assert!(handle.is_connected());
        assert_eq!(handle.get_mode_id(), "plan");
        assert!(handle.get_modes().is_some());
    }

    #[tokio::test]
    async fn test_rig_agent_handle_mode_switching() {
        let agent = create_test_agent();
        let mut handle = RigAgentHandle::new(agent);

        assert_eq!(handle.get_mode_id(), "plan");

        handle.set_mode_str("act").await.unwrap();
        assert_eq!(handle.get_mode_id(), "act");

        handle.set_mode_str("auto").await.unwrap();
        assert_eq!(handle.get_mode_id(), "auto");

        // Invalid mode should error
        let result = handle.set_mode_str("invalid").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rig_agent_handle_history_management() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent);

        // Initially empty
        assert!(handle.get_history().await.is_empty());

        // Can set initial history
        let agent2 = create_test_agent();
        let handle2 = RigAgentHandle::new(agent2).with_history(vec![Message::user("Hello")]);

        assert_eq!(handle2.get_history().await.len(), 1);
    }

    #[tokio::test]
    async fn test_rig_agent_handle_with_max_depth() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent).with_max_tool_depth(5);

        assert_eq!(handle.max_tool_depth, 5);
    }

    // Integration test requiring running Ollama
    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_rig_agent_handle_streaming() {
        let agent = create_test_agent();
        let mut handle = RigAgentHandle::new(agent);

        let mut stream = handle.send_message_stream("Say hello".to_string());
        let mut chunks = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => chunks.push(chunk),
                Err(e) => panic!("Stream error: {}", e),
            }
        }

        // Should have at least one chunk
        assert!(!chunks.is_empty());

        // Last chunk should be done
        assert!(chunks.last().unwrap().done);

        // History should be updated
        let history = handle.get_history().await;
        assert_eq!(history.len(), 2); // User + Assistant
    }
}
