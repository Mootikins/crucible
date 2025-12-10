//! Core configuration types and structures.

use crate::components::{
    AcpConfig, ChatConfig, CliConfig, DiscoveryPathsConfig, EmbeddingConfig, EmbeddingProviderType,
    GatewayConfig, HooksConfig,
};
use crate::{EnrichmentConfig, ProfileConfig};

#[cfg(feature = "toml")]
extern crate toml;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use tracing::{debug, error, info, warn};

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

    /// CLI configuration.
    #[serde(default)]
    pub cli: Option<CliConfig>,

    /// Embedding configuration.
    #[serde(default)]
    pub embedding: Option<EmbeddingConfig>,

    /// ACP (Agent Client Protocol) configuration.
    #[serde(default)]
    pub acp: Option<AcpConfig>,

    /// Chat configuration.
    #[serde(default)]
    pub chat: Option<ChatConfig>,

    /// Server configuration.
    pub server: Option<ServerConfig>,

    /// Logging configuration.
    pub logging: Option<LoggingConfig>,

    /// Discovery paths configuration.
    #[serde(default)]
    pub discovery: Option<DiscoveryPathsConfig>,

    /// Gateway configuration for upstream MCP servers.
    #[serde(default)]
    pub gateway: Option<GatewayConfig>,

    /// Hooks configuration.
    #[serde(default)]
    pub hooks: Option<HooksConfig>,

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
            cli: Some(CliConfig::default()),
            embedding: Some(EmbeddingConfig::default()),
            acp: Some(AcpConfig::default()),
            chat: Some(ChatConfig::default()),
            server: None,
            logging: None,
            discovery: None,
            gateway: None,
            hooks: None,
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

    /// Get the effective CLI configuration.
    pub fn cli_config(&self) -> Result<CliConfig, ConfigError> {
        if let Some(config) = &self.cli {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(CliConfig::default())
    }

    /// Get the effective embedding configuration.
    pub fn embedding_config(&self) -> Result<EmbeddingConfig, ConfigError> {
        if let Some(config) = &self.embedding {
            return Ok(config.clone());
        }

        // Fall back to default with FastEmbed
        Ok(EmbeddingConfig::default())
    }

    /// Get the effective ACP configuration.
    pub fn acp_config(&self) -> Result<AcpConfig, ConfigError> {
        if let Some(config) = &self.acp {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(AcpConfig::default())
    }

    /// Get the effective chat configuration.
    pub fn chat_config(&self) -> Result<ChatConfig, ConfigError> {
        if let Some(config) = &self.chat {
            return Ok(config.clone());
        }

        // Fall back to default
        Ok(ChatConfig::default())
    }

    /// Get the effective discovery configuration.
    pub fn discovery_config(&self) -> Option<&DiscoveryPathsConfig> {
        self.discovery.as_ref()
    }

    /// Get the effective gateway configuration.
    pub fn gateway_config(&self) -> Option<&GatewayConfig> {
        self.gateway.as_ref()
    }

    /// Get the effective hooks configuration.
    pub fn hooks_config(&self) -> Option<&HooksConfig> {
        self.hooks.as_ref()
    }

    /// Validate gateway configuration
    pub fn validate_gateway(&self) -> Result<(), ConfigValidationError> {
        if let Some(gateway) = &self.gateway {
            let mut errors = Vec::new();

            for server in &gateway.servers {
                // Validate server name is not empty
                if server.name.is_empty() {
                    errors.push("Gateway server name cannot be empty".to_string());
                }

                // Validate transport configuration
                match &server.transport {
                    crate::components::TransportType::Stdio { command, .. } => {
                        if command.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': stdio command cannot be empty",
                                server.name
                            ));
                        }
                    }
                    crate::components::TransportType::Sse { url, .. } => {
                        if url.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': SSE url cannot be empty",
                                server.name
                            ));
                        }
                        // Validate URL format
                        if !url.starts_with("http://") && !url.starts_with("https://") {
                            errors.push(format!(
                                "Gateway server '{}': SSE url must start with http:// or https://",
                                server.name
                            ));
                        }
                    }
                }

                // Validate prefix/suffix if present
                if let Some(prefix) = &server.prefix {
                    if prefix.is_empty() {
                        errors.push(format!(
                            "Gateway server '{}': prefix cannot be empty string",
                            server.name
                        ));
                    }
                }
            }

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate hooks configuration (checks pattern validity)
    pub fn validate_hooks(&self) -> Result<(), ConfigValidationError> {
        if let Some(hooks) = &self.hooks {
            let mut errors = Vec::new();

            // Validate patterns are valid glob patterns
            // For now, just check they're not empty if present
            let mut check_pattern = |name: &str, pattern: &Option<String>| {
                if let Some(p) = pattern {
                    if p.is_empty() {
                        errors.push(format!("Hook '{}': pattern cannot be empty string", name));
                    }
                }
            };

            check_pattern("test_filter", &hooks.builtin.test_filter.pattern);
            check_pattern("toon_transform", &hooks.builtin.toon_transform.pattern);
            check_pattern(
                "recipe_enrichment",
                &hooks.builtin.recipe_enrichment.pattern,
            );
            check_pattern("tool_selector", &hooks.builtin.tool_selector.pattern);

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate discovery configuration (checks path format validity)
    pub fn validate_discovery(&self) -> Result<(), ConfigValidationError> {
        if let Some(discovery) = &self.discovery {
            let mut errors = Vec::new();

            // Validate all type configs
            for (type_name, type_config) in &discovery.type_configs {
                for (idx, path) in type_config.additional_paths.iter().enumerate() {
                    let path_str = path.to_string_lossy();
                    // Check for empty paths
                    if path_str.trim().is_empty() {
                        errors.push(format!(
                            "Discovery '{}': additional_paths[{}] cannot be empty",
                            type_name, idx
                        ));
                    }
                }
            }

            // Validate flat format configs
            let mut check_type_config = |type_name: &str, config: &crate::components::TypeDiscoveryConfig| {
                for (idx, path) in config.additional_paths.iter().enumerate() {
                    let path_str = path.to_string_lossy();
                    if path_str.trim().is_empty() {
                        errors.push(format!(
                            "Discovery '{}': additional_paths[{}] cannot be empty",
                            type_name, idx
                        ));
                    }
                }
            };

            if let Some(hooks) = &discovery.hooks {
                check_type_config("hooks", hooks);
            }
            if let Some(tools) = &discovery.tools {
                check_type_config("tools", tools);
            }
            if let Some(events) = &discovery.events {
                check_type_config("events", events);
            }

            if !errors.is_empty() {
                return Err(ConfigValidationError::Multiple { errors });
            }
        }

        Ok(())
    }

    /// Validate all configuration sections
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        self.validate_gateway()?;
        self.validate_hooks()?;
        self.validate_discovery()?;
        Ok(())
    }
}

