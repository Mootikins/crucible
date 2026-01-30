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

use super::llm::TokenUsage;
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
    /// Tool calls initiated by the agent
    pub tool_calls: Option<Vec<ChatToolCall>>,
    /// Tool results (completions) from executed tools
    #[serde(default)]
    pub tool_results: Option<Vec<ChatToolResult>>,
    /// Reasoning/thinking content from the model (e.g., Qwen3-thinking, DeepSeek-R1)
    /// Rendered separately from main delta, typically in a collapsible block
    #[serde(default)]
    pub reasoning: Option<String>,
    /// Token usage (typically only present in final chunk when done=true)
    #[serde(default)]
    pub usage: Option<TokenUsage>,
    /// Subagent lifecycle events (spawned, completed, failed)
    #[serde(default)]
    pub subagent_events: Option<Vec<ChatSubagentEvent>>,
}

/// Result from a completed tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolResult {
    /// Tool name that completed
    pub name: String,
    /// Result content (may be truncated for display)
    pub result: String,
    /// Error message if tool failed
    pub error: Option<String>,
    /// LLM-assigned call ID for matching results to the correct tool call
    #[serde(default)]
    pub call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentEventType {
    Spawned,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSubagentEvent {
    pub id: String,
    pub event_type: SubagentEventType,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
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

    /// Clear conversation history
    ///
    /// Resets the agent's conversation context, removing all previous messages.
    /// UI state should be cleared separately.
    fn clear_history(&mut self) {}

    /// Switch to a different model
    ///
    /// This may require recreating the underlying agent/connection.
    /// The implementation should preserve conversation history if possible.
    ///
    /// # Arguments
    /// * `model_id` - The model identifier (e.g., "llama3.2", "gpt-4", "claude-3-opus")
    ///
    /// # Returns
    /// * `Ok(())` if the switch was successful
    /// * `Err(ChatError::NotSupported)` if the agent doesn't support model switching
    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported("switch_model".into()))
    }

    /// Get the current model identifier
    ///
    /// Returns the currently active model, if known.
    fn current_model(&self) -> Option<&str> {
        None
    }

    /// Get available models for this agent (cached)
    ///
    /// Returns a list of model identifiers that can be used with `switch_model`.
    /// Returns empty if model listing is not supported or models haven't been fetched.
    fn available_models(&self) -> Vec<String> {
        Vec::new()
    }

    /// Fetch available models from the provider
    ///
    /// This is an async method that queries the provider for available models.
    /// Use this for lazy loading when models are needed.
    async fn fetch_available_models(&mut self) -> Vec<String> {
        self.available_models()
    }

    /// Set the thinking budget for reasoning models.
    ///
    /// Values: -1 = unlimited, 0 = disabled, >0 = max tokens
    async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_thinking_budget".into()))
    }

    /// Get the current thinking budget.
    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }

    /// Cancel the current agent operation
    ///
    /// Propagates cancellation to the backend (e.g., daemon RPC).
    /// Default is a no-op for agents that don't support remote cancellation.
    async fn cancel(&self) -> ChatResult<()> {
        Ok(())
    }

    /// Set the temperature for response generation.
    ///
    /// Values: 0.0 = deterministic, 1.0 = balanced, 2.0 = maximum randomness
    async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_temperature".into()))
    }

    /// Get the current temperature setting.
    fn get_temperature(&self) -> Option<f64> {
        None
    }

    /// Set the maximum tokens for response generation.
    ///
    /// Values: None = provider default, Some(n) = limit to n tokens
    async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_max_tokens".into()))
    }

    /// Get the current max tokens setting.
    fn get_max_tokens(&self) -> Option<u32> {
        None
    }

    /// Respond to an interaction request
    ///
    /// Sends the user's response to an interaction request (Ask, Permission, etc.)
    /// back to the agent/daemon for processing.
    ///
    /// # Arguments
    /// * `request_id` - The ID of the interaction request being responded to
    /// * `response` - The user's response
    ///
    /// # Returns
    /// * `Ok(())` if the response was sent successfully
    /// * `Err(ChatError::NotSupported)` if the agent doesn't support interactions
    async fn interaction_respond(
        &mut self,
        _request_id: String,
        _response: crate::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("interaction_respond".into()))
    }

    /// Take the interaction event receiver (if available)
    ///
    /// Returns a receiver for out-of-band interaction events. This receiver
    /// delivers `InteractionRequested` events that arrive outside of message
    /// streaming (e.g., from Lua handlers, daemon triggers).
    ///
    /// This method should be called once at startup. Subsequent calls return `None`.
    /// The caller should poll this receiver in their event loop to handle interactions.
    ///
    /// # Returns
    /// * `Some(receiver)` - On first call, if interactions are supported
    /// * `None` - On subsequent calls or if interactions are not supported
    fn take_interaction_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crate::interaction::InteractionEvent>> {
        None
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

    fn clear_history(&mut self) {
        (**self).clear_history()
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        (**self).switch_model(model_id).await
    }

    fn current_model(&self) -> Option<&str> {
        (**self).current_model()
    }

    fn available_models(&self) -> Vec<String> {
        (**self).available_models()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        (**self).fetch_available_models().await
    }

    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()> {
        (**self).set_thinking_budget(budget).await
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        (**self).get_thinking_budget()
    }

    async fn cancel(&self) -> ChatResult<()> {
        (**self).cancel().await
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        (**self).set_temperature(temperature).await
    }

    fn get_temperature(&self) -> Option<f64> {
        (**self).get_temperature()
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        (**self).set_max_tokens(max_tokens).await
    }

    fn get_max_tokens(&self) -> Option<u32> {
        (**self).get_max_tokens()
    }

    async fn interaction_respond(
        &mut self,
        request_id: String,
        response: crate::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        (**self).interaction_respond(request_id, response).await
    }

    fn take_interaction_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crate::interaction::InteractionEvent>> {
        (**self).take_interaction_receiver()
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

/// Command invocation style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CommandKind {
    #[default]
    Slash,
    Repl,
}

impl CommandKind {
    pub fn prefix(&self) -> char {
        match self {
            CommandKind::Slash => '/',
            CommandKind::Repl => ':',
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDescriptor {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_options: Vec<CommandOption>,
    #[serde(default)]
    pub kind: CommandKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgumentSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum CompletionSource {
    #[default]
    None,
    Static(Vec<String>),
    FilePath,
    Directory,
    Note,
    Model,
    McpServer,
    McpTool,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentSpec {
    pub name: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub source: CompletionSource,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub variadic: bool,
}

impl ArgumentSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hint: None,
            source: CompletionSource::None,
            required: false,
            variadic: false,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn source(mut self, source: CompletionSource) -> Self {
        self.source = source;
        self
    }

    pub fn variadic(mut self) -> Self {
        self.variadic = true;
        self
    }
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
        "normal" => "plan",
        "plan" => "auto",
        "auto" => "normal",
        _ => "normal",
    }
}

pub fn mode_display_name(mode_id: &str) -> &'static str {
    match mode_id {
        "normal" => "Normal",
        "plan" => "Plan",
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
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
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
        assert!(!is_read_only("normal"));
    }

    #[test]
    fn test_cycle_mode_id() {
        assert_eq!(cycle_mode_id("normal"), "plan");
        assert_eq!(cycle_mode_id("plan"), "auto");
        assert_eq!(cycle_mode_id("auto"), "normal");
    }
}
