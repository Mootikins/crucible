use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export canonical embedding config types for compatibility
pub use crucible_config::EmbeddingProviderConfig as EmbeddingConfig;
pub use crucible_config::EmbeddingProviderType as ProviderType;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Kiln configuration
    pub kiln: KilnConfig,
    /// LLM configuration
    #[serde(default)]
    pub llm: LlmConfig,
    /// Network configuration
    #[serde(default)]
    pub network: NetworkConfig,
    /// Service configuration
    #[serde(default)]
    pub services: ServicesConfig,
    /// Migration configuration
    #[serde(default)]
    pub migration: MigrationConfig,
    /// Custom database path (overrides default kiln/.crucible/embeddings.db)
    #[serde(skip)]
    pub custom_database_path: Option<PathBuf>,
}

/// Kiln configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KilnConfig {
    /// Path to the kiln directory
    pub path: PathBuf,

    /// Embedding service URL
    #[serde(default = "default_embedding_url")]
    pub embedding_url: String,

    /// Embedding model name
    pub embedding_model: Option<String>,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Request timeout in seconds
    pub timeout_secs: Option<u64>,

    /// Connection pool size
    pub pool_size: Option<usize>,

    /// Enable request retries
    pub max_retries: Option<u32>,
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default model for chat
    pub chat_model: Option<String>,

    /// Default temperature for chat
    pub temperature: Option<f32>,

    /// Default max tokens for chat
    pub max_tokens: Option<u32>,

    /// Enable streaming responses
    pub streaming: Option<bool>,

    /// Default system prompt
    pub system_prompt: Option<String>,

    /// Backend-specific configurations
    #[serde(default)]
    pub backends: BackendConfigs,
}

/// Backend-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendConfigs {
    /// Ollama configuration
    #[serde(default)]
    pub ollama: OllamaConfig,

    /// OpenAI configuration
    #[serde(default)]
    pub openai: OpenAIConfig,

    /// Anthropic configuration
    #[serde(default)]
    pub anthropic: AnthropicConfig,
}

/// Ollama backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Ollama endpoint URL
    pub endpoint: Option<String>,

    /// Auto-discover models
    pub auto_discover: Option<bool>,
}

/// OpenAI backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// OpenAI API endpoint
    pub endpoint: Option<String>,

    /// API key (can also be set via environment)
    pub api_key: Option<String>,
}

/// Anthropic backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Anthropic API endpoint
    pub endpoint: Option<String>,

    /// API key (can also be set via environment)
    pub api_key: Option<String>,
}

/// Services configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    /// ScriptEngine service configuration
    #[serde(default)]
    pub script_engine: ScriptEngineConfig,
    /// Service discovery configuration
    #[serde(default)]
    pub discovery: ServiceDiscoveryConfig,
    /// Service health monitoring configuration
    #[serde(default)]
    pub health: ServiceHealthConfig,
}

/// ScriptEngine service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptEngineConfig {
    /// Enable ScriptEngine service
    pub enabled: bool,
    /// Security level for script execution
    pub security_level: String,
    /// Maximum script source size in bytes
    pub max_source_size: usize,
    /// Default execution timeout in seconds
    pub default_timeout_secs: u64,
    /// Enable script caching
    pub enable_caching: bool,
    /// Maximum number of cached scripts
    pub max_cache_size: usize,
    /// Maximum memory usage in MB
    pub max_memory_mb: usize,
    /// Maximum CPU percentage
    pub max_cpu_percentage: f32,
    /// Maximum concurrent operations
    pub max_concurrent_operations: usize,
}

/// Service discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryConfig {
    /// Enable service discovery
    pub enabled: bool,
    /// Discovery endpoints
    pub endpoints: Vec<String>,
    /// Discovery timeout in seconds
    pub timeout_secs: u64,
    /// Refresh interval in seconds
    pub refresh_interval_secs: u64,
}

/// Service health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealthConfig {
    /// Enable health monitoring
    pub enabled: bool,
    /// Health check interval in seconds
    pub check_interval_secs: u64,
    /// Health check timeout in seconds
    pub timeout_secs: u64,
    /// Number of consecutive failures before marking as unhealthy
    pub failure_threshold: u32,
    /// Enable automatic recovery
    pub auto_recovery: bool,
}