/// Processing configuration for file processing operations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingConfig {
    /// Number of parallel workers for processing (default: num_cpus / 2)
    #[serde(default)]
    pub parallel_workers: Option<usize>,
}

/// CLI application composite configuration structure.
///
/// This provides the main configuration interface for the CLI application,
/// combining all necessary components with sensible defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAppConfig {
    /// Path to the Obsidian kiln directory
    #[serde(default = "default_kiln_path")]
    pub kiln_path: std::path::PathBuf,

    /// Additional directories to search for agent cards
    ///
    /// Paths can be absolute or relative (to config file location).
    /// These are loaded after the default locations.
    #[serde(default)]
    pub agent_directories: Vec<std::path::PathBuf>,

    /// Embedding configuration
    #[serde(default)]
    pub embedding: EmbeddingConfig,

    /// ACP (Agent Client Protocol) configuration
    #[serde(default)]
    pub acp: AcpConfig,

    /// Chat configuration
    #[serde(default)]
    pub chat: ChatConfig,

    /// CLI-specific configuration
    #[serde(default)]
    pub cli: CliConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    /// Processing configuration
    #[serde(default)]
    pub processing: ProcessingConfig,
}

fn default_kiln_path() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

impl Default for CliAppConfig {
    fn default() -> Self {
        Self {
            kiln_path: default_kiln_path(),
            agent_directories: Vec::new(),
            embedding: EmbeddingConfig::default(),
            acp: AcpConfig::default(),
            chat: ChatConfig::default(),
            cli: CliConfig::default(),
            logging: None,
            processing: ProcessingConfig::default(),
        }
    }
}

