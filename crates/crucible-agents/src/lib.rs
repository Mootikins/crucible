//! Crucible Internal Agent System
//!
//! Provides internal agent implementations that use direct LLM API calls
//! instead of external ACP agents. Enables chat with local LLMs (Ollama)
//! and API providers (OpenAI, Anthropic).
//!
//! ## Key Components
//!
//! - `InternalAgentHandle`: Implements `AgentHandle` using `TextGenerationProvider`
//! - `SlidingWindowContext`: Token-budget-aware conversation history
//! - `LayeredPromptBuilder`: System prompt assembly from multiple sources
//! - `TokenBudget`: Token estimation and drift correction

pub mod context;
pub mod handle;
pub mod prompt;
pub mod token;

// Re-exports
pub use context::SlidingWindowContext;
pub use handle::{AgentHandle, InternalAgentHandle, Message, Role};
pub use prompt::LayeredPromptBuilder;
pub use token::TokenBudget;
