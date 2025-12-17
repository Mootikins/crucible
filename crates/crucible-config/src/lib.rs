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
mod enrichment;
mod includes;
mod loader;
mod profile;
mod value_source;

// Include test_utils when test-utils feature is enabled
#[cfg(feature = "test-utils")]
mod test_utils;

pub use components::*;
pub use config::{
    CacheConfig, CacheType, CliAppConfig, Config, ConfigError, ConfigValidationError,
    DatabaseConfig, DatabaseType, EffectiveLlmConfig, LoggingConfig, ProcessingConfig,
    ServerConfig,
};
pub use enrichment::*;
pub use includes::{process_file_references, IncludeConfig, IncludeError};
pub use loader::*;
pub use profile::*;
pub use value_source::*;

// Export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use test_utils::*;
