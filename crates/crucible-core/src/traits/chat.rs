//! Chat framework abstraction traits
//!
//! Following SOLID principles, this module defines backend-agnostic chat abstractions.
//!
//! ## Architecture
//!
//! - **AgentHandle**: Runtime handle to an active agent (ACP, internal, direct LLM)
//! - **ChatMode**: Permission model (Plan/Act/AutoApprove)
//! - **CommandHandler**: Trait for implementing slash commands
//! - **ChatContext**: Execution context for command handlers
//!
//! ## Naming Convention
//!
//! - **AgentCard**: Static definition (prompt + metadata) - see `agent::types`
//! - **AgentHandle**: Runtime handle to active agent - this module
//!
//! ## Design Principles
//!
//! **Dependency Inversion**: Core defines interfaces, implementations live in CLI/agent crates
//! **Interface Segregation**: Separate traits for distinct capabilities
//! **Protocol Independence**: Abstracts over ACP, internal agents, direct LLM APIs

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

/// Result type for chat operations
pub type ChatResult<T> = Result<T, ChatError>;

/// Chat operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ChatError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("Mode change error: {0}")]
    ModeChange(String),

    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Agent not available: {0}")]
    AgentUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Chat mode - determines agent permissions
///
/// Modes control what operations an agent can perform:
/// - **Plan**: Read-only mode, agent cannot modify files or state
/// - **Act**: Write-enabled mode, agent can modify files (may require confirmation)
/// - **AutoApprove**: Write-enabled with auto-confirmation of changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMode {
    /// Read-only mode - agent cannot modify files
    Plan,
    /// Write-enabled mode - agent can modify files
    Act,
    /// Auto-approve mode - modifications applied automatically
    AutoApprove,
}

impl ChatMode {
    /// Cycle to the next mode (Plan -> Act -> AutoApprove -> Plan)
    pub fn cycle_next(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::AutoApprove,
            ChatMode::AutoApprove => ChatMode::Plan,
        }
    }

    /// Toggle between Plan and Act (legacy behavior)
    pub fn toggle(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::Plan,
            ChatMode::AutoApprove => ChatMode::Plan,
        }
    }

    /// Check if this mode allows writes
    pub fn is_read_only(&self) -> bool {
        matches!(self, ChatMode::Plan)
    }

    /// Check if this mode auto-approves operations
    pub fn is_auto_approve(&self) -> bool {
        matches!(self, ChatMode::AutoApprove)
    }
}

/// Chunk from streaming response
///
/// Represents an incremental piece of content from a streaming chat response.
/// Chunks are emitted as the agent generates its response, allowing for
/// real-time display and processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunk {
    /// Incremental text content
    pub delta: String,

    /// True when this is the final chunk
    pub done: bool,

    /// Tool calls (populated in final chunk if any)
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