impl CliAppConfig {
    /// Load CLI configuration from file with env var and CLI flag overrides
    ///
    /// Priority (highest to lowest):
    /// 1. CLI flags (--embedding-url, --embedding-model)
    /// 2. Environment variables (CRUCIBLE_KILN_PATH, CRUCIBLE_EMBEDDING_URL, CRUCIBLE_EMBEDDING_MODEL)
    /// 3. Config file (~/.config/crucible/config.toml)
    /// 4. Default values
    pub fn load(
        config_file: Option<std::path::PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> anyhow::Result<Self> {
        // Determine config file path
        let config_path = config_file.unwrap_or_else(Self::default_config_path);

        debug!("Attempting to load config from: {}", config_path.display());

        // Try to load config file or use defaults
        let mut config = if config_path.exists() {
            info!("Found config file at: {}", config_path.display());

            let contents = std::fs::read_to_string(&config_path)
                .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

            #[cfg(feature = "toml")]
            {
                match toml::from_str::<CliAppConfig>(&contents) {
                    Ok(cfg) => {
                        info!("Successfully loaded config file: {}", config_path.display());
                        cfg
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse config file {}: {}",
                            config_path.display(),
                            e
                        );
                        return Err(anyhow::anyhow!(
                            "Failed to parse config file {}: {}",
                            config_path.display(),
                            e
                        ));
                    }
                }
            }

            #[cfg(not(feature = "toml"))]
            {
                return Err(anyhow::anyhow!(
                    "Failed to parse config file: TOML feature not enabled"
                ));
            }
        } else {
            debug!(
                "No config file found at {}, using defaults",
                config_path.display()
            );
            Self::default()
        };

        // Apply environment variable overrides (priority 2)
        Self::apply_env_overrides(&mut config);

        // Apply CLI flag overrides (priority 1 - highest)
        if let Some(url) = embedding_url {
            debug!("Overriding embedding.api_url from CLI flag: {}", url);
            config.embedding.api_url = Some(url);
        }
        if let Some(model) = embedding_model {
            debug!("Overriding embedding.model from CLI flag: {}", model);
            config.embedding.model = Some(model);
        }

        Ok(config)
    }

    /// Apply environment variable overrides to configuration
    ///
    /// Supported env vars:
    /// - CRUCIBLE_KILN_PATH: Path to the kiln (Obsidian vault)
    /// - CRUCIBLE_EMBEDDING_URL: Embedding provider API URL
    /// - CRUCIBLE_EMBEDDING_MODEL: Embedding model name
    /// - CRUCIBLE_EMBEDDING_PROVIDER: Embedding provider type (fastembed, ollama, openai)
    fn apply_env_overrides(config: &mut Self) {
        // Kiln path override
        if let Ok(kiln_path) = std::env::var("CRUCIBLE_KILN_PATH") {
            debug!("Overriding kiln_path from env: {}", kiln_path);
            config.kiln_path = std::path::PathBuf::from(kiln_path);
        }

        // Embedding API URL override
        if let Ok(url) = std::env::var("CRUCIBLE_EMBEDDING_URL") {
            debug!("Overriding embedding.api_url from env: {}", url);
            config.embedding.api_url = Some(url);
        }

        // Embedding model override
        if let Ok(model) = std::env::var("CRUCIBLE_EMBEDDING_MODEL") {
            debug!("Overriding embedding.model from env: {}", model);
            config.embedding.model = Some(model);
        }

        // Embedding provider override
        if let Ok(provider) = std::env::var("CRUCIBLE_EMBEDDING_PROVIDER") {
            debug!("Overriding embedding.provider from env: {}", provider);
            config.embedding.provider = match provider.to_lowercase().as_str() {
                "fastembed" => EmbeddingProviderType::FastEmbed,
                "ollama" => EmbeddingProviderType::Ollama,
                "openai" => EmbeddingProviderType::OpenAI,
                "anthropic" => EmbeddingProviderType::Anthropic,
                "cohere" => EmbeddingProviderType::Cohere,
                "vertexai" => EmbeddingProviderType::VertexAI,
                "custom" => EmbeddingProviderType::Custom,
                "mock" => EmbeddingProviderType::Mock,
                _ => {
                    warn!(
                        "Unknown embedding provider '{}', keeping current: {:?}",
                        provider, config.embedding.provider
                    );
                    config.embedding.provider.clone()
                }
            };
        }

        // Max concurrent embedding jobs override
        if let Ok(max_concurrent) = std::env::var("CRUCIBLE_EMBEDDING_MAX_CONCURRENT") {
            if let Ok(n) = max_concurrent.parse::<usize>() {
                debug!("Overriding embedding.max_concurrent from env: {}", n);
                config.embedding.max_concurrent = Some(n);
            }
        }
    }