/// Migration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Enable migration features
    pub enabled: bool,
    /// Default security level for migrated tools
    pub default_security_level: String,
    /// Enable automatic migration
    pub auto_migrate: bool,
    /// Enable caching of migrated tools
    pub enable_caching: bool,
    /// Maximum number of cached migrated tools
    pub max_cache_size: usize,
    /// Preserve original tool IDs during migration
    pub preserve_tool_ids: bool,
    /// Backup original tools before migration
    pub backup_originals: bool,
    /// Migration validation settings
    #[serde(default)]
    pub validation: MigrationValidationConfig,
}

/// Migration validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationValidationConfig {
    /// Enable automatic validation after migration
    pub auto_validate: bool,
    /// Strict validation mode (fail on any issue)
    pub strict: bool,
    /// Validate tool functionality
    pub validate_functionality: bool,
    /// Validate performance characteristics
    pub validate_performance: bool,
    /// Maximum performance degradation percentage
    pub max_performance_degradation: f32,
}

/// Default configuration constants
impl Default for CliConfig {
    fn default() -> Self {
        Self {
            kiln: KilnConfig {
                path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                embedding_url: default_embedding_url(),
                embedding_model: None,
            },
            llm: LlmConfig::default(),
            network: NetworkConfig::default(),
            services: ServicesConfig::default(),
            migration: MigrationConfig::default(),
            custom_database_path: None,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            timeout_secs: Some(30),
            pool_size: Some(10),
            max_retries: Some(3),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            chat_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(2048),
            streaming: Some(true),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            backends: BackendConfigs::default(),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            endpoint: Some("https://llama.terminal.krohnos.io".to_string()),
            auto_discover: Some(true),
        }
    }
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            endpoint: Some("https://api.openai.com/v1".to_string()),
            api_key: None,
        }
    }
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            endpoint: Some("https://api.anthropic.com".to_string()),
            api_key: None,
        }
    }
}

impl Default for ServicesConfig {
    fn default() -> Self {
        Self {
            script_engine: ScriptEngineConfig::default(),
            discovery: ServiceDiscoveryConfig::default(),
            health: ServiceHealthConfig::default(),
        }
    }
}

impl Default for ScriptEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            security_level: "safe".to_string(),
            max_source_size: 1024 * 1024, // 1MB
            default_timeout_secs: 30,
            enable_caching: true,
            max_cache_size: 1000,
            max_memory_mb: 100,
            max_cpu_percentage: 80.0,
            max_concurrent_operations: 50,
        }
    }
}

impl Default for ServiceDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoints: vec!["localhost:8080".to_string()],
            timeout_secs: 5,
            refresh_interval_secs: 30,
        }
    }
}

impl Default for ServiceHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 10,
            timeout_secs: 5,
            failure_threshold: 3,
            auto_recovery: true,
        }
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_security_level: "safe".to_string(),
            auto_migrate: false,
            enable_caching: true,
            max_cache_size: 500,
            preserve_tool_ids: true,
            backup_originals: true,
            validation: MigrationValidationConfig::default(),
        }
    }
}

impl Default for MigrationValidationConfig {
    fn default() -> Self {
        Self {
            auto_validate: true,
            strict: false,
            validate_functionality: true,
            validate_performance: false,
            max_performance_degradation: 20.0, // 20%
        }
    }
}

fn default_embedding_url() -> String {
    "http://localhost:11434".to_string()
}

