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
//! - `mode_ext`: CLI display extensions for ChatMode
//! - `registry`: Command registry implementation
//! - `slash_registry`: Generic Registry trait implementation for slash commands
//! - `handlers`: Built-in command handlers (exit, mode, search, help)
//! - `commands`: Static command handlers
//! - `display`: Terminal UI formatting
//! - `session`: Interactive session orchestrator

pub mod commands;
pub mod display;
pub mod handlers;
pub mod mode_ext;
pub mod registry;
pub mod session;
pub mod slash_registry;

// Re-export core traits for convenience
pub use crucible_core::traits::chat::{
    AgentHandle, ChatContext, ChatError, ChatMode, ChatResponse, ChatResult, CommandDescriptor,
    CommandHandler, CommandRegistry, SearchResult, ToolCall as ChatToolCall,
};

// Re-export CLI implementations
pub use commands::{Command, CommandParser};
pub use display::{Display, ToolCallDisplay, format_tool_args};
pub use handlers::{ExitHandler, HelpHandler, ModeCycleHandler, ModeHandler, SearchHandler};
pub use mode_ext::ChatModeDisplay;
pub use registry::CliCommandRegistry;
pub use session::{ChatSession, SessionConfig};
pub use slash_registry::{SlashCommand, SlashCommandRegistry, SlashCommandRegistryBuilder};
