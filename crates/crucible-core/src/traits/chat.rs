//! Chat framework abstraction traits
//!
//! Following SOLID principles, this module defines backend-agnostic chat abstractions.
//!
//! ## Architecture
//!
//! - **AgentHandle**: Runtime handle to an active agent (ACP, internal, direct LLM)
//! - **ChatMode**: Permission model (Plan/Act/AutoApprove)
//! - **CommandRegistry**: Extensible command system (static + dynamic)
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
#[async_trait]
pub trait AgentHandle: Send + Sync {
    /// Send a message to the agent and receive a response
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
    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse>;

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
        false
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
    pub tool_calls: Vec<ToolCall>,
}

/// Tool call made by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
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

/// Command registry trait for managing static and dynamic commands
///
/// Provides a registry for:
/// - **Static commands**: CLI-defined, always available (/plan, /act, /search, etc.)
/// - **Dynamic commands**: Agent-published, can change during session (/web, /test, etc.)
#[async_trait]
pub trait CommandRegistry: Send + Sync {
    /// Register a static command (always available)
    ///
    /// Static commands are defined by the CLI and don't change during a session.
    ///
    /// # Arguments
    ///
    /// * `name` - Command name (without / prefix)
    /// * `handler` - Command handler implementation
    fn register_static(&mut self, name: &str, handler: Box<dyn CommandHandler>);

    /// Update dynamic commands (from agent)
    ///
    /// Dynamic commands are published by agents and can change during a session.
    /// This is called when an agent sends an `available_commands_update` notification.
    ///
    /// # Arguments
    ///
    /// * `commands` - List of commands to register
    fn update_dynamic(&mut self, commands: Vec<CommandDescriptor>);

    /// Execute a command by name
    ///
    /// Routes the command to the appropriate handler (static or dynamic).
    /// If the command is dynamic (agent-provided), forwards it to the agent.
    ///
    /// # Arguments
    ///
    /// * `name` - Command name (without / prefix)
    /// * `args` - Command arguments (text after command name)
    /// * `ctx` - Execution context
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on successful execution.
    ///
    /// # Errors
    ///
    /// - `ChatError::UnknownCommand` - Command not found
    /// - `ChatError::CommandFailed` - Execution failed
    async fn execute(&self, name: &str, args: &str, ctx: &mut dyn ChatContext)
        -> ChatResult<()>;

    /// List all available commands (static + dynamic)
    fn list_commands(&self) -> Vec<CommandDescriptor>;
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
    async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> ChatResult<Vec<SearchResult>>;

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
}
