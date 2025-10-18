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
//!     let embedding_provider = config.embedding_provider()?;
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

mod config;
mod loader;
mod provider;
mod profile;
mod migration;
#[cfg(test)]
mod test_utils;

pub use config::*;
pub use loader::*;
pub use provider::*;
pub use profile::*;
pub use migration::*;
#[cfg(test)]
pub use test_utils::*;