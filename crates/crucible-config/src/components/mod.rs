//! Essential configuration components for Crucible
//!
//! Simple, focused configuration for the core components that actually need it.

pub mod acp;
pub mod chat;
pub mod cli;
pub mod context;
pub mod defaults;
pub mod discovery;
pub mod gateway;
pub mod handlers;
pub mod llm;
pub mod mcp;
pub mod permissions;
pub mod storage;

pub mod backend;
pub mod trust;

pub use acp::*;
pub use chat::*;
pub use cli::*;
pub use context::*;
pub use defaults::*;
pub use discovery::*;
pub use gateway::{GatewayConfig, UpstreamServerConfig as GatewayUpstreamServerConfig};
pub use handlers::*;
pub use llm::*;
pub use mcp::{McpConfig, TransportType, UpstreamServerConfig};
pub use permissions::{
    parse_rule, CompiledPermissions, ParsedRule, PermissionConfig, PermissionDecision,
    PermissionEngine, PermissionMatcher, PermissionMode,
};
pub use storage::*;

pub use backend::BackendType;
pub use trust::{DataClassification, TrustLevel};