/// Runtime handle to an active agent
///
/// This trait defines the interface for any chat backend:
/// - External agents via ACP (claude-code, gemini-cli, etc.)
/// - Internal agents (direct LLM API integration)
/// - Custom agent implementations
///
/// Handles are created when spawning an agent and dropped when the task completes.
/// This follows the systems programming convention of handles as runtime references.
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync for concurrent usage across async boundaries.
///
/// ## Streaming vs Non-streaming
///
/// The trait now defaults to streaming via `send_message_stream`. The convenience
/// method `send_message` provides a default implementation that collects the stream.
/// Implementations should override `send_message_stream` as the primary method.
#[async_trait]
pub trait AgentHandle: Send + Sync {
    /// Stream response chunks (primary method for streaming)
    ///
    /// # Arguments
    ///
    /// * `message` - User message to send to agent
    ///
    /// # Returns
    ///
    /// Returns a stream of `ChatChunk` values. The stream yields incremental
    /// content deltas and completes with a chunk where `done = true`.
    ///
    /// # Errors
    ///
    /// - `ChatError::Communication` - Failed to send/receive message
    /// - `ChatError::AgentUnavailable` - Agent not connected
    fn send_message_stream<'a>(
        &'a mut self,
        message: &'a str,
    ) -> BoxStream<'a, ChatResult<ChatChunk>>;

    /// Send a message to the agent and receive a response
    ///
    /// Convenience wrapper that collects the entire stream into a single response.
    /// Default implementation collects `send_message_stream`.
    ///
    /// # Arguments
    ///
    /// * `message` - User message to send to agent
    ///
    /// # Returns
    ///
    /// Returns the agent's response including content and any tool calls.
    ///
    /// # Errors
    ///
    /// - `ChatError::Communication` - Failed to send/receive message
    /// - `ChatError::AgentUnavailable` - Agent not connected
    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse> {
        use futures::StreamExt;

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut stream = self.send_message_stream(message);

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            content.push_str(&chunk.delta);
            if let Some(calls) = chunk.tool_calls {
                tool_calls.extend(calls);
            }
        }

        Ok(ChatResponse {
            content,
            tool_calls,
        })
    }

    /// Change the agent's operating mode
    ///
    /// Modes control permissions:
    /// - Plan: Read-only
    /// - Act: Write-enabled
    /// - AutoApprove: Auto-confirm writes
    ///
    /// # Arguments
    ///
    /// * `mode` - New mode to set
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success, or an error if mode change failed.
    ///
    /// # Errors
    ///
    /// - `ChatError::ModeChange` - Failed to change mode
    async fn set_mode(&mut self, mode: ChatMode) -> ChatResult<()>;

    /// Check if the agent is currently connected
    fn is_connected(&self) -> bool;

    /// Check if the agent supports streaming responses
    fn supports_streaming(&self) -> bool {
        true // Now default true since stream is the primary method
    }

    /// Handle command updates from agent (optional)
    ///
    /// ACP agents can publish available commands dynamically via `available_commands_update`
    /// notifications. This method is called when the agent advertises new commands.
    ///
    /// # Arguments
    ///
    /// * `commands` - List of commands published by the agent
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success. Default implementation ignores updates.
    async fn on_commands_update(&mut self, _commands: Vec<CommandDescriptor>) -> ChatResult<()> {
        Ok(()) // Default: ignore command updates
    }
}

/// Response from a chat agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Text content of the response
    pub content: String,

    /// Tool calls made by the agent (if any)
    pub tool_calls: Vec<ChatToolCall>,
}

/// Tool call made by an agent (chat layer)
///
/// This is distinct from `llm::ToolCall` which is the wire format for LLM APIs.
/// `ChatToolCall` is the simplified format used in the chat/agent interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    /// Tool name
    pub name: String,

    /// Tool arguments as JSON
    pub arguments: Option<serde_json::Value>,

    /// Tool call identifier (optional)
    pub id: Option<String>,
}

/// Command descriptor (from ACP available_commands_update or similar)
///
/// Represents a slash command that can be invoked in the chat interface.
/// Commands can be:
/// - **Static**: Defined by the CLI (e.g., /plan, /act, /search)
/// - **Dynamic**: Published by agents at runtime (e.g., /web, /test)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDescriptor {
    /// Command name (without / prefix)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Input hint for when input hasn't been provided (optional)
    pub input_hint: Option<String>,
}

/// Command handler trait
///
/// Implement this trait to handle slash commands.
/// Handlers are stateless and receive a ChatContext for accessing state.
#[async_trait]
pub trait CommandHandler: Send + Sync {
    /// Execute the command
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments (text after command name)
    /// * `ctx` - Execution context providing access to chat state
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success.
    ///
    /// # Errors
    ///
    /// - `ChatError::CommandFailed` - Execution failed
    /// - `ChatError::InvalidInput` - Invalid arguments
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()>;
}

/// Chat context for command execution
///
/// Provides command handlers with access to chat state and operations.
/// This trait is implemented by the chat session/orchestrator.
#[async_trait]
pub trait ChatContext: Send {
    /// Get the current chat mode
    fn get_mode(&self) -> ChatMode;

