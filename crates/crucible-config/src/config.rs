//! Core configuration types and structures.

use crate::components::{
    AcpConfig, BackendType, ChatConfig, CliConfig, ContextConfig, DiscoveryPathsConfig,
    GatewayConfig, HandlersConfig, LlmConfig, McpConfig, PermissionConfig, StorageConfig,
};
use crate::includes::IncludeConfig;
use crate::{EnrichmentConfig, ProfileConfig};

#[cfg(feature = "toml")]
extern crate toml;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use tracing::{debug, error, info};

/// Returns the Crucible home directory (`~/.crucible/`).
///
/// This is the default location for session storage when no kiln is explicitly
/// specified. Uses `$CRUCIBLE_HOME` if set, otherwise `$HOME/.crucible/`.
///
/// # Panics
///
/// Returns a fallback path (`/tmp/.crucible`) if the home directory cannot
/// be determined (should never happen in practice).
pub fn crucible_home() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("CRUCIBLE_HOME") {
        return std::path::PathBuf::from(home);
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".crucible")
}

/// Check if a path is the crucible home directory.
///
/// Used by storage code to avoid double `.crucible/` nesting when the
/// persist kiln is the default crucible home.
pub fn is_crucible_home(path: &std::path::Path) -> bool {
    path == crucible_home()
}

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

/// Resolved LLM provider configuration
#[derive(Clone)]
pub struct EffectiveLlmConfig {
    /// Provider key (e.g., "local", "cloud", or "default" for fallback)
    pub key: String,
    /// Provider type
    pub provider_type: BackendType,
    /// API endpoint
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// Temperature
    pub temperature: f32,
    /// Maximum tokens
    pub max_tokens: u32,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// API key (if applicable)
    pub api_key: Option<String>,
}

