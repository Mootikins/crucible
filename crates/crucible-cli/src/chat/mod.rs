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
//! - `slash_registry`: Generic Registry trait implementation for slash commands
//! - `handlers`: Built-in command handlers (exit, mode, search, help)
//! - `context`: CLI chat context implementation
//! - `display`: Terminal UI formatting
//! - `session`: Interactive session orchestrator

pub mod context;
pub mod diff;
pub mod display;
pub mod handlers;
pub mod mode_ext;
pub mod session;
pub mod slash_registry;

// Re-export core traits for convenience
pub use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatContext, ChatError, ChatMode, ChatResponse, ChatResult,
    ChatToolCall, CommandDescriptor, CommandHandler, SearchResult,
};

// Re-export CLI implementations
pub use context::CliChatContext;
pub use diff::DiffRenderer;
pub use display::{format_tool_args, Display, ToolCallDisplay};
pub use handlers::{ExitHandler, HelpHandler, ModeCycleHandler, ModeHandler, SearchHandler};
pub use mode_ext::ChatModeDisplay;
pub use session::{ChatSession, SessionConfig};
pub use slash_registry::{SlashCommand, SlashCommandRegistry, SlashCommandRegistryBuilder};