impl CliConfig {
    /// Load configuration with precedence: defaults < file < CLI args
    ///
    /// **NOTE:** Environment variable configuration has been removed in v0.2.0.
    /// Use config files or CLI arguments for configuration.
    pub fn load(
        config_file: Option<PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> Result<Self> {
        // Load from config file (if exists), otherwise use defaults
        let mut config = Self::from_file_or_default(config_file)?;

        // Override with CLI args (highest priority)
        if let Some(url) = embedding_url {
            config.kiln.embedding_url = url;
        }
        if let Some(model) = embedding_model {
            config.kiln.embedding_model = Some(model);
        }

        Ok(config)
    }

    /// Create a builder for programmatically constructing config (useful for tests)
    pub fn builder() -> CliConfigBuilder {
        CliConfigBuilder::new()
    }

    /// Get database path (always derived from kiln path)
    pub fn database_path(&self) -> PathBuf {
        if let Some(custom_path) = &self.custom_database_path {
            custom_path.clone()
        } else {
            self.kiln.path.join(".crucible/embeddings.db")
        }
    }

    /// Get tools directory path (always derived from kiln path)
    pub fn tools_path(&self) -> PathBuf {
        self.kiln.path.join("tools")
    }

    /// Get database path as a string
    pub fn database_path_str(&self) -> Result<String> {
        self.database_path()
            .to_str()
            .map(|s| s.to_string())
            .context("Database path is not valid UTF-8")
    }

    /// Get kiln path as a string
    pub fn kiln_path_str(&self) -> Result<String> {
        self.kiln
            .path
            .to_str()
            .map(|s| s.to_string())
            .context("Kiln path is not valid UTF-8")
    }

    /// Get default config file path
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("crucible");
        Ok(config_dir.join("config.toml"))
    }

    /// Create a new config file with example values
    pub fn create_example(path: &PathBuf) -> Result<()> {
        let example = r#"# Crucible CLI Configuration
# Location: ~/.config/crucible/config.toml

[kiln]
# Path to your Obsidian kiln
# Default: current directory
path = "/home/user/Documents/my-kiln"

# Embedding service endpoint
# Default: http://localhost:11434 (local Ollama)
# For remote Ollama: https://your-server.com
embedding_url = "http://localhost:11434"

# Embedding model name (required)
# Options: nomic-embed-text-v1.5-q8_0, nomic-embed-text-v2-moe-q4_k_m, all-minilm-l6-v2, etc.
# embedding_model = "nomic-embed-text-v1.5-q8_0"

[network]
# Request timeout in seconds
timeout_secs = 30

# Connection pool size
pool_size = 10

# Maximum retry attempts
max_retries = 3

[llm]
# Default chat model (can be overridden by agents)
chat_model = "llama3.2"

# Default temperature (0.0-2.0, lower = more deterministic)
temperature = 0.7

# Default maximum tokens in responses
max_tokens = 2048

# Enable streaming responses
streaming = true

# Default system prompt for chat
system_prompt = "You are a helpful assistant."

[llm.backends.ollama]
# Ollama endpoint URL
endpoint = "https://llama.terminal.krohnos.io"

# Auto-discover available models
auto_discover = true

[llm.backends.openai]
# OpenAI API endpoint
endpoint = "https://api.openai.com/v1"

# API key (can also be set via OPENAI_API_KEY env var)
# api_key = "sk-..."

[llm.backends.anthropic]
# Anthropic API endpoint
endpoint = "https://api.anthropic.com"

# API key (can also be set via ANTHROPIC_API_KEY env var)
# api_key = "sk-ant-..."

# Note: The following are automatically derived from kiln path:
#   - Database: {kiln}/.crucible/embeddings.db
#   - Tools: {kiln}/tools/

[services]
# ScriptEngine service configuration
[services.script_engine]
# Enable ScriptEngine service for tool execution
enabled = true

# Security level for script execution (safe, development, production)
security_level = "safe"

# Maximum script source size in bytes (1MB default)
max_source_size = 1048576

# Default execution timeout in seconds
default_timeout_secs = 30

# Enable script caching for performance
enable_caching = true

# Maximum number of cached scripts
max_cache_size = 1000

# Maximum memory usage per script execution (MB)
max_memory_mb = 100

# Maximum CPU percentage per script
max_cpu_percentage = 80.0

# Maximum concurrent script executions
max_concurrent_operations = 50

# Service discovery configuration
[services.discovery]
# Enable automatic service discovery
enabled = true

# Service discovery endpoints
endpoints = ["localhost:8080"]

# Discovery timeout in seconds
timeout_secs = 5

# Service discovery refresh interval in seconds
refresh_interval_secs = 30

# Service health monitoring configuration
[services.health]
# Enable health monitoring for services
enabled = true

# Health check interval in seconds
check_interval_secs = 10

# Health check timeout in seconds
timeout_secs = 5

# Number of consecutive failures before marking as unhealthy
failure_threshold = 3

# Enable automatic recovery for unhealthy services
auto_recovery = true

# Migration configuration
[migration]
# Enable migration features for tool migration
enabled = true

# Default security level for migrated tools
default_security_level = "safe"

# Enable automatic migration of discovered tools
auto_migrate = false

# Enable caching of migrated tools
enable_caching = true

# Maximum number of cached migrated tools
max_cache_size = 500

# Preserve original tool IDs during migration
preserve_tool_ids = true

# Backup original tools before migration
backup_originals = true

# Migration validation settings
[migration.validation]
# Enable automatic validation after migration
auto_validate = true

# Strict validation mode (fail on any issue)
strict = false

# Validate tool functionality
validate_functionality = true

# Validate performance characteristics
validate_performance = false

# Maximum performance degradation percentage (20% default)
max_performance_degradation = 20.0
"#;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        std::fs::write(path, example).context("Failed to write config file")?;

        Ok(())
    }

