//! Core configuration types and structures.

use crate::{EmbeddingProviderConfig, ProfileConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during configuration operations.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration value is missing.
    #[error("Missing configuration value: {field}")]
    MissingValue {
        /// The name of the missing configuration field
        field: String
    },

    /// Configuration value is invalid.
    #[error("Invalid configuration value: {field} = {value}")]
    InvalidValue {
        /// The name of the invalid configuration field
        field: String,
        /// The invalid value that was provided
        value: String
    },

    /// IO error during configuration loading.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing error.
    #[cfg(feature = "yaml")]
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// TOML parsing error.
    #[cfg(feature = "toml")]
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    /// Provider configuration error.
    #[error("Provider configuration error: {0}")]
    Provider(String),
}

/// Main configuration structure for the Crucible system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Current active profile name.
    pub profile: Option<String>,

    /// Available profiles configuration.
    pub profiles: HashMap<String, ProfileConfig>,

    /// Default embedding provider configuration.
    pub embedding_provider: Option<EmbeddingProviderConfig>,

    /// Default database configuration.
    pub database: Option<DatabaseConfig>,

    /// Server configuration.
    pub server: Option<ServerConfig>,

    /// Logging configuration.
    pub logging: Option<LoggingConfig>,

    /// Custom configuration values.
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Some("default".to_string()),
            profiles: HashMap::from([("default".to_string(), ProfileConfig::default())]),
            embedding_provider: None,
            database: None,
            server: None,
            logging: None,
            custom: HashMap::new(),
        }
    }
}

impl Config {
    /// Create a new empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the active profile configuration.
    pub fn active_profile(&self) -> Result<&ProfileConfig, ConfigError> {
        let profile_name = self.profile.as_deref().unwrap_or("default");
        self.profiles
            .get(profile_name)
            .ok_or_else(|| ConfigError::MissingValue {
                field: format!("profile.{}", profile_name),
            })
    }

    /// Get the effective embedding provider configuration.
    pub fn embedding_provider(&self) -> Result<EmbeddingProviderConfig, ConfigError> {
        if let Some(provider) = &self.embedding_provider {
            return Ok(provider.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(provider) = &profile.embedding_provider {
            return Ok(provider.clone());
        }

        Err(ConfigError::MissingValue {
            field: "embedding_provider".to_string(),
        })
    }

    /// Get the effective database configuration.
    pub fn database(&self) -> Result<DatabaseConfig, ConfigError> {
        if let Some(database) = &self.database {
            return Ok(database.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(database) = &profile.database {
            return Ok(database.clone());
        }

        Err(ConfigError::MissingValue {
            field: "database".to_string(),
        })
    }

    /// Get the effective server configuration.
    pub fn server(&self) -> Result<ServerConfig, ConfigError> {
        if let Some(server) = &self.server {
            return Ok(server.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(server) = &profile.server {
            return Ok(server.clone());
        }

        // Return default server configuration
        Ok(ServerConfig::default())
    }

    /// Get the effective logging configuration.
    pub fn logging(&self) -> LoggingConfig {
        if let Some(logging) = &self.logging {
            return logging.clone();
        }

        // Fall back to profile configuration
        if let Ok(profile) = self.active_profile() {
            if let Some(logging) = &profile.logging {
                return logging.clone();
            }
        }

        // Return default logging configuration
        LoggingConfig::default()
    }

    /// Get a custom configuration value.
    pub fn get<T>(&self, key: &str) -> Result<Option<T>, ConfigError>
    where
        T: for<'de> Deserialize<'de>,
    {
        if let Some(value) = self.custom.get(key) {
            let typed = serde_json::from_value(value.clone())?;
            Ok(Some(typed))
        } else {
            Ok(None)
        }
    }

    /// Set a custom configuration value.
    pub fn set<T>(&mut self, key: String, value: T) -> Result<(), ConfigError>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.custom.insert(key, json_value);
        Ok(())
    }
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseConfig {
    /// Database type.
    #[serde(rename = "type")]
    pub db_type: DatabaseType,

    /// Database connection URL.
    pub url: String,

    /// Maximum number of connections.
    pub max_connections: Option<u32>,

    /// Connection timeout in seconds.
    pub timeout_seconds: Option<u64>,

    /// Additional database-specific options.
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Supported database types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    /// SQLite database.
    Sqlite,
    /// PostgreSQL database.
    Postgres,
    /// MySQL database.
    Mysql,
    /// SurrealDB database.
    Surrealdb,
    /// Custom database type.
    Custom(String),
}

impl Default for DatabaseType {
    fn default() -> Self {
        Self::Sqlite
    }
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host address.
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Enable HTTPS.
    #[serde(default)]
    pub https: bool,

    /// Path to TLS certificate file.
    pub cert_file: Option<String>,

    /// Path to TLS private key file.
    pub key_file: Option<String>,

    /// Maximum request body size in bytes.
    pub max_body_size: Option<usize>,

    /// Request timeout in seconds.
    pub timeout_seconds: Option<u64>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            https: false,
            cert_file: None,
            key_file: None,
            max_body_size: Some(10 * 1024 * 1024), // 10MB
            timeout_seconds: Some(30),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Logging level.
    #[serde(default = "default_level")]
    pub level: String,

    /// Log format (json, text).
    #[serde(default = "default_format")]
    pub format: String,

    /// Enable file logging.
    #[serde(default)]
    pub file: bool,

    /// Log file path.
    pub file_path: Option<String>,

    /// Maximum log file size in bytes.
    pub max_file_size: Option<u64>,

    /// Number of log files to retain.
    pub max_files: Option<u32>,
}

fn default_level() -> String {
    "info".to_string()
}

fn default_format() -> String {
    "text".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: default_format(),
            file: false,
            file_path: None,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_files: Some(5),
        }
    }
}