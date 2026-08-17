//! # Crucible Configuration Library
//!
//! A flexible, production-ready configuration management system for the Crucible ecosystem.
//! Provides type-safe configuration loading, validation, and migration capabilities.
//!
//! ## Features
//!
//! - Environment-specific profiles
//! - Provider configuration management
//! - Migration utilities for backward compatibility
//! - Test utilities for easy testing
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use crucible_core::config::CliAppConfig;
//!
//! // `None` uses `CliAppConfig::default_config_path()`; the two `Option`s are
//! // the `--embedding-url` / `--embedding-model` CLI overrides.
//! let config = CliAppConfig::load(None, None, None)?;
//! let provider = config.effective_llm_provider()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
// After move into crucible-core, the inner `config` module ends up at
// crate::config::config which triggers module_inception. Renaming would ripple
// through many internal imports for no practical gain — the module only
// re-exports internal types.
#![allow(clippy::module_inception)]

pub mod components;
mod config;
pub mod credentials;
mod enrichment;
mod includes;
mod io_helpers;
mod kiln_config;
mod patterns;
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
    HandlersConfig, HighlightingConfig, LlmConfig, LlmProviderConfig, McpConfig, PermissionConfig,
    PermissionDecision, PermissionEngine, PermissionMode, StorageConfig, TransportType, TrustLevel,
    TypeDiscoveryConfig, UpstreamServerConfig,
};
pub use config::registry::{resolve_kiln_entries, KilnEntry, ProjectEntry};
// Its own line rather than folded into the block below: this is the
// location/settings classification of `CliAppConfig`'s top-level keys, and it
// has two security consumers (the plugin-visible config store and
// `config.set`) that should be able to find it without reading a 6-line list.
pub use config::{
    crucible_home, lua_stubs_dir, parse_duration_string, plugin_name_from_url, CliAppConfig,
    ConfigError, ConfigValidationError, EffectiveLlmConfig, InvalidKilnName, KilnName,
    LoggingConfig, PluginEntry, PluginsConfig, ScheduleEntry, ScmConfig, ServerConfig, WebConfig,
};
#[cfg(feature = "toml")]
pub use config::{
    register_kiln_entry_in_config, register_kiln_in_config, register_llm_provider_in_config,
    register_project_in_config,
};
pub use config::{LOCATION_CONFIG_KEYS, SETTINGS_CONFIG_KEYS};
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
pub use includes::{process_file_references, IncludeError, ResolveMode};
pub use kiln_config::{read_kiln_config, write_kiln_config, KilnConfig, KilnMeta};
pub use patterns::{
    BashPatterns, FilePatterns, PatternError, PatternResult, PatternStore, ToolPatterns,
};
pub use project_config::{read_project_config, write_project_config, ProjectConfig, ProjectMeta};
pub use security::{ProjectFileAccess, ShellPolicy};
pub use value_source::{ValueInfo, ValueSource, ValueSourceMap};
#[allow(deprecated)]
pub use workspace::{KilnAttachment, SecurityConfig, WorkspaceConfig, WorkspaceMeta};
