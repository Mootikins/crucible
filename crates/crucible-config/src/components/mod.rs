//! Essential configuration components for Crucible
//!
//! Simple, focused configuration for the core components that actually need it.

pub mod acp;
pub mod chat;
pub mod cli;
pub mod discovery;
pub mod embedding;
pub mod gateway;
pub mod hooks;
pub mod llm;

// New unified provider configuration
pub mod backend;
pub mod provider;
pub mod providers;

// Re-export essential component types
pub use acp::*;
pub use chat::*;
pub use cli::*;
pub use discovery::*;
pub use embedding::*;
pub use gateway::*;
pub use hooks::*;
pub use llm::*;

// Re-export unified provider types
pub use backend::BackendType;
pub use provider::{ModelConfig, ProviderConfig};
pub use providers::ProvidersConfig;
