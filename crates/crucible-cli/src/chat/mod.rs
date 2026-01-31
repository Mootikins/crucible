//! Chat Framework Module
//!
//! Provides a backend-agnostic chat interface built on crucible-core traits.
//! Supports multiple agent backends: ACP (external agents), internal agents, direct LLM.
//!
//! ## Architecture
//!
//! - **Core Traits** (`crucible-core/src/traits/chat.rs`): Backend-agnostic abstractions
//! - **CLI Implementation** (this module): Terminal-specific UI and orchestration
//!
//! ## Components
//!
//! - `mode_registry`: Dynamic mode management from agent SessionModeState
//! - `slash_registry`: Generic Registry trait implementation for slash commands
//! - `handlers`: Built-in command handlers (exit, mode, search, help)
//! - `context`: CLI chat context implementation
//! - `display`: Terminal UI formatting
//! - `session`: Interactive session orchestrator

pub mod bridge;
pub mod context;
pub mod diff;
pub mod display;
pub mod handlers;
pub mod mode_registry;
pub mod session;
pub mod slash_registry;

// Re-export core traits for convenience

pub use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatContext, ChatError, ChatResponse, ChatResult, ChatToolCall,
    CommandDescriptor, CommandHandler, SearchResult,
};

// Re-export CLI implementations
pub use context::CliChatContext;
pub use diff::DiffRenderer;
pub use display::{format_tool_args, Display, ToolCallDisplay};
pub use handlers::{
    AgentHandler, ExitHandler, HelpHandler, McpInfo, McpServerInfo, ModeCycleHandler, ModeHandler,
    NewHandler, ReplHelpHandler, ReplMcpHandler, ReplPaletteHandler, ReplQuitHandler,
    SearchHandler,
};
pub use mode_registry::{ModeError, ModeRegistry, ModeResult};
pub use session::{ChatSession, ChatSessionConfig};
pub use slash_registry::{SlashCommand, SlashCommandRegistry, SlashCommandRegistryBuilder};