    /// Request the chat session to exit
    fn request_exit(&mut self);

    /// Check if exit has been requested
    fn exit_requested(&self) -> bool;

    /// Set the chat mode
    ///
    /// # Arguments
    ///
    /// * `mode` - New mode to set
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success.
    ///
    /// # Errors
    ///
    /// - `ChatError::ModeChange` - Failed to change mode
    async fn set_mode(&mut self, mode: ChatMode) -> ChatResult<()>;

    /// Perform semantic search on the knowledge base
    ///
    /// # Arguments
    ///
    /// * `query` - Search query
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    ///
    /// Returns a list of search results.
    async fn semantic_search(&self, query: &str, limit: usize) -> ChatResult<Vec<SearchResult>>;

    /// Send a command to the agent
    ///
    /// Used for dynamic (agent-provided) commands. The command is forwarded
    /// to the agent as a regular user message with the command syntax.
    ///
    /// # Arguments
    ///
    /// * `name` - Command name
    /// * `args` - Command arguments
    ///
    /// # Returns
    ///
    /// Returns Ok(()) after the command has been sent to the agent.
    async fn send_command_to_agent(&mut self, name: &str, args: &str) -> ChatResult<()>;

    /// Display search results
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `results` - Search results to display
    fn display_search_results(&self, query: &str, results: &[SearchResult]);

    /// Display help information
    fn display_help(&self);

    /// Display error message
    ///
    /// # Arguments
    ///
    /// * `message` - Error message to display
    fn display_error(&self, message: &str);

    /// Display informational message
    ///
    /// # Arguments
    ///
    /// * `message` - Info message to display
    fn display_info(&self, message: &str);
}

