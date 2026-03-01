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

pub use acp::{AcpConfig, AgentProfile, DelegationConfig};
pub use chat::{AgentPreference, ChatConfig};
pub use cli::{CliConfig, HighlightingConfig};
pub use context::ContextConfig;
pub use defaults::{
    ANTHROPIC_MODELS, DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_MODEL, DEFAULT_BATCH_SIZE,
    DEFAULT_CHAT_MAX_TOKENS, DEFAULT_CHAT_MODEL, DEFAULT_GITHUB_COPILOT_ENDPOINT,
    DEFAULT_GITHUB_COPILOT_MODEL, DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OPENAI_ENDPOINT,
    DEFAULT_OPENAI_MODEL, DEFAULT_OPENROUTER_ENDPOINT, DEFAULT_OPENROUTER_MODEL,
    DEFAULT_PROVIDER_MAX_TOKENS, DEFAULT_TEMPERATURE, DEFAULT_TIMEOUT_SECS, DEFAULT_ZAI_ENDPOINT,
    DEFAULT_ZAI_MODEL, OPENAI_HARDCODED_MODELS, OPENAI_MODEL_PREFIXES, ZAI_MODELS,
};
pub use discovery::{DiscoveryPathsConfig, TypeDiscoveryConfig};
pub use gateway::{GatewayConfig, UpstreamServerConfig as GatewayUpstreamServerConfig};
pub use handlers::{
    BuiltinHandlersTomlConfig, HandlerConfig, HandlersConfig, ToolSelectorHandlerConfig,
};
pub use llm::{LlmConfig, LlmProviderConfig, LlmProviderConfigBuilder};
pub use mcp::{McpConfig, TransportType, UpstreamServerConfig};
pub use permissions::{
    parse_rule, CompiledPermissions, ParsedRule, PermissionConfig, PermissionDecision,
    PermissionEngine, PermissionMatcher, PermissionMode,
};
pub use storage::StorageConfig;

pub use backend::BackendType;
pub use trust::{DataClassification, TrustLevel};