    /// Log the effective configuration for debugging
    pub fn log_config(&self) {
        info!("Effective configuration:");
        info!("  kiln_path: {}", self.kiln_path.display());
        info!("  embedding.provider: {:?}", self.embedding.provider);
        info!("  embedding.model: {:?}", self.embedding.model);
        info!("  embedding.batch_size: {}", self.embedding.batch_size);
        info!("  acp.default_agent: {:?}", self.acp.default_agent);
        info!("  acp.enable_discovery: {}", self.acp.enable_discovery);
        info!(
            "  acp.session_timeout_minutes: {}",
            self.acp.session_timeout_minutes
        );
        info!("  cli.show_progress: {}", self.cli.show_progress);
        info!(
            "  cli.confirm_destructive: {}",
            self.cli.confirm_destructive
        );
        info!("  cli.verbose: {}", self.cli.verbose);
    }

    /// Get database path (always derived from kiln path)
    pub fn database_path(&self) -> std::path::PathBuf {
        // Only use PID suffix in test mode to prevent RocksDB lock collisions
        let db_name = if std::env::var("CRUCIBLE_TEST_MODE").is_ok() {
            let pid = std::process::id();
            format!("kiln-{}.db", pid)
        } else {
            "kiln.db".to_string()
        };
        self.kiln_path.join(".crucible").join(db_name)
    }

    /// Get tools directory path (always derived from kiln path)
    pub fn tools_path(&self) -> std::path::PathBuf {
        self.kiln_path.join("tools")
    }