    /// Load config from file or return default
    pub fn from_file_or_default(config_file: Option<PathBuf>) -> Result<Self> {
        // Check for test mode environment variable to skip loading user config
        if std::env::var("CRUCIBLE_TEST_MODE").is_ok() {
            return Ok(Self::default());
        }

        let path = config_file
            .or_else(|| Self::default_config_path().ok())
            .and_then(|p| if p.exists() { Some(p) } else { None });

        if let Some(path) = path {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            toml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file: {}", path.display()))
        } else {
            // Default configuration
            Ok(Self::default())
        }
    }

    /// Display the current configuration as TOML
    pub fn display_as_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize config as TOML")
    }

    /// Display the current configuration as JSON
    pub fn display_as_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize config as JSON")
    }

    /// Convert to EmbeddingConfig for use with create_provider
    pub fn to_embedding_config(&self) -> Result<EmbeddingConfig> {
        // Check if we're in test mode or mock provider requested
        if std::env::var("CRUCIBLE_TEST_MODE").is_ok()
            || self.kiln.embedding_model.as_ref().map(|m| m.as_str()) == Some("mock")
            || self.kiln.embedding_model.as_ref().map(|m| m.as_str()) == Some("mock-test-model") {
            return Ok(EmbeddingConfig::mock());
        }

        // Validate that embedding model is configured
        let model = self.kiln.embedding_model.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Embedding model is not configured. Please set it via:\n\
                - Environment variable: EMBEDDING_MODEL\n\
                - CLI argument: --embedding-model <model>\n\
                - Config file: embedding_model = \"<model>\""
            )
        })?;

        // Default to Ollama provider
        // In the future, we could add provider selection to the config
        Ok(EmbeddingConfig::ollama(
            Some(self.kiln.embedding_url.clone()),
            Some(model.clone()),
        ))
    }

    /// Get resolved chat model (from config or default)
    pub fn chat_model(&self) -> String {
        self.llm
            .chat_model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string())
    }

    /// Get resolved temperature (from config or default)
    pub fn temperature(&self) -> f32 {
        self.llm.temperature.unwrap_or(0.7)
    }

    /// Get resolved max tokens (from config or default)
    pub fn max_tokens(&self) -> u32 {
        self.llm.max_tokens.unwrap_or(2048)
    }

    /// Get resolved streaming setting (from config or default)
    pub fn streaming(&self) -> bool {
        self.llm.streaming.unwrap_or(true)
    }

    /// Get resolved system prompt (from config or default)
    pub fn system_prompt(&self) -> String {
        self.llm
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful assistant.".to_string())
    }

    /// Get resolved Ollama endpoint (from config or default)
    pub fn ollama_endpoint(&self) -> String {
        self.llm
            .backends
            .ollama
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://llama.terminal.krohnos.io".to_string())
    }

    /// Get resolved timeout (from config or default)
    pub fn timeout(&self) -> u64 {
        self.network.timeout_secs.unwrap_or(30)
    }

    /// Get resolved OpenAI API key (from config or environment)
    pub fn openai_api_key(&self) -> Option<String> {
        self.llm
            .backends
            .openai
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
    }

    /// Get resolved Anthropic API key (from config or environment)
    pub fn anthropic_api_key(&self) -> Option<String> {
        self.llm
            .backends
            .anthropic
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
    }
}

