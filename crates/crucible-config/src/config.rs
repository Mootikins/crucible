//! Core configuration types and structures.

use crate::{EnrichmentConfig, ProfileConfig};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during configuration validation.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ConfigValidationError {
    /// A required field is missing or empty
    #[error("Missing required field: {field}")]
    MissingField {
        /// Name of the missing field
        field: String,
    },

    /// A field contains an invalid value
    #[error("Invalid value for {field}: {reason}")]
    InvalidValue {
        /// Name of the field with invalid value
        field: String,
        /// Reason why the value is invalid
        reason: String,
    },

    /// Multiple validation errors occurred
    #[error("Multiple validation errors: {errors:?}")]
    Multiple {
        /// List of validation errors
        errors: Vec<String>,
    },
}

/// Errors that can occur during configuration operations.
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

    /// YAML parsing error.
    #[cfg(feature = "yaml")]
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

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
}

/// Main configuration structure for the Crucible system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Current active profile name.
    #[serde(default)]
    pub profile: Option<String>,

    /// Available profiles configuration.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,

    /// Enrichment configuration (includes embedding provider).
    pub enrichment: Option<EnrichmentConfig>,

    /// Default database configuration.
    pub database: Option<DatabaseConfig>,

    /// Server configuration.
    pub server: Option<ServerConfig>,

    /// Logging configuration.
    pub logging: Option<LoggingConfig>,

    /// Custom configuration values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Some("default".to_string()),
            profiles: HashMap::from([("default".to_string(), ProfileConfig::default())]),
            enrichment: None,
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

    /// Get the effective enrichment configuration.
    pub fn enrichment_config(&self) -> Result<EnrichmentConfig, ConfigError> {
        if let Some(config) = &self.enrichment {
            return Ok(config.clone());
        }

        // Fall back to profile configuration
        let profile = self.active_profile()?;
        if let Some(config) = &profile.enrichment {
            return Ok(config.clone());
        }

        Err(ConfigError::MissingValue {
            field: "enrichment".to_string(),
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

    /// Get the kiln path from configuration.
    pub fn kiln_path(&self) -> Result<String, ConfigError> {
        self.get::<String>("kiln_path")?
            .ok_or_else(|| ConfigError::MissingValue {
                field: "kiln_path".to_string(),
            })
    }

    /// Get the kiln path or return None if not set.
    pub fn kiln_path_opt(&self) -> Option<String> {
        self.get::<String>("kiln_path").ok().flatten()
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

/// Cache configuration.
///
/// Consolidated from all crates to provide flexible caching control.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheConfig {
    /// Enable caching.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Cache type/strategy.
    #[serde(default)]
    pub cache_type: CacheType,

    /// Maximum cache size in entries.
    #[serde(default = "default_cache_max_size")]
    pub max_size: usize,

    /// Time-to-live in seconds.
    #[serde(default = "default_cache_ttl")]
    pub ttl_seconds: u64,

    /// Cache eviction policy (lru, lfu, fifo).
    #[serde(default = "default_eviction_policy")]
    pub eviction_policy: String,

    /// Additional cache-specific options.
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Supported cache types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CacheType {
    /// In-memory LRU cache.
    Lru,
    /// In-memory TTL cache.
    Ttl,
    /// Redis cache.
    Redis,
    /// No caching.
    None,
}

impl Default for CacheType {
    fn default() -> Self {
        Self::Lru
    }
}

fn default_cache_max_size() -> usize {
    10000
}

fn default_cache_ttl() -> u64 {
    300 // 5 minutes
}

fn default_eviction_policy() -> String {
    "lru".to_string()
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_type: CacheType::default(),
            max_size: default_cache_max_size(),
            ttl_seconds: default_cache_ttl(),
            eviction_policy: default_eviction_policy(),
            options: HashMap::new(),
        }
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
///
/// Consolidated from all crates to provide comprehensive logging control.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoggingConfig {
    /// Global log level (trace, debug, info, warn, error).
    #[serde(default = "default_level")]
    pub level: String,

    /// Log format (json, text, compact).
    #[serde(default = "default_format")]
    pub format: String,

    /// Enable console/stdout logging.
    #[serde(default = "default_true")]
    pub console: bool,

    /// Enable file logging.
    #[serde(default)]
    pub file: bool,

    /// Log file path.
    pub file_path: Option<String>,

    /// Enable log rotation.
    #[serde(default = "default_true")]
    pub rotation: bool,

    /// Maximum log file size in bytes.
    pub max_file_size: Option<u64>,

    /// Number of log files to retain.
    pub max_files: Option<u32>,

    /// Component/module-specific log levels (e.g., "crucible_core" => "debug").
    #[serde(default)]
    pub component_levels: HashMap<String, String>,

    /// Include timestamps in log output.
    #[serde(default = "default_true")]
    pub timestamps: bool,

    /// Include module/target path in log output.
    #[serde(default = "default_true")]
    pub target: bool,

    /// Use ANSI colors in console output.
    #[serde(default = "default_true")]
    pub ansi: bool,
}

fn default_level() -> String {
    "info".to_string()
}

fn default_format() -> String {
    "text".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: default_format(),
            console: true,
            file: false,
            file_path: None,
            rotation: true,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_files: Some(5),
            component_levels: HashMap::new(),
            timestamps: true,
            target: true,
            ansi: true,
        }
    }
}