    /// Get database path as a string
    pub fn database_path_str(&self) -> anyhow::Result<String> {
        self.database_path()
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))
    }

    /// Get kiln path as a string
    pub fn kiln_path_str(&self) -> anyhow::Result<String> {
        self.kiln_path
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Kiln path is not valid UTF-8"))
    }

    /// Display the current configuration as TOML
    #[cfg(feature = "toml")]
    pub fn display_as_toml(&self) -> anyhow::Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config as TOML: {}", e))
    }

    /// Display the current configuration as TOML (placeholder when toml feature is disabled)
    #[cfg(not(feature = "toml"))]
    pub fn display_as_toml(&self) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("TOML feature not enabled"))
    }

    /// Display the current configuration as JSON
    pub fn display_as_json(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config as JSON: {}", e))
    }

    /// Create a new config file with example values
    pub fn create_example(path: &std::path::Path) -> anyhow::Result<()> {
        let example = r#"# Crucible CLI Configuration
# Location: ~/.config/crucible/config.toml

# Path to your Obsidian kiln
# Default: current directory
kiln_path = "/home/user/Documents/my-kiln"

# Additional directories to search for agent cards (optional)
# Paths can be absolute or relative to this config file location
# agent_directories = ["/home/user/shared-agents", "./docs/agents"]

# Embedding configuration
[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
batch_size = 16

# ACP (Agent Client Protocol) configuration
[acp]
default_agent = null
enable_discovery = true
session_timeout_minutes = 30
max_message_size_mb = 25

# Chat configuration
[chat]
model = null
enable_markdown = true

# CLI configuration
[cli]
show_progress = true
confirm_destructive = true
verbose = false

# Logging configuration (optional)
# If not set, defaults to "off" unless --verbose or --log-level is specified
# [logging]
# level = "info"  # off, error, warn, info, debug, trace

# Processing configuration (optional)
# [processing]
# parallel_workers = 4  # Number of parallel workers (default: num_cpus / 2)
"#;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create config directory: {}", e))?;
        }

        std::fs::write(path, example)
            .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;
        Ok(())
    }

    // Legacy compatibility methods
    #[allow(missing_docs)]
    pub fn chat_model(&self) -> String {
        self.chat
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string())
    }

    #[allow(missing_docs)]
    pub fn temperature(&self) -> f32 {
        0.7 // Default temperature
    }

    #[allow(missing_docs)]
    pub fn max_tokens(&self) -> u32 {
        2048 // Default max tokens
    }

    #[allow(missing_docs)]
    pub fn streaming(&self) -> bool {
        true // Default streaming
    }

    #[allow(missing_docs)]
    pub fn system_prompt(&self) -> String {
        "You are a helpful assistant.".to_string()
    }

    #[allow(missing_docs)]
    pub fn ollama_endpoint(&self) -> String {
        "http://localhost:11434".to_string()
    }

    #[allow(missing_docs)]
    pub fn timeout(&self) -> u64 {
        30 // Default timeout
    }

    #[allow(missing_docs)]
    pub fn openai_api_key(&self) -> Option<String> {
        std::env::var("OPENAI_API_KEY").ok()
    }

    #[allow(missing_docs)]
    pub fn anthropic_api_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY").ok()
    }

    /// Get the default config file path
    pub fn default_config_path() -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        home.join(".config").join("crucible").join("config.toml")
    }

    /// Get the logging level from config, if set
    ///
    /// Returns the log level string (e.g., "off", "error", "warn", "info", "debug", "trace")
    /// from the logging configuration section, or None if not configured.
    pub fn logging_level(&self) -> Option<String> {
        self.logging.as_ref().map(|l| l.level.clone())
    }

    /// Get the parallel workers setting from config, if set
    ///
    /// Returns the number of parallel workers for processing, or None if not configured.
    /// When None, the CLI should use a default (e.g., num_cpus / 2).
    pub fn parallel_workers(&self) -> Option<usize> {
        self.processing.parallel_workers
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_agent_directories_default_empty() {
        let config = CliAppConfig::default();
        assert!(config.agent_directories.is_empty());
    }

    #[test]
    fn test_agent_directories_loads_from_toml() {
        let toml_content = r#"
kiln_path = "/tmp/test-kiln"
agent_directories = ["/home/user/shared-agents", "./local-agents"]

[embedding]
provider = "fastembed"
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

        assert_eq!(config.agent_directories.len(), 2);
        assert_eq!(
            config.agent_directories[0],
            std::path::PathBuf::from("/home/user/shared-agents")
        );
        assert_eq!(
            config.agent_directories[1],
            std::path::PathBuf::from("./local-agents")
        );
    }

    #[test]
    fn test_agent_directories_optional_when_missing() {
        let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[embedding]
provider = "fastembed"
"#;
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

        assert!(config.agent_directories.is_empty());
    }

    #[test]
    fn test_config_with_new_sections() {
        let toml_content = r#"
profile = "default"

[discovery.type_configs.tools]
additional_paths = ["/custom/tools"]
use_defaults = true

[[gateway.servers]]
name = "github"
prefix = "gh_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[hooks.builtin.test_filter]
enabled = true
pattern = "just_test*"
priority = 10

[hooks.builtin.tool_selector]
enabled = true
allowed_tools = ["search_*"]
"#;

        let config: Config = toml::from_str(toml_content).unwrap();

        // Check discovery config
        assert!(config.discovery.is_some());
        let discovery = config.discovery.as_ref().unwrap();
        assert!(discovery.type_configs.contains_key("tools"));

        // Check gateway config
        assert!(config.gateway.is_some());
        let gateway = config.gateway.as_ref().unwrap();
        assert_eq!(gateway.servers.len(), 1);
        assert_eq!(gateway.servers[0].name, "github");

        // Check hooks config
        assert!(config.hooks.is_some());
        let hooks = config.hooks.as_ref().unwrap();
        assert!(hooks.builtin.test_filter.enabled);
        assert!(hooks.builtin.tool_selector.enabled);
    }

    #[test]
    fn test_validate_gateway_empty_name() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::UpstreamServerConfig {
                    name: "".to_string(),
                    transport: crate::components::TransportType::Stdio {
                        command: "test".to_string(),
                        args: vec![],
                        env: std::collections::HashMap::new(),
                    },
                    prefix: None,
                    allowed_tools: None,
                    blocked_tools: None,
                    auto_reconnect: true,
                }],
            }),
            ..Config::default()
        };

        let result = config.validate_gateway();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_gateway_invalid_sse_url() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::TransportType::Sse {
                        url: "invalid-url".to_string(),
                        auth_header: None,
                    },
                    prefix: None,
                    allowed_tools: None,
                    blocked_tools: None,
                    auto_reconnect: true,
                }],
            }),
            ..Config::default()
        };

        let result = config.validate_gateway();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_gateway_valid() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::TransportType::Sse {
                        url: "http://localhost:3000/sse".to_string(),
                        auth_header: None,
                    },
                    prefix: Some("test_".to_string()),
                    allowed_tools: None,
                    blocked_tools: None,
                    auto_reconnect: true,
                }],
            }),
            ..Config::default()
        };

        let result = config.validate_gateway();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hooks_empty_pattern() {
        let config = Config {
            hooks: Some(HooksConfig {
                builtin: crate::components::BuiltinHooksTomlConfig {
                    test_filter: crate::components::HookConfig {
                        enabled: true,
                        pattern: Some("".to_string()),
                        priority: Some(10),
                    },
                    ..Default::default()
                },
            }),
            ..Config::default()
        };

        let result = config.validate_hooks();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_hooks_valid() {
        let config = Config {
            hooks: Some(HooksConfig {
                builtin: crate::components::BuiltinHooksTomlConfig {
                    test_filter: crate::components::HookConfig {
                        enabled: true,
                        pattern: Some("just_test*".to_string()),
                        priority: Some(10),
                    },
                    ..Default::default()
                },
            }),
            ..Config::default()
        };

        let result = config.validate_hooks();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_all_sections() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::TransportType::Stdio {
                        command: "npx".to_string(),
                        args: vec![],
                        env: std::collections::HashMap::new(),
                    },
                    prefix: None,
                    allowed_tools: None,
                    blocked_tools: None,
                    auto_reconnect: true,
                }],
            }),
            hooks: Some(HooksConfig::default()),
            ..Config::default()
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_discovery_empty_path() {
        use std::collections::HashMap;
        let mut type_configs = HashMap::new();
        type_configs.insert(
            "tools".to_string(),
            crate::components::TypeDiscoveryConfig {
                additional_paths: vec![std::path::PathBuf::from("")],
                use_defaults: true,
            },
        );

        let config = Config {
            discovery: Some(crate::components::DiscoveryPathsConfig {
                hooks: None,
                tools: None,
                events: None,
                type_configs,
            }),
            ..Config::default()
        };

        let result = config.validate_discovery();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_discovery_valid() {
        use std::collections::HashMap;
        let mut type_configs = HashMap::new();
        type_configs.insert(
            "tools".to_string(),
            crate::components::TypeDiscoveryConfig {
                additional_paths: vec![std::path::PathBuf::from("/valid/path")],
                use_defaults: true,
            },
        );

        let config = Config {
            discovery: Some(crate::components::DiscoveryPathsConfig {
                hooks: None,
                tools: None,
                events: None,
                type_configs,
            }),
            ..Config::default()
        };

        let result = config.validate_discovery();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_default_has_new_sections_none() {
        let config = Config::default();
        assert!(config.discovery.is_none());
        assert!(config.gateway.is_none());
        assert!(config.hooks.is_none());
    }

    #[test]
    fn test_config_accessor_methods() {
        let config = Config {
            discovery: Some(DiscoveryPathsConfig::default()),
            gateway: Some(GatewayConfig::default()),
            hooks: Some(HooksConfig::default()),
            ..Config::default()
        };

        assert!(config.discovery_config().is_some());
        assert!(config.gateway_config().is_some());
        assert!(config.hooks_config().is_some());
    }
}
