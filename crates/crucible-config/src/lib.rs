//! # Crucible Configuration Library
//!
//! A flexible, production-ready configuration management system for the Crucible ecosystem.
//! Provides type-safe configuration loading, validation, and migration capabilities.
//!
//! ## Features
//!
//! - Multi-format support (YAML, TOML, JSON)
//! - Environment-specific profiles
//! - Provider configuration management
//! - Migration utilities for backward compatibility
//! - Test utilities for easy testing
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use crucible_config::{Config, ConfigLoader};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = ConfigLoader::load_from_file("config.yaml").await?;
//!     let enrichment_config = config.enrichment_config()?;
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod components;
mod config;
pub mod credentials;
mod enrichment;
mod global;
mod includes;
mod io_helpers;
mod kiln_config;
mod loader;
mod patterns;
mod profile;
mod project_config;
mod security;
pub mod serde_helpers;
mod value_source;
mod workspace;

pub use components::defaults::{
    ANTHROPIC_MODELS, DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_MODEL, DEFAULT_BATCH_SIZE,
    DEFAULT_CHAT_MAX_TOKENS, DEFAULT_CHAT_MODEL, DEFAULT_GITHUB_COPILOT_ENDPOINT,
    DEFAULT_GITHUB_COPILOT_MODEL, DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OPENAI_ENDPOINT,
    DEFAULT_OPENAI_MODEL, DEFAULT_OPENROUTER_ENDPOINT, DEFAULT_OPENROUTER_MODEL,
    DEFAULT_PROVIDER_MAX_TOKENS, DEFAULT_TEMPERATURE, DEFAULT_TIMEOUT_SECS, DEFAULT_ZAI_ENDPOINT,
    DEFAULT_ZAI_MODEL, OPENAI_HARDCODED_MODELS, OPENAI_MODEL_PREFIXES, ZAI_MODELS,
};
pub use components::mcp;
pub use components::{
    AcpConfig, AgentPreference, AgentProfile, BackendType, ChatConfig, CliConfig,
    CompiledPermissions, ContextConfig, DataClassification, DelegationConfig, DiscoveryPathsConfig,
    GatewayConfig, GatewayUpstreamServerConfig, HandlersConfig, HighlightingConfig, LlmConfig,
    LlmProviderConfig, McpConfig, PermissionConfig, PermissionDecision, PermissionEngine,
    PermissionMode, StorageConfig, TransportType, TrustLevel, TypeDiscoveryConfig,
    UpstreamServerConfig,
};
pub use config::{
    crucible_home, is_crucible_home, CliAppConfig, Config, ConfigError, ConfigValidationError,
    EffectiveLlmConfig, LoggingConfig, ProcessingConfig, ScmConfig, ServerConfig, WebConfig,
};
#[cfg(feature = "keyring")]
pub use credentials::KeyringStore;
pub use credentials::{
    resolve_api_key, AutoStore, CredentialError, CredentialResult, CredentialSource,
    CredentialStore, ProviderSecrets, SecretsFile, SecretsFileContent,
};
pub use enrichment::{
    default_max_precognition_chars, BurnBackendConfig, BurnEmbedConfig, CohereConfig, CustomConfig,
    EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig, MockConfig, OllamaConfig,
    OpenAIConfig, PipelineConfig, VertexAIConfig,
};
pub use global::GlobalConfig;
pub use includes::{process_file_references, IncludeConfig, IncludeError, ResolveMode};
pub use kiln_config::{read_kiln_config, write_kiln_config, KilnConfig, KilnMeta};
pub use loader::{ConfigFormat, ConfigLoader};
pub use patterns::{
    BashPatterns, FilePatterns, PatternError, PatternResult, PatternStore, ToolPatterns,
};
pub use profile::{Environment, ProfileConfig};
pub use project_config::{read_project_config, write_project_config, ProjectConfig, ProjectMeta};
pub use security::ShellPolicy;
pub use value_source::{ValueInfo, ValueSource, ValueSourceMap};
#[allow(deprecated)]
pub use workspace::{KilnAttachment, SecurityConfig, WorkspaceConfig, WorkspaceMeta};