/// Builder for programmatically constructing CliConfig (useful for tests and programmatic configuration)
pub struct CliConfigBuilder {
    kiln_path: Option<PathBuf>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
    chat_model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    streaming: Option<bool>,
    system_prompt: Option<String>,
    ollama_endpoint: Option<String>,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    timeout_secs: Option<u64>,
    custom_database_path: Option<PathBuf>,
}

impl CliConfigBuilder {
    /// Create a new builder with defaults
    pub fn new() -> Self {
        Self {
            kiln_path: None,
            embedding_url: None,
            embedding_model: None,
            chat_model: None,
            temperature: None,
            max_tokens: None,
            streaming: None,
            system_prompt: None,
            ollama_endpoint: None,
            openai_api_key: None,
            anthropic_api_key: None,
            timeout_secs: None,
            custom_database_path: None,
        }
    }

    /// Set kiln path
    pub fn kiln_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.kiln_path = Some(path.into());
        self
    }

    /// Set embedding URL
    pub fn embedding_url<S: Into<String>>(mut self, url: S) -> Self {
        self.embedding_url = Some(url.into());
        self
    }

    /// Set embedding model
    pub fn embedding_model<S: Into<String>>(mut self, model: S) -> Self {
        self.embedding_model = Some(model.into());
        self
    }

    /// Set chat model
    pub fn chat_model<S: Into<String>>(mut self, model: S) -> Self {
        self.chat_model = Some(model.into());
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set streaming
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = Some(streaming);
        self
    }

    /// Set system prompt
    pub fn system_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set Ollama endpoint
    pub fn ollama_endpoint<S: Into<String>>(mut self, endpoint: S) -> Self {
        self.ollama_endpoint = Some(endpoint.into());
        self
    }

    /// Set OpenAI API key
    pub fn openai_api_key<S: Into<String>>(mut self, key: S) -> Self {
        self.openai_api_key = Some(key.into());
        self
    }

    /// Set Anthropic API key
    pub fn anthropic_api_key<S: Into<String>>(mut self, key: S) -> Self {
        self.anthropic_api_key = Some(key.into());
        self
    }

    /// Set timeout in seconds
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Set custom database path
    pub fn database_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.custom_database_path = Some(path.into());
        self
    }

    /// Build the CliConfig
    pub fn build(self) -> Result<CliConfig> {
        let mut config = CliConfig::default();

        // Set kiln path (required)
        if let Some(path) = self.kiln_path {
            config.kiln.path = path;
        }

        // Set embedding configuration
        if let Some(url) = self.embedding_url {
            config.kiln.embedding_url = url;
        }
        if let Some(model) = self.embedding_model {
            config.kiln.embedding_model = Some(model);
        }

        // Set LLM configuration
        if let Some(model) = self.chat_model {
            config.llm.chat_model = Some(model);
        }
        if let Some(temp) = self.temperature {
            config.llm.temperature = Some(temp);
        }
        if let Some(tokens) = self.max_tokens {
            config.llm.max_tokens = Some(tokens);
        }
        if let Some(streaming) = self.streaming {
            config.llm.streaming = Some(streaming);
        }
        if let Some(prompt) = self.system_prompt {
            config.llm.system_prompt = Some(prompt);
        }

        // Set backend configuration
        if let Some(endpoint) = self.ollama_endpoint {
            config.llm.backends.ollama.endpoint = Some(endpoint);
        }
        if let Some(key) = self.openai_api_key {
            config.llm.backends.openai.api_key = Some(key);
        }
        if let Some(key) = self.anthropic_api_key {
            config.llm.backends.anthropic.api_key = Some(key);
        }

        // Set network configuration
        if let Some(timeout) = self.timeout_secs {
            config.network.timeout_secs = Some(timeout);
        }

        // Set custom database path
        if let Some(path) = self.custom_database_path {
            config.custom_database_path = Some(path);
        }

        Ok(config)
    }
}

