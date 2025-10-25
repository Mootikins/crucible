use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Compatibility embedding configuration for service layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: ProviderType,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub batch_size: u32,
}

/// Embedding provider types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    Ollama,
    Anthropic,
    Custom(String),
}

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Vault configuration
    pub vault: VaultConfig,
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
}

/// Vault configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Path to the vault directory
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
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
            vault: VaultConfig {
                path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                embedding_url: default_embedding_url(),
                embedding_model: None,
            },
            llm: LlmConfig::default(),
            network: NetworkConfig::default(),
            services: ServicesConfig::default(),
            migration: MigrationConfig::default(),
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
    /// Load configuration with precedence: defaults < file < env
    pub fn load(
        config_file: Option<PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> Result<Self> {
        // Start with defaults from config file (if exists)
        let mut config = Self::from_file_or_default(config_file)?;

        // Override with env vars (optional for testing/on-the-fly)
        if let Ok(path) = std::env::var("OBSIDIAN_VAULT_PATH") {
            config.vault.path = PathBuf::from(path);
        }

        if let Ok(url) = std::env::var("EMBEDDING_ENDPOINT") {
            config.vault.embedding_url = url;
        }
        if let Ok(model) = std::env::var("EMBEDDING_MODEL") {
            config.vault.embedding_model = Some(model);
        }

        // LLM environment variables
        if let Ok(model) = std::env::var("CRUCIBLE_CHAT_MODEL") {
            config.llm.chat_model = Some(model);
        }
        if let Ok(temp) = std::env::var("CRUCIBLE_TEMPERATURE") {
            config.llm.temperature = temp.parse().ok();
        }
        if let Ok(tokens) = std::env::var("CRUCIBLE_MAX_TOKENS") {
            config.llm.max_tokens = tokens.parse().ok();
        }
        if let Ok(prompt) = std::env::var("CRUCIBLE_SYSTEM_PROMPT") {
            config.llm.system_prompt = Some(prompt);
        }

        // Backend-specific environment variables
        if let Ok(endpoint) = std::env::var("OLLAMA_ENDPOINT") {
            config.llm.backends.ollama.endpoint = Some(endpoint);
        }
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            config.llm.backends.openai.api_key = Some(api_key);
        }
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            config.llm.backends.anthropic.api_key = Some(api_key);
        }

        // Network environment variables
        if let Ok(timeout) = std::env::var("CRUCIBLE_TIMEOUT") {
            config.network.timeout_secs = timeout.parse().ok();
        }

        // Override with CLI args (highest priority) - but only for non-vault-path options
        if let Some(url) = embedding_url {
            config.vault.embedding_url = url;
        }
        if let Some(model) = embedding_model {
            config.vault.embedding_model = Some(model);
        }

        Ok(config)
    }

    /// Get database path (always derived from vault path)
    pub fn database_path(&self) -> PathBuf {
        self.vault.path.join(".crucible/embeddings.db")
    }

    /// Get tools directory path (always derived from vault path)
    pub fn tools_path(&self) -> PathBuf {
        self.vault.path.join("tools")
    }

    /// Get database path as a string
    pub fn database_path_str(&self) -> Result<String> {
        self.database_path()
            .to_str()
            .map(|s| s.to_string())
            .context("Database path is not valid UTF-8")
    }

    /// Get vault path as a string
    pub fn vault_path_str(&self) -> Result<String> {
        self.vault
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

[vault]
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
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
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
        // Validate that embedding model is configured
        let model = self.vault.embedding_model.as_ref()
            .ok_or_else(|| anyhow::anyhow!(
                "Embedding model is not configured. Please set it via:\n\
                - Environment variable: EMBEDDING_MODEL\n\
                - CLI argument: --embedding-model <model>\n\
                - Config file: embedding_model = \"<model>\""
            ))?;

        // For now, we default to Ollama provider
        // In the future, we could add provider selection to the config
        Ok(EmbeddingConfig {
            provider: ProviderType::Ollama,
            endpoint: self.vault.embedding_url.clone(),
            api_key: None, // Not needed for Ollama
            model: model.clone(),
            timeout_secs: self.network.timeout_secs.unwrap_or(30),
            max_retries: self.network.max_retries.unwrap_or(3),
            batch_size: 1,
        })
    }

    /// Get resolved chat model (from config or default)
    pub fn chat_model(&self) -> String {
        self.llm.chat_model.clone()
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
        self.llm.system_prompt.clone()
            .unwrap_or_else(|| "You are a helpful assistant.".to_string())
    }

    /// Get resolved Ollama endpoint (from config or default)
    pub fn ollama_endpoint(&self) -> String {
        self.llm.backends.ollama.endpoint.clone()
            .unwrap_or_else(|| "https://llama.terminal.krohnos.io".to_string())
    }

    /// Get resolved timeout (from config or default)
    pub fn timeout(&self) -> u64 {
        self.network.timeout_secs.unwrap_or(30)
    }

    /// Get resolved OpenAI API key (from config or environment)
    pub fn openai_api_key(&self) -> Option<String> {
        self.llm.backends.openai.api_key.clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
    }

    /// Get resolved Anthropic API key (from config or environment)
    pub fn anthropic_api_key(&self) -> Option<String> {
        self.llm.backends.anthropic.api_key.clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
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
        assert_eq!(config.vault.embedding_url, "http://localhost:11434");

        // Should have no default model (None)
        assert_eq!(config.vault.embedding_model, None);

        // Should have default Ollama endpoint
        assert_eq!(config.ollama_endpoint(), "https://llama.terminal.krohnos.io");

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
    fn test_load_config_without_obsidian_vault_path() {
        // Clear environment variable to test default behavior
        std::env::remove_var("OBSIDIAN_VAULT_PATH");

        let result = CliConfig::load(None, None, None);
        assert!(result.is_ok());
        let config = result.unwrap();

        // Should use current directory as default when no env var is set
        assert!(config.vault.path.is_absolute() || config.vault.path.as_path() == std::path::Path::new("."));
    }

    #[test]
    fn test_load_config_with_explicit_url() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", "/tmp/test");

        let config = CliConfig::load(
            None,
            Some("https://example.com".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(config.vault.embedding_url, "https://example.com");

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
    }

    #[test]
    fn test_database_path_derivation() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault");

        // Set the required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", vault_path.to_str().unwrap());

        let config = CliConfig::load(None, None, None).unwrap();

        let expected_db = vault_path.join(".crucible/embeddings.db");
        assert_eq!(config.database_path(), expected_db);

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
    }

    #[test]
    fn test_tools_path_derivation() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault");

        // Set the required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", vault_path.to_str().unwrap());

        let config = CliConfig::load(None, None, None).unwrap();

        let expected_tools = vault_path.join("tools");
        assert_eq!(config.tools_path(), expected_tools);

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
    }

    #[test]
    fn test_create_example() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        CliConfig::create_example(&config_path).unwrap();

        assert!(config_path.exists());
        let contents = std::fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("Crucible CLI Configuration"));
        assert!(contents.contains("[vault]"));
    }

    #[test]
    fn test_display_as_toml() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", "/tmp/test");

        let config = CliConfig::load(None, None, None).unwrap();
        let toml_str = config.display_as_toml().unwrap();
        assert!(toml_str.contains("[vault]"));
        assert!(toml_str.contains("path"));
        assert!(toml_str.contains("embedding_url"));

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
    }

    #[test]
    fn test_display_as_json() {
        // Set the required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", "/tmp/test");

        let config = CliConfig::load(None, None, None).unwrap();
        let json_str = config.display_as_json().unwrap();
        assert!(json_str.contains("\"vault\""));
        assert!(json_str.contains("\"path\""));
        assert!(json_str.contains("\"embedding_url\""));

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
    }

    #[test]
    fn test_default_llm_config() {
        let config = CliConfig::default();

        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.max_tokens(), 2048);
        assert!(config.streaming());
        assert_eq!(config.system_prompt(), "You are a helpful assistant.");
        assert_eq!(config.ollama_endpoint(), "https://llama.terminal.krohnos.io");
        assert_eq!(config.timeout(), 30);
    }

    #[test]
    fn test_llm_config_from_file() {
        // Store original environment variables
        let original_chat_model = std::env::var("CRUCIBLE_CHAT_MODEL");
        let original_ollama_endpoint = std::env::var("OLLAMA_ENDPOINT");

        // Clear environment variables that might interfere
        std::env::remove_var("CRUCIBLE_CHAT_MODEL");
        std::env::remove_var("OLLAMA_ENDPOINT");
        std::env::remove_var("CRUCIBLE_TEST_MODE");

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let config_content = r#"
[vault]
path = "/tmp/test-vault"

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
        assert_eq!(config.ollama_endpoint(), "https://custom-ollama.example.com");
        assert_eq!(config.timeout(), 60);

        // Restore original environment variables
        if let Ok(val) = original_chat_model {
            std::env::set_var("CRUCIBLE_CHAT_MODEL", val);
        }
        if let Ok(val) = original_ollama_endpoint {
            std::env::set_var("OLLAMA_ENDPOINT", val);
        }
    }

    #[test]
    fn test_environment_variable_override() {
        // Store original environment variables
        let original_vault_path = std::env::var("OBSIDIAN_VAULT_PATH");
        let original_chat_model = std::env::var("CRUCIBLE_CHAT_MODEL");
        let original_temperature = std::env::var("CRUCIBLE_TEMPERATURE");
        let original_ollama_endpoint = std::env::var("OLLAMA_ENDPOINT");

        // Enable test mode to skip loading user config
        std::env::set_var("CRUCIBLE_TEST_MODE", "1");

        // Set environment variables
        std::env::set_var("OBSIDIAN_VAULT_PATH", "/tmp/env-test");
        std::env::set_var("CRUCIBLE_CHAT_MODEL", "env-model");
        std::env::set_var("CRUCIBLE_TEMPERATURE", "0.9");
        std::env::set_var("OLLAMA_ENDPOINT", "https://env-ollama.example.com");

        let config = CliConfig::load(None, None, None).unwrap();

        assert_eq!(config.vault.path, std::path::PathBuf::from("/tmp/env-test"));
        assert_eq!(config.chat_model(), "env-model");
        assert_eq!(config.temperature(), 0.9);
        assert_eq!(config.ollama_endpoint(), "https://env-ollama.example.com");

        // Restore original environment variables
        std::env::remove_var("CRUCIBLE_TEST_MODE");
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
        std::env::remove_var("CRUCIBLE_CHAT_MODEL");
        std::env::remove_var("CRUCIBLE_TEMPERATURE");
        std::env::remove_var("OLLAMA_ENDPOINT");

        if let Ok(val) = original_vault_path {
            std::env::set_var("OBSIDIAN_VAULT_PATH", val);
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
    fn test_api_key_from_environment() {
        // Set required environment variable
        std::env::set_var("OBSIDIAN_VAULT_PATH", "/tmp/test");
        std::env::set_var("OPENAI_API_KEY", "sk-test-openai");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-anthropic");

        let config = CliConfig::load(None, None, None).unwrap();

        assert_eq!(config.openai_api_key(), Some("sk-test-openai".to_string()));
        assert_eq!(config.anthropic_api_key(), Some("sk-ant-test-anthropic".to_string()));

        // Clean up
        std::env::remove_var("OBSIDIAN_VAULT_PATH");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