impl std::fmt::Debug for EffectiveLlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectiveLlmConfig")
            .field("key", &self.key)
            .field("provider_type", &self.provider_type)
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("timeout_secs", &self.timeout_secs)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
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
    /// Include configuration for external files.
    ///
    /// Allows separating configuration into multiple files:
    /// ```toml
    /// [include]
    /// gateway = "mcps.toml"  # MCP server configurations
    /// discovery = "discovery.toml"  # Discovery paths
    /// ```
    ///
    /// Paths are relative to the main config file's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<IncludeConfig>,

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

    /// ACP (Agent Client Protocol) configuration.
    #[serde(default)]
    pub acp: Option<AcpConfig>,

    /// Chat configuration.
    #[serde(default)]
    pub chat: Option<ChatConfig>,

    /// LLM provider configuration with named instances.
    #[serde(default)]
    pub llm: Option<LlmConfig>,

    /// Server configuration.
    pub server: Option<ServerConfig>,

    /// Web UI server configuration.
    #[serde(default)]
    pub web: Option<WebConfig>,

    /// SCM (git) integration configuration.
    #[serde(default)]
    pub scm: Option<ScmConfig>,

    /// Logging configuration.
    pub logging: Option<LoggingConfig>,

    /// Discovery paths configuration.
    #[serde(default)]
    pub discovery: Option<DiscoveryPathsConfig>,

    /// Gateway configuration for upstream MCP servers (legacy alias for mcp).
    #[serde(default)]
    pub gateway: Option<GatewayConfig>,

    /// MCP upstream server configuration.
    #[serde(default)]
    pub mcp: Option<McpConfig>,

    /// Handlers configuration.
    #[serde(default)]
    pub handlers: Option<HandlersConfig>,

    /// Context configuration (project rules, etc.)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    /// Permission configuration for tool access control.
    #[serde(default)]
    pub permissions: Option<PermissionConfig>,

    /// Custom configuration values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            include: None,
            profile: Some("default".to_string()),
            profiles: HashMap::from([("default".to_string(), ProfileConfig::default())]),
            enrichment: None,
            database: None,
            cli: Some(CliConfig::default()),
            acp: Some(AcpConfig::default()),
            chat: Some(ChatConfig::default()),
            llm: None,
            server: None,
            web: None,
            scm: None,
            logging: None,
            discovery: None,
            gateway: None,
            mcp: None,
            handlers: None,
            context: None,
            permissions: None,
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

    /// Get the effective MCP configuration.
    pub fn mcp_config(&self) -> Option<&McpConfig> {
        self.mcp.as_ref()
    }

    /// Get the effective handlers configuration.
    pub fn handlers_config(&self) -> Option<&HandlersConfig> {
        self.handlers.as_ref()
    }

    /// Get the effective context configuration.
    pub fn context_config(&self) -> Option<&ContextConfig> {
        self.context.as_ref()
    }

    /// Get the effective LLM configuration.
    pub fn llm_config(&self) -> Option<&LlmConfig> {
        self.llm.as_ref()
    }

    /// Get the effective LLM provider for chat.
    pub fn effective_llm_provider(&self) -> Result<EffectiveLlmConfig, ConfigError> {
        if let Some(llm) = &self.llm {
            if let Some((key, provider)) = llm.default_provider() {
                return Ok(EffectiveLlmConfig {
                    key: key.clone(),
                    provider_type: provider.provider_type,
                    endpoint: provider.endpoint(),
                    model: provider.model(),
                    temperature: provider.temperature(),
                    max_tokens: provider.max_tokens(),
                    timeout_secs: provider.timeout_secs(),
                    api_key: provider.api_key(),
                });
            }
        }

        Err(ConfigError::MissingValue {
            field: "llm.default".to_string(),
        })
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

                match &server.transport {
                    crate::components::gateway::TransportType::Stdio { command, .. } => {
                        if command.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': stdio command cannot be empty",
                                server.name
                            ));
                        }
                    }
                    crate::components::gateway::TransportType::Sse { url, .. } => {
                        if url.is_empty() {
                            errors.push(format!(
                                "Gateway server '{}': SSE url cannot be empty",
                                server.name
                            ));
                        }
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

    /// Validate handlers configuration (checks pattern validity)
    pub fn validate_handlers(&self) -> Result<(), ConfigValidationError> {
        if let Some(handlers) = &self.handlers {
            let mut errors = Vec::new();

            // Validate patterns are valid glob patterns
            // For now, just check they're not empty if present
            let mut check_pattern = |name: &str, pattern: &Option<String>| {
                if let Some(p) = pattern {
                    if p.is_empty() {
                        errors.push(format!(
                            "Handler '{}': pattern cannot be empty string",
                            name
                        ));
                    }
                }
            };

            check_pattern("test_filter", &handlers.builtin.test_filter.pattern);
            check_pattern("toon_transform", &handlers.builtin.toon_transform.pattern);
            check_pattern(
                "recipe_enrichment",
                &handlers.builtin.recipe_enrichment.pattern,
            );
            check_pattern("tool_selector", &handlers.builtin.tool_selector.pattern);

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
            let mut check_type_config =
                |type_name: &str, config: &crate::components::TypeDiscoveryConfig| {
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

            if let Some(handlers) = &discovery.handlers {
                check_type_config("handlers", handlers);
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
        self.validate_handlers()?;
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

    /// ACP (Agent Client Protocol) configuration
    #[serde(default)]
    pub acp: AcpConfig,

    /// Chat configuration
    #[serde(default)]
    pub chat: ChatConfig,

    /// LLM provider configuration with named instances
    #[serde(default)]
    pub llm: LlmConfig,

    /// Enrichment configuration (embedding provider, pipeline settings)
    #[serde(default)]
    pub enrichment: Option<EnrichmentConfig>,

    /// CLI-specific configuration
    #[serde(default)]
    pub cli: CliConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    /// Processing configuration
    #[serde(default)]
    pub processing: ProcessingConfig,

    /// Context configuration (rules files, etc.)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    /// Storage configuration (embedded vs daemon mode)
    #[serde(default)]
    pub storage: Option<StorageConfig>,

    /// MCP server configuration (upstream servers, gateway settings)
    #[serde(default)]
    pub mcp: Option<McpConfig>,

    /// Permission configuration for tool access control.
    #[serde(default)]
    pub permissions: Option<PermissionConfig>,

    /// Per-plugin configuration sections (e.g. `[plugins.discord]`)
    #[serde(default)]
    pub plugins: HashMap<String, serde_json::Value>,

    /// Web UI server configuration
    #[serde(default)]
    pub web: Option<WebConfig>,

    /// Value source tracking for configuration provenance
    ///
    /// Tracks where each configuration value came from (file, environment, CLI, default).
    /// Populated during `load()` or `load_with_tracking()`.
    #[serde(skip)]
    pub source_map: Option<crate::value_source::ValueSourceMap>,
}

fn default_kiln_path() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

impl Default for CliAppConfig {
    fn default() -> Self {
        Self {
            kiln_path: default_kiln_path(),
            agent_directories: Vec::new(),
            acp: AcpConfig::default(),
            chat: ChatConfig::default(),
            llm: LlmConfig::default(),
            enrichment: None,
            cli: CliConfig::default(),
            logging: None,
            processing: ProcessingConfig::default(),
            context: None,
            storage: None,
            mcp: None,
            permissions: None,
            plugins: HashMap::new(),
            web: None,
            source_map: None,
        }
    }
}

impl CliAppConfig {
    /// Load CLI configuration from file with env var and CLI flag overrides
    ///
    /// Priority (highest to lowest):
    /// 1. CLI flags (--kiln-path, --embedding-url, --embedding-model)
    /// 2. Config file (~/.config/crucible/config.toml)
    /// 3. Default values
    ///
    /// Note: API keys are read from environment variables specified in config
    /// (e.g., `api_key = "OPENAI_API_KEY"`)
    ///
    /// This version also populates the `source_map` field to track where each
    /// configuration value came from. Use `--trace` with `cru config show` to
    /// display this information.
    pub fn load(
        config_file: Option<std::path::PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> anyhow::Result<Self> {
        use crate::value_source::{ValueSource, ValueSourceMap};

        // Determine config file path
        let config_path = config_file.unwrap_or_else(Self::default_config_path);

        debug!("Attempting to load config from: {}", config_path.display());

        let mut source_map = ValueSourceMap::new();
        let config_path_str = config_path.to_string_lossy().to_string();

        // Try to load config file or use defaults
        let (mut config, file_fields) = if config_path.exists() {
            info!("Found config file at: {}", config_path.display());

            let contents = std::fs::read_to_string(&config_path)
                .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

            #[cfg(feature = "toml")]
            {
                // First parse as a raw TOML table to detect which fields are present
                let raw_table: toml::Table = toml::from_str(&contents).map_err(|e| {
                    error!(
                        "Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    );
                    anyhow::anyhow!(
                        "Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    )
                })?;

                if raw_table.contains_key("embedding") {
                    return Err(anyhow::anyhow!(
                        "Failed to parse config file {}: legacy [embedding] is no longer supported. Use [llm.providers.<name>] with [llm].default",
                        config_path.display()
                    ));
                }
                if raw_table.contains_key("providers") {
                    return Err(anyhow::anyhow!(
                        "Failed to parse config file {}: legacy [providers] is no longer supported. Use [llm.providers.<name>] with [llm].default",
                        config_path.display()
                    ));
                }
                if let Some(toml::Value::Table(chat)) = raw_table.get("chat") {
                    if chat.contains_key("provider") {
                        return Err(anyhow::anyhow!(
                            "Failed to parse config file {}: chat.provider is no longer supported. Use [llm.providers.<name>] with [llm].default",
                            config_path.display()
                        ));
                    }
                }

                let file_fields = Self::detect_present_fields(&raw_table);

                match toml::from_str::<CliAppConfig>(&contents) {
                    Ok(cfg) => {
                        info!("Successfully loaded config file: {}", config_path.display());
                        (cfg, file_fields)
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
            (Self::default(), Vec::new())
        };

        // Track sources for all known fields
        let all_tracked_fields = [
            "kiln_path",
            "agent_directories",
            "llm.default",
            "acp.default_agent",
            "acp.enable_discovery",
            "acp.session_timeout_minutes",
            "acp.max_message_size_mb",
            "chat.model",
            "chat.enable_markdown",
            "chat.endpoint",
            "chat.temperature",
            "chat.max_tokens",
            "chat.timeout_secs",
            "cli.show_progress",
            "cli.confirm_destructive",
            "cli.verbose",
            "logging.level",
            "processing.parallel_workers",
        ];

        for field in &all_tracked_fields {
            if file_fields.contains(&(*field).to_string()) {
                source_map.set(
                    field,
                    ValueSource::File {
                        path: Some(config_path_str.clone()),
                    },
                );
            } else {
                source_map.set(field, ValueSource::Default);
            }
        }

        // Apply CLI flag overrides (priority 1 - highest)
        if let Some(url) = embedding_url {
            if let Some(default_key) = config.llm.default.clone() {
                if let Some(provider) = config.llm.providers.get_mut(&default_key) {
                    debug!(
                        "Overriding llm.providers.{}.endpoint from CLI flag: {}",
                        default_key, url
                    );
                    provider.endpoint = Some(url);
                    source_map.set("llm.default.endpoint", ValueSource::Cli);
                }
            }
        }
        if let Some(model) = embedding_model {
            if let Some(default_key) = config.llm.default.clone() {
                if let Some(provider) = config.llm.providers.get_mut(&default_key) {
                    debug!(
                        "Overriding llm.providers.{}.default_model from CLI flag: {}",
                        default_key, model
                    );
                    provider.default_model = Some(model);
                    source_map.set("llm.default.model", ValueSource::Cli);
                }
            }
        }

        config.source_map = Some(source_map);
        Ok(config)
    }

    /// Detect which fields are present in a TOML table
    #[cfg(feature = "toml")]
    fn detect_present_fields(table: &toml::Table) -> Vec<String> {
        let mut fields = Vec::new();

        // Top-level fields
        if table.contains_key("kiln_path") {
            fields.push("kiln_path".to_string());
        }
        if table.contains_key("agent_directories") {
            fields.push("agent_directories".to_string());
        }
        if let Some(toml::Value::Table(llm)) = table.get("llm") {
            if llm.contains_key("default") {
                fields.push("llm.default".to_string());
            }
        }

        // ACP section
        if let Some(toml::Value::Table(acp)) = table.get("acp") {
            if acp.contains_key("default_agent") {
                fields.push("acp.default_agent".to_string());
            }
            if acp.contains_key("enable_discovery") {
                fields.push("acp.enable_discovery".to_string());
            }
            if acp.contains_key("session_timeout_minutes") {
                fields.push("acp.session_timeout_minutes".to_string());
            }
            if acp.contains_key("max_message_size_mb") {
                fields.push("acp.max_message_size_mb".to_string());
            }
        }

        // Chat section
        if let Some(toml::Value::Table(chat)) = table.get("chat") {
            if chat.contains_key("model") {
                fields.push("chat.model".to_string());
            }
            if chat.contains_key("enable_markdown") {
                fields.push("chat.enable_markdown".to_string());
            }
            if chat.contains_key("endpoint") {
                fields.push("chat.endpoint".to_string());
            }
            if chat.contains_key("temperature") {
                fields.push("chat.temperature".to_string());
            }
            if chat.contains_key("max_tokens") {
                fields.push("chat.max_tokens".to_string());
            }
            if chat.contains_key("timeout_secs") {
                fields.push("chat.timeout_secs".to_string());
            }
        }

        // CLI section
        if let Some(toml::Value::Table(cli)) = table.get("cli") {
            if cli.contains_key("show_progress") {
                fields.push("cli.show_progress".to_string());
            }
            if cli.contains_key("confirm_destructive") {
                fields.push("cli.confirm_destructive".to_string());
            }
            if cli.contains_key("verbose") {
                fields.push("cli.verbose".to_string());
            }
        }

        // Logging section
        if let Some(toml::Value::Table(logging)) = table.get("logging") {
            if logging.contains_key("level") {
                fields.push("logging.level".to_string());
            }
        }

        // Processing section
        if let Some(toml::Value::Table(processing)) = table.get("processing") {
            if processing.contains_key("parallel_workers") {
                fields.push("processing.parallel_workers".to_string());
            }
        }

        fields
    }

    /// Log the effective configuration for debugging
    pub fn log_config(&self) {
        info!("Effective configuration:");
        info!("  kiln_path: {}", self.kiln_path.display());
        info!("  llm.default: {:?}", self.llm.default);
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

    /// Get database path for SurrealDB (always derived from kiln path)
    ///
    /// Note: This returns the SurrealDB-specific path. SQLite uses a different
    /// path (`crucible-sqlite.db`) computed in the storage factory.
    pub fn database_path(&self) -> std::path::PathBuf {
        // Only use PID suffix in test mode to prevent RocksDB lock collisions
        let db_name = if std::env::var("CRUCIBLE_TEST_MODE").is_ok() {
            let pid = std::process::id();
            format!("crucible-surreal-{}.db", pid)
        } else {
            "crucible-surreal.db".to_string()
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

    /// Get the source map, preferring the stored one if available
    fn get_source_map(&self) -> crate::value_source::ValueSourceMap {
        if let Some(ref map) = self.source_map {
            return map.clone();
        }
        // Fallback to heuristic for configs created without load()
        Self::build_fallback_source_map()
    }

    /// Build a fallback source map when no tracking data is available.
    /// Used when config is created with default() instead of load().
    fn build_fallback_source_map() -> crate::value_source::ValueSourceMap {
        use crate::value_source::{ValueSource, ValueSourceMap};

        let mut map = ValueSourceMap::new();
        let tracked_fields = [
            "kiln_path",
            "llm.default",
            "acp.default_agent",
            "acp.enable_discovery",
            "acp.session_timeout_minutes",
            "chat.model",
            "chat.enable_markdown",
            "cli.show_progress",
            "cli.confirm_destructive",
            "cli.verbose",
        ];

        for field in &tracked_fields {
            map.set(field, ValueSource::Default);
        }

        map
    }

    /// Display the current configuration as JSON with source tracking
    pub fn display_as_json_with_sources(&self) -> anyhow::Result<String> {
        use crate::value_source::ValueSource;

        let source_map = self.get_source_map();

        // Create a comprehensive output with sources for all tracked fields
        let mut output = serde_json::Map::new();

        // Helper to create a value item with source
        let make_item = |value: serde_json::Value, source: &ValueSource| -> serde_json::Value {
            let mut item = serde_json::Map::new();
            item.insert("value".to_string(), value);
            item.insert(
                "source".to_string(),
                serde_json::Value::String(source.detail()),
            );
            item.insert(
                "source_short".to_string(),
                serde_json::Value::String(source.short().to_string()),
            );
            serde_json::Value::Object(item)
        };

        // kiln_path
        let kiln_source = source_map.get("kiln_path").unwrap_or(&ValueSource::Default);
        output.insert(
            "kiln_path".to_string(),
            make_item(
                serde_json::Value::String(self.kiln_path.to_string_lossy().to_string()),
                kiln_source,
            ),
        );

        let llm_source = source_map
            .get("llm.default")
            .unwrap_or(&ValueSource::Default);
        let mut llm_section = serde_json::Map::new();
        if let Some(default_key) = &self.llm.default {
            llm_section.insert(
                "default".to_string(),
                make_item(serde_json::Value::String(default_key.clone()), llm_source),
            );
        }
        output.insert("llm".to_string(), serde_json::Value::Object(llm_section));

        // acp section
        let mut acp_section = serde_json::Map::new();
        if let Some(ref agent) = self.acp.default_agent {
            let agent_source = source_map
                .get("acp.default_agent")
                .unwrap_or(&ValueSource::Default);
            acp_section.insert(
                "default_agent".to_string(),
                make_item(serde_json::Value::String(agent.clone()), agent_source),
            );
        }

        let discovery_source = source_map
            .get("acp.enable_discovery")
            .unwrap_or(&ValueSource::Default);
        acp_section.insert(
            "enable_discovery".to_string(),
            make_item(
                serde_json::Value::Bool(self.acp.enable_discovery),
                discovery_source,
            ),
        );

        let timeout_source = source_map
            .get("acp.session_timeout_minutes")
            .unwrap_or(&ValueSource::Default);
        acp_section.insert(
            "session_timeout_minutes".to_string(),
            make_item(
                serde_json::Value::Number(self.acp.session_timeout_minutes.into()),
                timeout_source,
            ),
        );

        output.insert("acp".to_string(), serde_json::Value::Object(acp_section));

        // chat section
        let mut chat_section = serde_json::Map::new();
        if let Some(ref model) = self.chat.model {
            let model_source = source_map
                .get("chat.model")
                .unwrap_or(&ValueSource::Default);
            chat_section.insert(
                "model".to_string(),
                make_item(serde_json::Value::String(model.clone()), model_source),
            );
        }

        let markdown_source = source_map
            .get("chat.enable_markdown")
            .unwrap_or(&ValueSource::Default);
        chat_section.insert(
            "enable_markdown".to_string(),
            make_item(
                serde_json::Value::Bool(self.chat.enable_markdown),
                markdown_source,
            ),
        );

        output.insert("chat".to_string(), serde_json::Value::Object(chat_section));

        // cli section
        let mut cli_section = serde_json::Map::new();

        let progress_source = source_map
            .get("cli.show_progress")
            .unwrap_or(&ValueSource::Default);
        cli_section.insert(
            "show_progress".to_string(),
            make_item(
                serde_json::Value::Bool(self.cli.show_progress),
                progress_source,
            ),
        );

        let confirm_source = source_map
            .get("cli.confirm_destructive")
            .unwrap_or(&ValueSource::Default);
        cli_section.insert(
            "confirm_destructive".to_string(),
            make_item(
                serde_json::Value::Bool(self.cli.confirm_destructive),
                confirm_source,
            ),
        );

        let verbose_source = source_map
            .get("cli.verbose")
            .unwrap_or(&ValueSource::Default);
        cli_section.insert(
            "verbose".to_string(),
            make_item(serde_json::Value::Bool(self.cli.verbose), verbose_source),
        );

        output.insert("cli".to_string(), serde_json::Value::Object(cli_section));

        serde_json::to_string_pretty(&output)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config as JSON: {}", e))
    }

    /// Display the current configuration as TOML with source tracking
    pub fn display_as_toml_with_sources(&self) -> anyhow::Result<String> {
        use crate::value_source::ValueSource;

        let source_map = self.get_source_map();

        // Generate TOML with inline comments for sources
        let mut output = String::new();

        // Add header comment
        output.push_str("# Effective Configuration with Value Sources\n");
        output.push_str("# Sources: file (<path>), cli, env (<var>), default\n\n");

        // kiln_path
        let kiln_source = source_map.get("kiln_path").unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "kiln_path = \"{}\"  # from: {}\n",
            self.kiln_path.display(),
            kiln_source.detail()
        ));

        output.push_str("\n[llm]\n");
        let llm_source = source_map
            .get("llm.default")
            .unwrap_or(&ValueSource::Default);
        if let Some(default_key) = &self.llm.default {
            output.push_str(&format!(
                "default = \"{}\"  # from: {}\n",
                default_key,
                llm_source.detail()
            ));
        }

        // ACP section
        output.push_str("\n[acp]\n");
        if let Some(ref agent) = self.acp.default_agent {
            let agent_source = source_map
                .get("acp.default_agent")
                .unwrap_or(&ValueSource::Default);
            output.push_str(&format!(
                "default_agent = \"{}\"  # from: {}\n",
                agent,
                agent_source.detail()
            ));
        }

        let discovery_source = source_map
            .get("acp.enable_discovery")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "enable_discovery = {}  # from: {}\n",
            self.acp.enable_discovery,
            discovery_source.detail()
        ));

        let timeout_source = source_map
            .get("acp.session_timeout_minutes")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "session_timeout_minutes = {}  # from: {}\n",
            self.acp.session_timeout_minutes,
            timeout_source.detail()
        ));

        // Chat section
        output.push_str("\n[chat]\n");
        if let Some(ref model) = self.chat.model {
            let model_source = source_map
                .get("chat.model")
                .unwrap_or(&ValueSource::Default);
            output.push_str(&format!(
                "model = \"{}\"  # from: {}\n",
                model,
                model_source.detail()
            ));
        }

        let markdown_source = source_map
            .get("chat.enable_markdown")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "enable_markdown = {}  # from: {}\n",
            self.chat.enable_markdown,
            markdown_source.detail()
        ));

        // CLI section
        output.push_str("\n[cli]\n");
        let progress_source = source_map
            .get("cli.show_progress")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "show_progress = {}  # from: {}\n",
            self.cli.show_progress,
            progress_source.detail()
        ));

        let confirm_source = source_map
            .get("cli.confirm_destructive")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "confirm_destructive = {}  # from: {}\n",
            self.cli.confirm_destructive,
            confirm_source.detail()
        ));

        let verbose_source = source_map
            .get("cli.verbose")
            .unwrap_or(&ValueSource::Default);
        output.push_str(&format!(
            "verbose = {}  # from: {}\n",
            self.cli.verbose,
            verbose_source.detail()
        ));

        Ok(output)
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

# LLM provider configuration
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"

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
        crate::components::defaults::DEFAULT_TEMPERATURE
    }

    #[allow(missing_docs)]
    pub fn max_tokens(&self) -> u32 {
        crate::components::defaults::DEFAULT_CHAT_MAX_TOKENS
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
    ///
    /// Uses platform-appropriate directories:
    /// - Linux: `~/.config/crucible/config.toml` (XDG Base Directory)
    /// - macOS: `~/Library/Application Support/crucible/config.toml`
    /// - Windows: `%APPDATA%\crucible\config.toml` (Roaming AppData)
    pub fn default_config_path() -> std::path::PathBuf {
        // Allow overriding config directory via environment variable
        // This is crucial for test isolation and custom setups
        if let Ok(config_dir) = std::env::var("CRUCIBLE_CONFIG_DIR") {
            return std::path::PathBuf::from(config_dir).join("config.toml");
        }

        // Use platform-appropriate config directory
        // dirs::config_dir() returns:
        // - Windows: %APPDATA% (Roaming AppData)
        // - Linux: ~/.config (XDG Base Directory)
        // - macOS: ~/Library/Application Support
        if let Some(config_dir) = dirs::config_dir() {
            return config_dir.join("crucible").join("config.toml");
        }

        // Fallback: Use home directory with .config subdirectory
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

    /// Get the effective LLM provider for chat.
    pub fn effective_llm_provider(&self) -> Result<EffectiveLlmConfig, ConfigError> {
        if let Some((key, provider)) = self.llm.default_provider() {
            return Ok(EffectiveLlmConfig {
                key: key.clone(),
                provider_type: provider.provider_type,
                endpoint: provider.endpoint(),
                model: provider.model(),
                temperature: provider.temperature(),
                max_tokens: provider.max_tokens(),
                timeout_secs: provider.timeout_secs(),
                api_key: provider.api_key(),
            });
        }

        Err(ConfigError::MissingValue {
            field: "llm.default".to_string(),
        })
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    /// SQLite database.
    #[default]
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

/// Web UI server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Enable the web UI server.
    #[serde(default)]
    pub enabled: bool,

    /// Web server port.
    #[serde(default = "default_web_port")]
    pub port: u16,

    /// Web server host address.
    #[serde(default = "default_web_host")]
    pub host: String,

    /// Path to static web assets directory (optional, uses embedded assets if not set).
    #[serde(default)]
    pub static_dir: Option<String>,
}

fn default_web_port() -> u16 {
    3000
}

fn default_web_host() -> String {
    "127.0.0.1".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_web_port(),
            host: default_web_host(),
            static_dir: None,
        }
    }
}

/// SCM (git) integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScmConfig {
    /// Enable SCM detection for projects.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Detect and group git worktrees under their main repository.
    #[serde(default)]
    pub detect_worktrees: bool,
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
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_crucible_home_and_is_crucible_home() {
        // Test env override
        let tmp = std::env::temp_dir().join("crucible_test_home_combined");
        std::env::set_var("CRUCIBLE_HOME", &tmp);
        assert_eq!(crucible_home(), tmp);
        assert!(is_crucible_home(&tmp));
        assert!(!is_crucible_home(std::path::Path::new("/some/other/path")));
        std::env::remove_var("CRUCIBLE_HOME");
    }

    #[test]
    fn test_agent_directories_default_empty() {
        let config = CliAppConfig::default();
        assert!(config.agent_directories.is_empty());
    }

    #[test]
    fn test_agent_directories_loads_from_toml() {
        let kiln_path = test_path("test-kiln");
        let toml_content = format!(
            r#"
kiln_path = "{}"
agent_directories = ["/home/user/shared-agents", "./local-agents"]
"#,
            kiln_path.to_string_lossy().replace('\\', "\\\\")
        );
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
        let kiln_path = test_path("test-kiln");
        let toml_content = format!(
            r#"
kiln_path = "{}"
"#,
            kiln_path.to_string_lossy().replace('\\', "\\\\")
        );
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

[handlers.builtin.test_filter]
enabled = true
pattern = "just_test*"
priority = 10

[handlers.builtin.tool_selector]
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

        // Check handlers config
        assert!(config.handlers.is_some());
        let handlers = config.handlers.as_ref().unwrap();
        assert!(handlers.builtin.test_filter.enabled);
        assert!(handlers.builtin.tool_selector.enabled);
    }

    #[test]
    fn test_validate_gateway_empty_name() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::gateway::UpstreamServerConfig {
                    name: "".to_string(),
                    transport: crate::components::gateway::TransportType::Stdio {
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
                servers: vec![crate::components::gateway::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::gateway::TransportType::Sse {
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
                servers: vec![crate::components::gateway::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::gateway::TransportType::Sse {
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
    fn test_validate_handlers_empty_pattern() {
        let config = Config {
            handlers: Some(HandlersConfig {
                builtin: crate::components::BuiltinHandlersTomlConfig {
                    test_filter: crate::components::HandlerConfig {
                        enabled: true,
                        pattern: Some("".to_string()),
                        priority: Some(10),
                    },
                    ..Default::default()
                },
            }),
            ..Config::default()
        };

        let result = config.validate_handlers();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_handlers_valid() {
        let config = Config {
            handlers: Some(HandlersConfig {
                builtin: crate::components::BuiltinHandlersTomlConfig {
                    test_filter: crate::components::HandlerConfig {
                        enabled: true,
                        pattern: Some("just_test*".to_string()),
                        priority: Some(10),
                    },
                    ..Default::default()
                },
            }),
            ..Config::default()
        };

        let result = config.validate_handlers();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_all_sections() {
        let config = Config {
            gateway: Some(GatewayConfig {
                servers: vec![crate::components::gateway::UpstreamServerConfig {
                    name: "test".to_string(),
                    transport: crate::components::gateway::TransportType::Stdio {
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
            handlers: Some(HandlersConfig::default()),
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
                handlers: None,
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
                handlers: None,
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
        assert!(config.handlers.is_none());
    }

    #[test]
    fn test_config_accessor_methods() {
        let config = Config {
            discovery: Some(DiscoveryPathsConfig::default()),
            gateway: Some(GatewayConfig::default()),
            handlers: Some(HandlersConfig::default()),
            ..Config::default()
        };

        assert!(config.discovery_config().is_some());
        assert!(config.gateway_config().is_some());
        assert!(config.handlers_config().is_some());
    }

    #[test]
    fn test_effective_llm_provider_from_llm_config() {
        use std::collections::HashMap;
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            crate::components::LlmProviderConfig {
                provider_type: crate::components::BackendType::Ollama,
                endpoint: Some("http://192.168.1.100:11434".to_string()),
                default_model: Some("llama3.1:70b".to_string()),
                temperature: Some(0.9),
                max_tokens: Some(8192),
                timeout_secs: Some(300),
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );

        let config = Config {
            llm: Some(crate::components::LlmConfig {
                default: Some("local".to_string()),
                providers,
            }),
            ..Config::default()
        };

        let effective = config.effective_llm_provider().unwrap();
        assert_eq!(effective.key, "local");
        assert_eq!(effective.endpoint, "http://192.168.1.100:11434");
        assert_eq!(effective.model, "llama3.1:70b");
        assert_eq!(effective.temperature, 0.9);
        assert_eq!(effective.max_tokens, 8192);
        assert_eq!(effective.timeout_secs, 300);
    }

    #[test]
    fn test_effective_llm_provider_without_llm_default_returns_error() {
        let config = Config {
            llm: None,
            chat: Some(ChatConfig {
                model: Some("gpt-4o".to_string()),
                enable_markdown: true,
                agent_preference: crate::components::AgentPreference::default(),
                endpoint: Some("https://api.openai.com/v1".to_string()),
                temperature: Some(0.8),
                max_tokens: Some(4096),
                timeout_secs: Some(60),
                size_aware_prompts: true,
                show_thinking: false,
            }),
            ..Config::default()
        };

        let effective = config.effective_llm_provider();
        assert!(effective.is_err());
    }

    #[test]
    fn test_config_with_llm_section_from_toml() {
        let toml_content = r#"
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"
temperature = 0.7
timeout_secs = 120

[llm.providers.cloud]
type = "openai"
api_key = "OPENAI_API_KEY"
default_model = "gpt-4o"
temperature = 0.7
max_tokens = 4096
"#;

        let config: Config = toml::from_str(toml_content).unwrap();

        assert!(config.llm.is_some());
        let llm = config.llm.as_ref().unwrap();
        assert_eq!(llm.default, Some("local".to_string()));
        assert_eq!(llm.providers.len(), 2);

        let local = llm.get_provider("local").unwrap();
        assert_eq!(local.provider_type, crate::components::BackendType::Ollama);
        assert_eq!(local.model(), "llama3.2");

        let cloud = llm.get_provider("cloud").unwrap();
        assert_eq!(cloud.provider_type, crate::components::BackendType::OpenAI);
        assert_eq!(cloud.model(), "gpt-4o");
        assert_eq!(cloud.api_key, Some("OPENAI_API_KEY".to_string()));
    }

    #[test]
    fn test_cli_app_config_effective_llm_provider() {
        use std::collections::HashMap;
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            crate::components::LlmProviderConfig {
                provider_type: crate::components::BackendType::Ollama,
                endpoint: Some("http://localhost:11434".to_string()),
                default_model: Some("llama3.2".to_string()),
                temperature: Some(0.7),
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );

        let config = CliAppConfig {
            llm: crate::components::LlmConfig {
                default: Some("local".to_string()),
                providers,
            },
            ..Default::default()
        };

        let effective = config.effective_llm_provider().unwrap();
        assert_eq!(effective.key, "local");
        assert_eq!(effective.model, "llama3.2");
        assert_eq!(effective.temperature, 0.7);
    }

    #[test]
    fn test_cli_app_config_effective_llm_provider_missing_default_errors() {
        let config = CliAppConfig::default();
        let effective = config.effective_llm_provider();
        assert!(effective.is_err());
    }

    #[test]
    fn test_effective_llm_provider_requires_llm_default_provider() {
        let config = CliAppConfig {
            llm: crate::components::LlmConfig::default(),
            ..Default::default()
        };

        let effective = config.effective_llm_provider();
        assert!(
            effective.is_err(),
            "effective_llm_provider should fail without llm.default"
        );
    }

    #[test]
    fn test_cli_app_config_rejects_legacy_embedding_section() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[embedding]
provider = "fastembed"
"#;
        std::fs::write(temp.path(), toml_content).unwrap();

        let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
        assert!(
            parsed.is_err(),
            "legacy [embedding] config should be rejected"
        );
    }

    #[test]
    fn test_cli_app_config_rejects_legacy_providers_section() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[providers]
default_embedding = "legacy"

[providers.legacy]
backend = "ollama"
"#;
        std::fs::write(temp.path(), toml_content).unwrap();

        let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
        assert!(
            parsed.is_err(),
            "legacy [providers] config should be rejected"
        );
    }

    #[test]
    fn test_cli_app_config_loads_llm_provider_config() {
        let kiln_path = test_path("test");
        let toml = format!(
            r#"
kiln_path = "{}"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"
"#,
            kiln_path.to_string_lossy().replace('\\', "\\\\")
        );
        let config: CliAppConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.llm.default, Some("local".to_string()));
        let provider = config.llm.providers.get("local").unwrap();
        assert_eq!(
            provider.provider_type,
            crate::components::BackendType::Ollama
        );
        assert_eq!(provider.model(), "llama3.2");
    }

    #[test]
    fn test_cli_app_config_rejects_chat_provider_field() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[chat]
provider = "openai"
"#;
        std::fs::write(temp.path(), toml_content).unwrap();

        let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
        assert!(parsed.is_err(), "chat.provider should be rejected");
    }
}