impl Default for CliConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_with_defaults() {
        // Store original environment variables
        let original_embedding_endpoint = std::env::var("EMBEDDING_ENDPOINT");
        let original_embedding_model = std::env::var("EMBEDDING_MODEL");
        let original_chat_model = std::env::var("CRUCIBLE_CHAT_MODEL");
        let original_temperature = std::env::var("CRUCIBLE_TEMPERATURE");
        let original_ollama_endpoint = std::env::var("OLLAMA_ENDPOINT");

        // Clear environment variables that might interfere
        std::env::remove_var("EMBEDDING_ENDPOINT");
        std::env::remove_var("EMBEDDING_MODEL");
        std::env::remove_var("CRUCIBLE_CHAT_MODEL");
        std::env::remove_var("CRUCIBLE_TEMPERATURE");
        std::env::remove_var("OLLAMA_ENDPOINT");

        // Enable test mode to skip loading user config
        std::env::set_var("CRUCIBLE_TEST_MODE", "1");

        let config = CliConfig::load(None, None, None).unwrap();

        // Should have default LLM settings
        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.max_tokens(), 2048);

        // Should have default embedding URL
        assert_eq!(config.kiln.embedding_url, "http://localhost:11434");

        // Should have no default model (None)
        assert_eq!(config.kiln.embedding_model, None);

        // Should have default Ollama endpoint
        assert_eq!(
            config.ollama_endpoint(),
            "https://llama.terminal.krohnos.io"
        );

        // Restore original environment variables
        std::env::remove_var("CRUCIBLE_TEST_MODE");

        if let Ok(val) = original_embedding_endpoint {
            std::env::set_var("EMBEDDING_ENDPOINT", val);
        }
        if let Ok(val) = original_embedding_model {
            std::env::set_var("EMBEDDING_MODEL", val);
        }
        if let Ok(val) = original_chat_model {
            std::env::set_var("CRUCIBLE_CHAT_MODEL", val);
        }
        if let Ok(val) = original_temperature {
            std::env::set_var("CRUCIBLE_TEMPERATURE", val);
        }
        if let Ok(val) = original_ollama_endpoint {
            std::env::set_var("OLLAMA_ENDPOINT", val);
        }
    }

    #[test]
    fn test_load_config_without_obsidian_kiln_path() {
        // Clear environment variable to test default behavior
        std::env::remove_var("OBSIDIAN_KILN_PATH");

        let result = CliConfig::load(None, None, None);
        assert!(result.is_ok());
        let config = result.unwrap();

        // Should use current directory as default when no env var is set
        assert!(
            config.kiln.path.is_absolute()
                || config.kiln.path.as_path() == std::path::Path::new(".")
        );
    }

    #[test]
    fn test_load_config_with_explicit_url() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_KILN_PATH", "/tmp/test");

        let config = CliConfig::load(None, Some("https://example.com".to_string()), None).unwrap();

        assert_eq!(config.kiln.embedding_url, "https://example.com");

        // Clean up
        std::env::remove_var("OBSIDIAN_KILN_PATH");
    }

    #[test]
    fn test_database_path_derivation() {
        let temp = TempDir::new().unwrap();
        let kiln_path = temp.path().join("kiln");

        // Use builder to create config with explicit kiln path
        let config = CliConfig::builder()
            .kiln_path(&kiln_path)
            .build()
            .unwrap();

        let expected_db = kiln_path.join(".crucible/embeddings.db");

        // The config should use the path we set via builder
        assert_eq!(&config.kiln.path, &kiln_path, "Config kiln path should match builder");
        assert_eq!(config.database_path(), expected_db);
    }

    #[test]
    fn test_tools_path_derivation() {
        let temp = TempDir::new().unwrap();
        let kiln_path = temp.path().join("kiln");

        // Use builder to create config with explicit kiln path
        let config = CliConfig::builder()
            .kiln_path(&kiln_path)
            .build()
            .unwrap();

        let expected_tools = kiln_path.join("tools");

        // The config should use the path we set via builder
        assert_eq!(&config.kiln.path, &kiln_path, "Config kiln path should match builder");
        assert_eq!(config.tools_path(), expected_tools);
    }

    #[test]
    fn test_create_example() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        CliConfig::create_example(&config_path).unwrap();

        assert!(config_path.exists());
        let contents = std::fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("Crucible CLI Configuration"));
        assert!(contents.contains("[kiln]"));
    }

    #[test]
    fn test_display_as_toml() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_KILN_PATH", "/tmp/test");

        let config = CliConfig::load(None, None, None).unwrap();
        let toml_str = config.display_as_toml().unwrap();
        assert!(toml_str.contains("[kiln]"));
        assert!(toml_str.contains("path"));
        assert!(toml_str.contains("embedding_url"));

        // Clean up
        std::env::remove_var("OBSIDIAN_KILN_PATH");
    }

    #[test]
    fn test_display_as_json() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_KILN_PATH", "/tmp/test");

        let config = CliConfig::load(None, None, None).unwrap();
        let json_str = config.display_as_json().unwrap();
        assert!(json_str.contains("\"kiln\""));
        assert!(json_str.contains("\"path\""));
        assert!(json_str.contains("\"embedding_url\""));

        // Clean up
        std::env::remove_var("OBSIDIAN_KILN_PATH");
    }

    #[test]
    fn test_default_llm_config() {
        let config = CliConfig::default();

        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.max_tokens(), 2048);
        assert!(config.streaming());
        assert_eq!(config.system_prompt(), "You are a helpful assistant.");
        assert_eq!(
            config.ollama_endpoint(),
            "https://llama.terminal.krohnos.io"
        );
        assert_eq!(config.timeout(), 30);
    }

    #[test]
    fn test_llm_config_from_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let config_content = r#"
