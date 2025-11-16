//! Configuration loading utilities for various file formats.

use crate::{Config, ConfigError};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Configuration loader supporting multiple file formats.
#[derive(Debug)]
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
    format: ConfigFormat,
}

impl ConfigLoader {
    /// Create a new configuration loader.
    pub fn new() -> Self {
        Self {
            search_paths: vec![
                PathBuf::from("./config"),
                PathBuf::from("./"),
                PathBuf::from("~/.config/crucible"),
            ],
            format: ConfigFormat::Auto,
        }
    }

    /// Create a loader with specific search paths.
    pub fn with_search_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths: paths,
            format: ConfigFormat::Auto,
        }
    }

    /// Set the configuration format.
    pub fn with_format(mut self, format: ConfigFormat) -> Self {
        self.format = format;
        self
    }

    /// Add a search path.
    pub fn add_search_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.search_paths.push(path.as_ref().to_path_buf());
        self
    }

    /// Load configuration from a file.
    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await?;

        let format = ConfigFormat::from_path(path)?;
        Self::parse_from_string(&content, format)
    }

    /// Load configuration from a file (synchronous).
    pub fn load_from_file_sync<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;

        let format = ConfigFormat::from_path(path)?;
        Self::parse_from_string(&content, format)
    }

    /// Load configuration from a string.
    pub fn load_from_str(content: &str, format: ConfigFormat) -> Result<Config, ConfigError> {
        Self::parse_from_string(content, format)
    }

    /// Search for and load configuration from standard locations.
    pub async fn load_from_search_paths(&self, filename: &str) -> Result<Config, ConfigError> {
        // First try exact filename
        for search_path in &self.search_paths {
            let config_path = search_path.join(filename);

            if config_path.exists() {
                tracing::debug!("Loading config from: {}", config_path.display());
                return Self::load_from_file(&config_path).await;
            }
        }

        // Try with different extensions if not specified
        if !filename.contains('.') {
            for search_path in &self.search_paths {
                for extension in ["yaml", "yml", "toml", "json"] {
                    let config_path = search_path.join(format!("{}.{}", filename, extension));

                    if config_path.exists() {
                        tracing::debug!("Loading config from: {}", config_path.display());
                        return Self::load_from_file(&config_path).await;
                    }
                }
            }
        }

        Err(ConfigError::MissingValue {
            field: format!("config file: {}", filename),
        })
    }

    /// Search for and load configuration from standard locations (synchronous).
    pub fn load_from_search_paths_sync(&self, filename: &str) -> Result<Config, ConfigError> {
        // First try exact filename
        for search_path in &self.search_paths {
            let config_path = search_path.join(filename);

            if config_path.exists() {
                tracing::debug!("Loading config from: {}", config_path.display());
                return Self::load_from_file_sync(&config_path);
            }
        }

        // Try with different extensions if not specified
        if !filename.contains('.') {
            for search_path in &self.search_paths {
                for extension in ["yaml", "yml", "toml", "json"] {
                    let config_path = search_path.join(format!("{}.{}", filename, extension));

                    if config_path.exists() {
                        tracing::debug!("Loading config from: {}", config_path.display());
                        return Self::load_from_file_sync(&config_path);
                    }
                }
            }
        }

        Err(ConfigError::MissingValue {
            field: format!("config file: {}", filename),
        })
    }

    /// Load configuration with environment variable overrides.
    pub async fn load_with_env_overrides<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let mut config = Self::load_from_file(path).await?;
        Self::apply_env_overrides(&mut config);
        Ok(config)
    }

    /// Load configuration with environment variable overrides (synchronous).
    pub fn load_with_env_overrides_sync<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let mut config = Self::load_from_file_sync(path)?;
        Self::apply_env_overrides(&mut config);
        Ok(config)
    }

    /// Parse configuration from a string with specified format.
    fn parse_from_string(content: &str, format: ConfigFormat) -> Result<Config, ConfigError> {
        match format {
            #[cfg(feature = "yaml")]
            ConfigFormat::Yaml => {
                let config: Config = serde_yaml::from_str(content)?;
                Ok(config)
            }
            #[cfg(feature = "toml")]
            ConfigFormat::Toml => {
                let config: Config = toml::from_str(content)?;
                Ok(config)
            }
            ConfigFormat::Json => {
                let config: Config = serde_json::from_str(content)?;
                Ok(config)
            }
            #[cfg(not(feature = "yaml"))]
            ConfigFormat::Yaml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "YAML support not enabled",
            ))),
            #[cfg(not(feature = "toml"))]
            ConfigFormat::Toml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "TOML support not enabled",
            ))),
            ConfigFormat::Auto => {
                // Try to detect format and parse
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Yaml) {
                    return Ok(config);
                }
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Toml) {
                    return Ok(config);
                }
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Json) {
                    return Ok(config);
                }
                Err(ConfigError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unable to parse configuration in any supported format",
                )))
            }
        }
    }

    /// Apply environment variable overrides to configuration.
    pub fn apply_env_overrides(config: &mut Config) {
        // Override profile
        if let Ok(profile) = std::env::var("CRUCIBLE_PROFILE") {
            config.profile = Some(profile);
        }

        // Override embedding provider API key
        if let Ok(api_key) = std::env::var("CRUCIBLE_EMBEDDING_API_KEY") {
            use crate::{EnrichmentConfig, EmbeddingProviderConfig, OpenAIConfig, PipelineConfig};

            if config.enrichment.is_none() {
                // Create a default OpenAI provider configuration
                config.enrichment = Some(EnrichmentConfig {
                    provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
                        api_key: api_key.clone(),
                        model: "text-embedding-3-small".to_string(),
                        base_url: "https://api.openai.com/v1".to_string(),
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        dimensions: 1536,
                        headers: Default::default(),
                    }),
                    pipeline: PipelineConfig::default(),
                });
            } else if let Some(ref mut enrichment) = config.enrichment {
                // Update API key if using OpenAI provider
                if let EmbeddingProviderConfig::OpenAI(ref mut openai_config) = enrichment.provider {
                    openai_config.api_key = api_key;
                }
            }
        }

        // Override database URL
        if let Ok(database_url) = std::env::var("CRUCIBLE_DATABASE_URL") {
            if config.database.is_none() {
                config.database = Some(crate::DatabaseConfig {
                    db_type: crate::DatabaseType::Sqlite,
                    url: database_url.clone(),
                    max_connections: Some(5),
                    timeout_seconds: Some(30),
                    options: std::collections::HashMap::new(),
                });
            } else if let Some(ref mut database) = config.database {
                database.url = database_url;
            }
        }

        // Override server host
        if let Ok(host) = std::env::var("CRUCIBLE_SERVER_HOST") {
            if config.server.is_none() {
                config.server = Some(crate::ServerConfig::default());
            }
            if let Some(ref mut server) = config.server {
                server.host = host;
            }
        }

        // Override server port
        if let Ok(port) = std::env::var("CRUCIBLE_SERVER_PORT") {
            if let Ok(port) = port.parse::<u16>() {
                if config.server.is_none() {
                    config.server = Some(crate::ServerConfig::default());
                }
                if let Some(ref mut server) = config.server {
                    server.port = port;
                }
            }
        }

        // Override logging level
        if let Ok(level) = std::env::var("CRUCIBLE_LOG_LEVEL") {
            if config.logging.is_none() {
                config.logging = Some(crate::LoggingConfig {
                    level: level.clone(),
                    format: "text".to_string(),
                    file: false,
                    file_path: None,
                    max_file_size: None,
                    max_files: None,
                    ..Default::default()
                });
            } else if let Some(ref mut logging) = config.logging {
                logging.level = level;
            }
        }
    }

    /// Save configuration to a file.
    pub async fn save_to_file<P: AsRef<Path>>(config: &Config, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let content = Self::serialize_to_string(config, format)?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(path, content).await?;
        Ok(())
    }

    /// Save configuration to a file (synchronous).
    pub fn save_to_file_sync<P: AsRef<Path>>(config: &Config, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let content = Self::serialize_to_string(config, format)?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Serialize configuration to a string with specified format.
    fn serialize_to_string(config: &Config, format: ConfigFormat) -> Result<String, ConfigError> {
        match format {
            #[cfg(feature = "yaml")]
            ConfigFormat::Yaml => {
                let content = serde_yaml::to_string(config)?;
                Ok(content)
            }
            #[cfg(feature = "toml")]
            ConfigFormat::Toml => {
                let content = toml::to_string_pretty(config)
                    .map_err(|e| crate::ConfigError::TomlSer(format!("{}", e)))?;
                Ok(content)
            }
            ConfigFormat::Json => {
                let content = serde_json::to_string_pretty(config)?;
                Ok(content)
            }
            #[cfg(not(feature = "yaml"))]
            ConfigFormat::Yaml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "YAML support not enabled",
            ))),
            #[cfg(not(feature = "toml"))]
            ConfigFormat::Toml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "TOML support not enabled",
            ))),
            ConfigFormat::Auto => {
                // Default to YAML for auto format
                Self::serialize_to_string(config, ConfigFormat::Yaml)
            }
        }
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// YAML format.
    Yaml,
    /// TOML format.
    Toml,
    /// JSON format.
    Json,
    /// Auto-detect format from file extension.
    Auto,
}

impl ConfigFormat {
    /// Detect format from file path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();

        // Handle temporary files without extensions by trying to detect from content
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "yaml" | "yml" => Ok(Self::Yaml),
                "toml" => Ok(Self::Toml),
                "json" => Ok(Self::Json),
                _ => Err(ConfigError::MissingValue {
                    field: format!("unsupported file extension: {}", extension),
                }),
            }
        } else {
            // For files without extensions (like temporary files), default to YAML
            // The actual format detection will happen during parsing
            Ok(Self::Auto)
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Auto => "yaml",
        }
    }
}

/// Builder for creating configuration loaders with fluent interface.
pub struct ConfigLoaderBuilder {
    loader: ConfigLoader,
}

impl ConfigLoaderBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            loader: ConfigLoader::new(),
        }
    }

    /// Add a search path.
    pub fn search_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.loader = self.loader.add_search_path(path);
        self
    }

    /// Set the format.
    pub fn format(mut self, format: ConfigFormat) -> Self {
        self.loader.format = format;
        self
    }

    /// Build the loader.
    pub fn build(self) -> ConfigLoader {
        self.loader
    }
}

impl Default for ConfigLoaderBuilder {
    fn default() -> Self {
        Self::new()
    }
}
