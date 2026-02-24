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
mod loader;
mod patterns;
mod profile;
mod security;
mod value_source;
mod workspace;
pub use components::*;
pub use config::{
    crucible_home, is_crucible_home, CliAppConfig, Config, ConfigError,
    ConfigValidationError, EffectiveLlmConfig, LoggingConfig,
    ProcessingConfig, ScmConfig, ServerConfig, WebConfig,
};
#[cfg(feature = "keyring")]
pub use credentials::KeyringStore;
pub use credentials::{
    resolve_api_key, AutoStore, CredentialError, CredentialResult, CredentialSource,
    CredentialStore, ProviderSecrets, SecretsFile, SecretsFileContent,
};
pub use enrichment::*;
pub use global::GlobalConfig;
pub use includes::{process_file_references, IncludeConfig, IncludeError};
pub use loader::*;
pub use patterns::{
    BashPatterns, FilePatterns, PatternError, PatternResult, PatternStore, ToolPatterns,
};
pub use profile::*;
pub use security::ShellPolicy;
pub use value_source::*;
pub use workspace::{KilnAttachment, SecurityConfig, WorkspaceConfig, WorkspaceMeta};

