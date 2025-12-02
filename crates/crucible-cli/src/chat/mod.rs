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
//! - `commands`: Static command handlers
//! - `display`: Terminal UI formatting
//! - `session`: Interactive session orchestrator

pub mod mode_ext;
pub mod registry;

// Re-export core traits for convenience
pub use crucible_core::traits::chat::{
    ChatAgent, ChatContext, ChatError, ChatMode, ChatResponse, ChatResult, CommandDescriptor,
    CommandHandler, CommandRegistry, SearchResult, ToolCall as ChatToolCall,
};

// Re-export CLI implementations
pub use mode_ext::ChatModeDisplay;
pub use registry::CliCommandRegistry;
