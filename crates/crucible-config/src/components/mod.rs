//! Essential configuration components for Crucible
//!
//! Simple, focused configuration for the core components that actually need it.

pub mod acp;
pub mod chat;
pub mod cli;
pub mod context;
pub mod discovery;
pub mod embedding;
pub mod gateway;
pub mod handlers;
pub mod llm;
pub mod storage;

// New unified provider configuration
pub mod backend;
pub mod provider;
pub mod providers;

// Re-export essential component types
pub use acp::*;
pub use chat::*;
pub use cli::*;
pub use context::*;
pub use discovery::*;
pub use embedding::*;
pub use gateway::*;
pub use handlers::*;
pub use llm::*;
pub use storage::*;

// Re-export unified provider types
pub use backend::BackendType;
pub use provider::{ModelConfig, ProviderConfig};
pub use providers::ProvidersConfig;
