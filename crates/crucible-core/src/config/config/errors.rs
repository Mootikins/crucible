//! Configuration error types.

use thiserror::Error;

/// Errors that can occur during configuration operations.
///
/// This also carries configuration *validation* failures
/// (`EnrichmentBackendConfig::validate`): a missing field is
/// `MissingValue`, and an invalid field is `InvalidValue` with the
/// explanation in `value`. A second `ConfigValidationError` type used to
/// carry these under different variant names (`MissingField`,
/// `InvalidValue { reason }`); it added a type with the same two shapes and
/// no reader that needed them kept apart.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration value is missing.
    #[error("Missing configuration value: {field}")]
    MissingValue {
        /// The name of the missing configuration field
        field: String,
    },

    /// Configuration value is invalid.
    #[error("Invalid configuration value: {field} = {value}")]
    InvalidValue {
        /// The name of the invalid configuration field
        field: String,
        /// The invalid value that was provided
        value: String,
    },

    /// IO error during configuration loading.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// TOML parsing error.
    #[cfg(feature = "toml")]
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    /// TOML serialization error.
    #[cfg(feature = "toml")]
    #[error("TOML serialization error: {0}")]
    TomlSer(String),

    /// Provider configuration error.
    #[error("Provider configuration error: {0}")]
    Provider(String),

    /// General configuration error.
    #[error("{0}")]
    Other(String),
}

/// Adapter: convert internal anyhow errors to ConfigError at crate boundaries.
impl From<anyhow::Error> for ConfigError {
    fn from(err: anyhow::Error) -> Self {
        ConfigError::Other(err.to_string())
    }
}
