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
pub mod mcp;
pub mod storage;

pub mod backend;
pub mod provider;
pub mod providers;

pub use acp::*;
pub use chat::*;
pub use cli::*;
pub use context::*;
pub use discovery::*;
pub use embedding::*;
pub use gateway::{GatewayConfig, UpstreamServerConfig as GatewayUpstreamServerConfig};
pub use handlers::*;
pub use llm::*;
pub use mcp::{McpConfig, TransportType, UpstreamServerConfig};
pub use storage::*;

pub use backend::BackendType;
pub use provider::{ModelConfig, ProviderConfig};
pub use providers::ProvidersConfig;