/// Search result from semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document/note title
    pub title: String,

    /// Content snippet
    pub snippet: String,

    /// Similarity score (0.0 to 1.0)
    pub similarity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[test]
    fn test_chat_mode_cycle() {
        let plan = ChatMode::Plan;
        assert_eq!(plan.cycle_next(), ChatMode::Act);

        let act = ChatMode::Act;
        assert_eq!(act.cycle_next(), ChatMode::AutoApprove);

        let auto = ChatMode::AutoApprove;
        assert_eq!(auto.cycle_next(), ChatMode::Plan);
    }

    #[test]
    fn test_chat_mode_toggle() {
        let plan = ChatMode::Plan;
        assert_eq!(plan.toggle(), ChatMode::Act);

        let act = ChatMode::Act;
        assert_eq!(act.toggle(), ChatMode::Plan);

        let auto = ChatMode::AutoApprove;
        assert_eq!(auto.toggle(), ChatMode::Plan);
    }

    #[test]
    fn test_chat_mode_read_only() {
        assert!(ChatMode::Plan.is_read_only());
        assert!(!ChatMode::Act.is_read_only());
        assert!(!ChatMode::AutoApprove.is_read_only());
    }

    #[test]
    fn test_chat_mode_auto_approve() {
        assert!(!ChatMode::Plan.is_auto_approve());
        assert!(!ChatMode::Act.is_auto_approve());
        assert!(ChatMode::AutoApprove.is_auto_approve());
    }

    #[test]
    fn test_chat_error_clone_serialize() {
        let err = ChatError::Connection("test error".to_string());
        let cloned = err.clone();
        assert_eq!(format!("{}", err), format!("{}", cloned));

        let json = serde_json::to_string(&err).unwrap();
        let deserialized: ChatError = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{}", err), format!("{}", deserialized));
    }

    #[test]
    fn test_chat_chunk_creation() {
        let chunk = ChatChunk {
            delta: "test content".to_string(),
            done: false,
            tool_calls: None,
        };
        assert_eq!(chunk.delta, "test content");
        assert!(!chunk.done);
        assert!(chunk.tool_calls.is_none());
    }

    #[test]
    fn test_chat_chunk_with_tool_calls() {
        let tool_call = ChatToolCall {
            name: "test_tool".to_string(),
            arguments: Some(serde_json::json!({"key": "value"})),
            id: Some("call-123".to_string()),
        };

        let chunk = ChatChunk {
            delta: "final content".to_string(),
            done: true,
            tool_calls: Some(vec![tool_call.clone()]),
        };

        assert_eq!(chunk.delta, "final content");
        assert!(chunk.done);
        assert!(chunk.tool_calls.is_some());
        assert_eq!(chunk.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(chunk.tool_calls.as_ref().unwrap()[0].name, "test_tool");
    }

    #[test]
    fn test_chat_chunk_serialize_deserialize() {
        let chunk = ChatChunk {
            delta: "test".to_string(),
            done: true,
            tool_calls: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: ChatChunk = serde_json::from_str(&json).unwrap();

        assert_eq!(chunk.delta, deserialized.delta);
        assert_eq!(chunk.done, deserialized.done);
    }

    // Test implementation of AgentHandle for testing streaming
    struct TestStreamingAgent {
        chunks: Vec<String>,
    }

    #[async_trait]
    impl AgentHandle for TestStreamingAgent {
        fn send_message_stream<'a>(
            &'a mut self,
            _message: &'a str,
        ) -> BoxStream<'a, ChatResult<ChatChunk>> {
            let chunks = self.chunks.clone();
            let total = chunks.len();
            Box::pin(futures::stream::iter(
                chunks
                    .into_iter()
                    .enumerate()
                    .map(move |(i, delta)| {
                        Ok(ChatChunk {
                            delta,
                            done: i == total - 1,
                            tool_calls: None,
                        })
                    }),
            ))
        }

        async fn set_mode(&mut self, _mode: ChatMode) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_agent_handle_streaming() {
        let mut agent = TestStreamingAgent {
            chunks: vec!["Hello, ".to_string(), "world!".to_string()],
        };

        let mut stream = agent.send_message_stream("test");
        let mut all_chunks = Vec::new();

        while let Some(result) = stream.next().await {
            all_chunks.push(result.unwrap());
        }

        assert_eq!(all_chunks.len(), 2);
        assert_eq!(all_chunks[0].delta, "Hello, ");
        assert!(!all_chunks[0].done);
        assert_eq!(all_chunks[1].delta, "world!");
        assert!(all_chunks[1].done);
    }

    #[tokio::test]
    async fn test_agent_handle_send_message_default() {
        let mut agent = TestStreamingAgent {
            chunks: vec!["Hello, ".to_string(), "world!".to_string()],
        };

        let response = agent.send_message("test").await.unwrap();

        assert_eq!(response.content, "Hello, world!");
        assert!(response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_agent_handle_send_message_with_tool_calls() {
        struct TestAgentWithTools;

        #[async_trait]
        impl AgentHandle for TestAgentWithTools {
            fn send_message_stream<'a>(
                &'a mut self,
                _message: &'a str,
            ) -> BoxStream<'a, ChatResult<ChatChunk>> {
                let tool_call = ChatToolCall {
                    name: "search".to_string(),
                    arguments: Some(serde_json::json!({"query": "test"})),
                    id: Some("call-1".to_string()),
                };

                Box::pin(futures::stream::iter(vec![
                    Ok(ChatChunk {
                        delta: "Searching...".to_string(),
                        done: false,
                        tool_calls: None,
                    }),
                    Ok(ChatChunk {
                        delta: " Done!".to_string(),
                        done: true,
                        tool_calls: Some(vec![tool_call]),
                    }),
                ]))
            }

            async fn set_mode(&mut self, _mode: ChatMode) -> ChatResult<()> {
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }
        }

        let mut agent = TestAgentWithTools;
        let response = agent.send_message("search for something").await.unwrap();

        assert_eq!(response.content, "Searching... Done!");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "search");
    }
}