[kiln]
path = "/tmp/test-kiln"

[llm]
chat_model = "custom-model"
temperature = 0.5
max_tokens = 1024
streaming = false
system_prompt = "Custom prompt"

[llm.backends.ollama]
endpoint = "https://custom-ollama.example.com"
auto_discover = false

[network]
timeout_secs = 60
"#;

        std::fs::write(&config_path, config_content).unwrap();

        let config = CliConfig::load(Some(config_path), None, None).unwrap();

        assert_eq!(config.chat_model(), "custom-model");
        assert_eq!(config.temperature(), 0.5);
        assert_eq!(config.max_tokens(), 1024);
        assert!(!config.streaming());
        assert_eq!(config.system_prompt(), "Custom prompt");
        assert_eq!(
            config.ollama_endpoint(),
            "https://custom-ollama.example.com"
        );
        assert_eq!(config.timeout(), 60);
    }

    #[test]
    fn test_builder_override() {
        // Test that builder can override all config values programmatically
        let config = CliConfig::builder()
            .kiln_path("/tmp/builder-test")
            .chat_model("builder-model")
            .temperature(0.9)
            .ollama_endpoint("https://builder-ollama.example.com")
            .build()
            .unwrap();

        assert_eq!(config.kiln.path, std::path::PathBuf::from("/tmp/builder-test"));
        assert_eq!(config.chat_model(), "builder-model");
        assert_eq!(config.temperature(), 0.9);
        assert_eq!(config.ollama_endpoint(), "https://builder-ollama.example.com");
    }

    #[test]
    fn test_api_key_from_environment() {
        // Set required environment variable
        std::env::set_var("OBSIDIAN_KILN_PATH", "/tmp/test");
        std::env::set_var("OPENAI_API_KEY", "sk-test-openai");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-anthropic");

        let config = CliConfig::load(None, None, None).unwrap();

        assert_eq!(config.openai_api_key(), Some("sk-test-openai".to_string()));
        assert_eq!(
            config.anthropic_api_key(),
            Some("sk-ant-test-anthropic".to_string())
        );

        // Clean up
        std::env::remove_var("OBSIDIAN_KILN_PATH");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
