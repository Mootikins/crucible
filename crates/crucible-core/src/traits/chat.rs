//\! Chat framework abstraction traits
//\!
//\! Following SOLID principles, this module defines backend-agnostic chat abstractions.
//\!
//\! ## Architecture
//\!
//\! - **AgentHandle**: Runtime handle to an active agent (ACP, internal, direct LLM)
//\! - **CommandHandler**: Trait for implementing slash commands
//\! - **ChatContext**: Execution context for command handlers
//\!
//\! ## Mode Handling
//\!
//\! Modes are now handled via string IDs (e.g., "plan", "act", "auto") with
//\! `SessionModeState` providing the list of available modes from the agent.
//\!
//\! ## Naming Convention
//\!
//\! - **AgentCard**: Static definition (prompt + metadata) - see `agent::types`
//\! - **AgentHandle**: Runtime handle to active agent - this module
//\!
//\! ## Design Principles
//\!
//\! **Dependency Inversion**: Core defines interfaces, implementations live in CLI/agent crates
//\! **Interface Segregation**: Separate traits for distinct capabilities
//\! **Protocol Independence**: Abstracts over ACP, internal agents, direct LLM APIs

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::types::acp::schema::{AvailableCommand, SessionModeState};

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

    #[error("Invalid mode: {0}")]
    InvalidMode(String),

    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Chunk from streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunk {
    pub delta: String,
    pub done: bool,
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

/// Runtime handle to an active agent
#[async_trait]
pub trait AgentHandle: Send + Sync {
    fn send_message_stream(&mut self, message: String)
        -> BoxStream<'static, ChatResult<ChatChunk>>;

    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse> {
        use futures::StreamExt;

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut stream = self.send_message_stream(message.to_string());

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

    fn is_connected(&self) -> bool;

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn on_commands_update(&mut self, _commands: Vec<CommandDescriptor>) -> ChatResult<()> {
        Ok(())
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        None
    }

    fn get_mode_id(&self) -> &str {
        "plan"
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;

    fn get_commands(&self) -> &[AvailableCommand] {
        &[]
    }
}

/// Blanket implementation for boxed trait objects
///
/// This allows `Box<dyn AgentHandle + Send + Sync>` to be used anywhere
/// an `AgentHandle` is expected, enabling factory patterns that return
/// type-erased agents.
#[async_trait]
impl AgentHandle for Box<dyn AgentHandle + Send + Sync> {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        (**self).send_message_stream(message)
    }

    fn is_connected(&self) -> bool {
        (**self).is_connected()
    }

    fn supports_streaming(&self) -> bool {
        (**self).supports_streaming()
    }

    async fn on_commands_update(&mut self, commands: Vec<CommandDescriptor>) -> ChatResult<()> {
        (**self).on_commands_update(commands).await
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        (**self).get_modes()
    }

    fn get_mode_id(&self) -> &str {
        (**self).get_mode_id()
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        (**self).set_mode_str(mode_id).await
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        (**self).get_commands()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub name: String,
    pub arguments: Option<serde_json::Value>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDescriptor {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
    /// Optional secondary selection options advertised by the agent (e.g., model choices)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_options: Vec<CommandOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOption {
    pub label: String,
    pub value: String,
}

#[async_trait]
pub trait CommandHandler: Send + Sync {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()>;
}

#[async_trait]
pub trait ChatContext: Send {
    fn get_mode_id(&self) -> &str;
    fn request_exit(&mut self);
    fn exit_requested(&self) -> bool;
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;
    async fn semantic_search(&self, query: &str, limit: usize) -> ChatResult<Vec<SearchResult>>;
    async fn send_command_to_agent(&mut self, name: &str, args: &str) -> ChatResult<()>;
    fn display_search_results(&self, query: &str, results: &[SearchResult]);
    fn display_help(&self);
    fn display_error(&self, message: &str);
    fn display_info(&self, message: &str);

    /// Switch the agent to a different model (triggers reconnection for ACP agents)
    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported("switch_model".into()))
    }

    /// Get the currently active model (if known)
    fn current_model(&self) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub snippet: String,
    pub similarity: f32,
}

// Mode ID Helper Functions

pub fn is_read_only(mode_id: &str) -> bool {
    mode_id == "plan"
}

pub fn is_auto_approve(mode_id: &str) -> bool {
    mode_id == "auto"
}

pub fn cycle_mode_id(current: &str) -> &'static str {
    match current {
        "plan" => "act",
        "act" => "auto",
        "auto" => "plan",
        _ => "plan",
    }
}

pub fn mode_display_name(mode_id: &str) -> &'static str {
    match mode_id {
        "plan" => "Plan",
        "act" => "Act",
        "auto" => "Auto",
        _ => "Unknown",
    }
}

pub fn mode_icon(mode_id: &str) -> &'static str {
    match mode_id {
        "plan" => "📖",
        "act" => "✏️",
        "auto" => "⚡",
        _ => "●",
    }
}

pub fn mode_description(mode_id: &str) -> &'static str {
    match mode_id {
        "plan" => "read-only",
        "act" => "write-enabled",
        "auto" => "auto-approve",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    struct TestAgent {
        chunks: Vec<String>,
    }

    #[async_trait]
    impl AgentHandle for TestAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let chunks = self.chunks.clone();
            let total = chunks.len();
            Box::pin(futures::stream::iter(chunks.into_iter().enumerate().map(
                move |(i, delta)| {
                    Ok(ChatChunk {
                        delta,
                        done: i == total - 1,
                        tool_calls: None,
                    })
                },
            )))
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_streaming() {
        let mut agent = TestAgent {
            chunks: vec!["Hello".to_string()],
        };
        let mut stream = agent.send_message_stream("test".to_string());
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.delta, "Hello");
    }

    #[test]
    fn test_is_read_only() {
        assert!(is_read_only("plan"));
        assert!(!is_read_only("act"));
    }

    #[test]
    fn test_cycle_mode_id() {
        assert_eq!(cycle_mode_id("plan"), "act");
        assert_eq!(cycle_mode_id("act"), "auto");
        assert_eq!(cycle_mode_id("auto"), "plan");
    }
}
