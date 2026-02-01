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

mod components;
mod config;
pub mod credentials;
mod enrichment;
mod global;
mod includes;
mod loader;
mod patterns;
mod profile;
mod resolver;
mod security;
mod value_source;
mod workspace;

// Include test_utils when test-utils feature is enabled
#[cfg(feature = "test-utils")]
mod test_utils;

pub use components::*;
pub use config::{
    CacheConfig, CacheType, CliAppConfig, Config, ConfigError, ConfigValidationError,
    DatabaseConfig, DatabaseType, EffectiveLlmConfig, LoggingConfig, ProcessingConfig,
    ServerConfig,
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
pub use resolver::ConfigResolver;
pub use security::ShellPolicy;
pub use value_source::*;
pub use workspace::{KilnAttachment, SecurityConfig, WorkspaceConfig, WorkspaceMeta};

// Export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use test_utils::*;
