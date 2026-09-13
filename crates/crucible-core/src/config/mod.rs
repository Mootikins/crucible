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
pub mod lua_emit;
pub mod merge;
pub mod overlay;
mod patterns;
pub mod plugin_spec;
mod project_config;
pub mod provenance;
pub mod redact;
mod security;
pub mod serde_helpers;
pub mod settings_file;
pub mod store;
mod tilde;
mod workspace;

pub use components::defaults::{
    ANTHROPIC_MODELS, DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_MODEL, DEFAULT_BATCH_SIZE,
    DEFAULT_CHAT_MODEL, DEFAULT_GITHUB_COPILOT_ENDPOINT, DEFAULT_GITHUB_COPILOT_MODEL,
    DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OPENAI_ENDPOINT, DEFAULT_OPENAI_MODEL,
    DEFAULT_OPENROUTER_ENDPOINT, DEFAULT_OPENROUTER_MODEL, DEFAULT_TIMEOUT_SECS,
    DEFAULT_ZAI_ENDPOINT, DEFAULT_ZAI_MODEL, OPENAI_HARDCODED_MODELS, OPENAI_MODEL_PREFIXES,
    ZAI_MODELS,
};
pub use components::mcp;
pub use components::{
    ollama_endpoint_from_env, AcpConfig, AgentPreference, AgentProfile, BackendType, ChatConfig,
    CliConfig, CompiledPermissions, ContextConfig, DataClassification, DelegationConfig,
    HighlightingConfig, LlmConfig, LlmProviderConfig, McpConfig, OllamaModelTag,
    OllamaTagsResponse, PermissionConfig, PermissionDecision, PermissionEngine, PermissionMode,
    TransportType, TrustLevel, UpstreamServerConfig,
};
pub use config::registry::{resolve_kiln_entries, KilnEntry, ProjectEntry};
// Its own line rather than folded into the block below: this is the
// location/settings classification of `CliAppConfig`'s top-level keys, and it
// has two security consumers (the plugin-visible config store and
// `config.set`) that should be able to find it without reading a 6-line list.
#[cfg(feature = "toml")]
pub use config::{
    crucible_home, declared_plugins, lua_stubs_dir, lua_stubs_dir_in, parse_duration_string,
    plugin_name_from_url, CliAppConfig, ConfigError, ConfigValidationError, EffectiveLlmConfig,
    InvalidKilnName, KilnName, LoggingConfig, PluginEntry, PluginsConfig, ScheduleEntry,
    ServerConfig, WebConfig, WorkspaceConfig, PLUGINS_DECLARE_KEY,
};
pub use config::{LOCATION_CONFIG_KEYS, SETTINGS_CONFIG_KEYS};
pub use credentials::{
    discover_credentials, resolve_api_key, CredentialError, CredentialResult, CredentialSource,
    DiscoveredCredential, ProviderSecrets, SecretsFile, SecretsFileContent,
};
pub use enrichment::{
    default_max_precognition_chars, EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig,
    MockConfig, OllamaConfig, OpenAIConfig, PipelineConfig,
};
pub use includes::{process_file_references, IncludeError};
pub use kiln_config::{read_kiln_config, write_kiln_config, KilnConfig, KilnMeta};
pub use lua_emit::emit_lua_config;
pub use merge::{deep_merge, flatten_leaves, leaf_at, nest_leaves, set_leaf};
pub use overlay::{
    overlay_layers, overlay_registrations, LayeredOverlay, Overlay, Registration,
    RegistrationOrigin, Shadowed, ShadowedRegistration,
};
pub use patterns::{
    BashPatterns, FilePatterns, PatternError, PatternResult, PatternStore, RefusedRule,
    ToolPatterns,
};
pub use plugin_spec::{Spec, SpecEntry, SpecRank, SpecSource};
pub use project_config::{read_project_config, write_project_config, ProjectConfig};
pub use provenance::{ConfigSource, LastSet, LeafOrigin, ProvenanceMap, SourceOrigin};
pub use redact::{names_a_credential, redact_credentials, REDACTED};
pub use security::{ProjectFileAccess, ShellPolicy};
pub use settings_file::{load_settings, save_settings_delta, settings_path, SETTINGS_FILE_NAME};
pub use store::{
    split_pinned_by, ConfigStore, LayerDrop, LocationPolicy, PinnedLeaf, SavedSettings,
};
pub use tilde::expand_tilde;
pub use workspace::{KilnAttachment, SecurityConfig};
