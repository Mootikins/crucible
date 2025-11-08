//! Migration utilities for backward compatibility with existing configurations.

use crate::{Config, EmbeddingProviderConfig, ProfileConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// Legacy configuration format (environment variables based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyConfig {
    /// Legacy embedding provider configuration.
    pub embedding_provider: Option<LegacyEmbeddingProviderConfig>,

    /// Legacy database configuration.
    pub database: Option<LegacyDatabaseConfig>,

    /// Legacy server configuration.
    pub server: Option<LegacyServerConfig>,

    /// Legacy custom values.
    pub custom: HashMap<String, serde_json::Value>,
}

/// Legacy embedding provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyEmbeddingProviderConfig {
    /// Provider type (legacy format).
    pub provider: Option<String>,

    /// API key (legacy format).
    pub api_key: Option<String>,

    /// Base URL (legacy format).
    pub base_url: Option<String>,

    /// Model name (legacy format).
    pub model: Option<String>,

    /// Timeout (legacy format).
    pub timeout: Option<u64>,
}

/// Legacy database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyDatabaseConfig {
    /// Database URL (legacy format).
    pub url: Option<String>,

    /// Database type (legacy format).
    pub db_type: Option<String>,

    /// Max connections (legacy format).
    pub max_connections: Option<u32>,
}

/// Legacy server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyServerConfig {
    /// Host (legacy format).
    pub host: Option<String>,

    /// Port (legacy format).
    pub port: Option<u16>,

    /// Enable HTTPS (legacy format).
    pub https: Option<bool>,
}

/// Migration result containing status and messages.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// Migration success status.
    pub success: bool,

    /// Informational messages.
    pub info: Vec<String>,

    /// Warning messages.
    pub warnings: Vec<String>,

    /// Error messages.
    pub errors: Vec<String>,

    /// Migrated configuration.
    pub config: Option<Config>,
}

impl MigrationResult {
    /// Create a new migration result.
    pub fn new() -> Self {
        Self {
            success: false,
            info: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
            config: None,
        }
    }

    /// Add an informational message.
    pub fn add_info<S: Into<String>>(&mut self, message: S) {
        self.info.push(message.into());
    }

    /// Add a warning message.
    pub fn add_warning<S: Into<String>>(&mut self, message: S) {
        self.warnings.push(message.into());
    }

    /// Add an error message.
    pub fn add_error<S: Into<String>>(&mut self, message: S) {
        self.errors.push(message.into());
    }

    /// Set success status.
    pub fn set_success(&mut self, success: bool) {
        self.success = success;
    }

    /// Set the migrated configuration.
    pub fn set_config(&mut self, config: Config) {
        self.config = Some(config);
    }

    /// Check if migration was successful.
    pub fn is_success(&self) -> bool {
        self.success && self.config.is_some()
    }
}

impl Default for MigrationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration migrator for handling legacy formats.
pub struct ConfigMigrator {
    /// Migration version.
    version: String,

    /// Enable automatic migration.
    auto_migrate: bool,

    /// Preserve legacy files.
    preserve_legacy: bool,
}

impl ConfigMigrator {
    /// Create a new configuration migrator.
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            auto_migrate: true,
            preserve_legacy: true,
        }
    }

    /// Set the migration version.
    pub fn with_version<S: Into<String>>(mut self, version: S) -> Self {
        self.version = version.into();
        self
    }

    /// Enable or disable automatic migration.
    pub fn with_auto_migrate(mut self, auto_migrate: bool) -> Self {
        self.auto_migrate = auto_migrate;
        self
    }

    /// Set whether to preserve legacy files.
    pub fn with_preserve_legacy(mut self, preserve_legacy: bool) -> Self {
        self.preserve_legacy = preserve_legacy;
        self
    }

    /// Migrate from environment variables to new configuration format.
    pub fn migrate_from_env_vars(&self) -> MigrationResult {
        let mut result = MigrationResult::new();
        result.add_info("Starting migration from environment variables");

        let mut config = Config::new();

        // Migrate CRUCIBLE_PROFILE
        if let Ok(profile) = std::env::var("CRUCIBLE_PROFILE") {
            config.profile = Some(profile.clone());
            result.add_info(format!("Migrated CRUCIBLE_PROFILE: {}", profile));
        }

        // Migrate embedding provider configuration
        self.migrate_embedding_provider_from_env(&mut config, &mut result);

        // Migrate database configuration
        self.migrate_database_from_env(&mut config, &mut result);

        // Migrate server configuration
        self.migrate_server_from_env(&mut config, &mut result);

        // Migrate logging configuration
        self.migrate_logging_from_env(&mut config, &mut result);

        // Create default profile if none exists
        if config.profiles.is_empty() {
            let profile = ProfileConfig::development();
            config.profiles.insert("default".to_string(), profile);
            result.add_info("Created default development profile");
        }

        result.set_config(config);
        result.set_success(true);

        info!("Environment variable migration completed successfully");
        result
    }

    /// Migrate from legacy configuration file.
    pub fn migrate_from_legacy_file(&self, legacy_path: &str) -> MigrationResult {
        let mut result = MigrationResult::new();
        result.add_info(format!(
            "Starting migration from legacy file: {}",
            legacy_path
        ));

        // Read legacy configuration
        let legacy_content = match std::fs::read_to_string(legacy_path) {
            Ok(content) => content,
            Err(err) => {
                result.add_error(format!("Failed to read legacy file: {}", err));
                return result;
            }
        };

        // Parse legacy configuration
        let legacy_config: LegacyConfig = match serde_yaml::from_str(&legacy_content) {
            Ok(config) => config,
            Err(err) => {
                result.add_error(format!("Failed to parse legacy configuration: {}", err));
                return result;
            }
        };

        // Migrate to new format
        let config = self.migrate_legacy_config(legacy_config, &mut result);

        result.set_config(config);
        result.set_success(true);

        // Backup legacy file if preserve is enabled
        if self.preserve_legacy {
            let backup_path = format!("{}.backup", legacy_path);
            if let Err(err) = std::fs::copy(legacy_path, &backup_path) {
                result.add_warning(format!("Failed to backup legacy file: {}", err));
            } else {
                result.add_info(format!("Legacy file backed up to: {}", backup_path));
            }
        }

        info!("Legacy file migration completed successfully");
        result
    }

    /// Auto-detect and migrate existing configurations.
    pub fn auto_migrate(&self) -> Vec<MigrationResult> {
        let mut results = Vec::new();

        // Check for environment variable configuration
        if self.has_env_config() {
            results.push(self.migrate_from_env_vars());
        }

        // Check for legacy configuration files
        for legacy_file in self.find_legacy_files() {
            results.push(self.migrate_from_legacy_file(&legacy_file));
        }

        results
    }

    /// Migrate embedding provider from environment variables.
    fn migrate_embedding_provider_from_env(
        &self,
        config: &mut Config,
        result: &mut MigrationResult,
    ) {
        let mut provider_config = None;

        // Check for OpenAI configuration
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            let mut provider =
                EmbeddingProviderConfig::openai(api_key, std::env::var("OPENAI_MODEL").ok());

            if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
                provider.api.base_url = Some(base_url);
            }

            provider_config = Some(provider);
            result.add_info("Migrated OpenAI embedding provider from environment");
        }

        // Check for Ollama configuration
        if let Ok(ollama_url) = std::env::var("OLLAMA_BASE_URL") {
            let model =
                std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "nomic-embed-text".to_string());

            provider_config = Some(EmbeddingProviderConfig::ollama(
                Some(ollama_url),
                Some(model),
            ));
            result.add_info("Migrated Ollama embedding provider from environment");
        }

        // Check for generic embedding configuration
        if let Ok(api_key) = std::env::var("CRUCIBLE_EMBEDDING_API_KEY") {
            let provider_type = std::env::var("CRUCIBLE_EMBEDDING_PROVIDER")
                .unwrap_or_else(|_| "openai".to_string());

            let model = std::env::var("CRUCIBLE_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".to_string());

            match provider_type.to_lowercase().as_str() {
                "openai" => {
                    provider_config = Some(EmbeddingProviderConfig::openai(api_key, Some(model)));
                }
                "ollama" => {
                    let base_url = std::env::var("CRUCIBLE_EMBEDDING_BASE_URL").ok();
                    provider_config = Some(EmbeddingProviderConfig::ollama(base_url, Some(model)));
                }
                _ => {
                    result.add_warning(format!(
                        "Unknown embedding provider type: {}",
                        provider_type
                    ));
                }
            }

            result.add_info("Migrated generic embedding provider from environment");
        }

        if let Some(provider) = provider_config {
            config.embedding = Some(provider);
        }
    }

    /// Migrate database configuration from environment variables.
    fn migrate_database_from_env(&self, config: &mut Config, result: &mut MigrationResult) {
        if let Ok(database_url) = std::env::var("CRUCIBLE_DATABASE_URL") {
            use crate::{DatabaseConfig, DatabaseType};

            let db_type = if database_url.starts_with("sqlite:") {
                DatabaseType::Sqlite
            } else if database_url.starts_with("postgresql:")
                || database_url.starts_with("postgres:")
            {
                DatabaseType::Postgres
            } else if database_url.starts_with("mysql:") {
                DatabaseType::Mysql
            } else {
                DatabaseType::Sqlite // Default
            };

            config.database = Some(DatabaseConfig {
                db_type,
                url: database_url.clone(),
                max_connections: Some(5),
                timeout_seconds: Some(30),
                options: HashMap::new(),
            });

            result.add_info(format!("Migrated database URL: {}", database_url));
        }
    }

    /// Migrate server configuration from environment variables.
    fn migrate_server_from_env(&self, config: &mut Config, result: &mut MigrationResult) {
        use crate::ServerConfig;

        let mut server = ServerConfig::default();

        let mut migrated = false;

        if let Ok(host) = std::env::var("CRUCIBLE_SERVER_HOST") {
            server.host = host.clone();
            result.add_info(format!("Migrated server host: {}", host));
            migrated = true;
        }

        if let Ok(port) = std::env::var("CRUCIBLE_SERVER_PORT") {
            if let Ok(port_num) = port.parse::<u16>() {
                server.port = port_num;
                result.add_info(format!("Migrated server port: {}", port_num));
                migrated = true;
            } else {
                result.add_warning(format!("Invalid server port: {}", port));
            }
        }

        if migrated {
            config.server = Some(server);
        }
    }

    /// Migrate logging configuration from environment variables.
    fn migrate_logging_from_env(&self, config: &mut Config, result: &mut MigrationResult) {
        use crate::LoggingConfig;

        let mut logging = LoggingConfig::default();

        let mut migrated = false;

        if let Ok(level) = std::env::var("CRUCIBLE_LOG_LEVEL") {
            logging.level = level.clone();
            result.add_info(format!("Migrated log level: {}", level));
            migrated = true;
        }

        if let Ok(format) = std::env::var("CRUCIBLE_LOG_FORMAT") {
            logging.format = format.clone();
            result.add_info(format!("Migrated log format: {}", format));
            migrated = true;
        }

        if migrated {
            config.logging = Some(logging);
        }
    }

    /// Migrate legacy configuration to new format.
    fn migrate_legacy_config(&self, legacy: LegacyConfig, result: &mut MigrationResult) -> Config {
        let mut config = Config::new();

        // Migrate embedding provider
        if let Some(legacy_provider) = legacy.embedding_provider {
            config.embedding = self.migrate_legacy_embedding_provider(legacy_provider, result);
        }

        // Migrate database
        if let Some(legacy_db) = legacy.database {
            config.database = self.migrate_legacy_database(legacy_db, result);
        }

        // Migrate server
        if let Some(legacy_server) = legacy.server {
            config.server = self.migrate_legacy_server(legacy_server, result);
        }

        // Migrate custom values
        config.custom = legacy.custom;

        config
    }

    /// Migrate legacy embedding provider configuration.
    fn migrate_legacy_embedding_provider(
        &self,
        legacy: LegacyEmbeddingProviderConfig,
        result: &mut MigrationResult,
    ) -> Option<EmbeddingProviderConfig> {
        use crate::{ApiConfig, EmbeddingProviderType, ModelConfig};

        let provider_type = match legacy.provider.as_deref() {
            Some("openai") => EmbeddingProviderType::OpenAI,
            Some("ollama") => EmbeddingProviderType::Ollama,
            Some("cohere") => EmbeddingProviderType::Cohere,
            Some(other) => {
                result.add_warning(format!("Unknown legacy provider: {}", other));
                EmbeddingProviderType::Custom(other.to_string())
            }
            None => {
                result.add_warning("No provider type specified in legacy configuration");
                return None;
            }
        };

        let api_config = ApiConfig {
            key: legacy.api_key,
            base_url: legacy.base_url,
            timeout_seconds: legacy.timeout,
            retry_attempts: Some(3),
            headers: HashMap::new(),
        };

        let model_config = ModelConfig {
            name: legacy.model.unwrap_or_else(|| "default".to_string()),
            dimensions: None,
            max_tokens: Some(8192),
        };

        result.add_info("Migrated legacy embedding provider configuration");

        Some(EmbeddingProviderConfig {
            provider_type,
            api: api_config,
            model: model_config,
            options: HashMap::new(),
        })
    }

    /// Migrate legacy database configuration.
    fn migrate_legacy_database(
        &self,
        legacy: LegacyDatabaseConfig,
        result: &mut MigrationResult,
    ) -> Option<crate::DatabaseConfig> {
        use crate::{DatabaseConfig, DatabaseType};

        let db_type = match legacy.db_type.as_deref() {
            Some("sqlite") => DatabaseType::Sqlite,
            Some("postgres") | Some("postgresql") => DatabaseType::Postgres,
            Some("mysql") => DatabaseType::Mysql,
            Some("surrealdb") => DatabaseType::Surrealdb,
            Some(other) => {
                result.add_warning(format!("Unknown legacy database type: {}", other));
                DatabaseType::Custom(other.to_string())
            }
            None => DatabaseType::Sqlite,
        };

        result.add_info("Migrated legacy database configuration");

        Some(DatabaseConfig {
            db_type,
            url: legacy.url.unwrap_or_else(|| ":memory:".to_string()),
            max_connections: legacy.max_connections.or(Some(5)),
            timeout_seconds: Some(30),
            options: HashMap::new(),
        })
    }

    /// Migrate legacy server configuration.
    fn migrate_legacy_server(
        &self,
        legacy: LegacyServerConfig,
        result: &mut MigrationResult,
    ) -> Option<crate::ServerConfig> {
        use crate::ServerConfig;

        result.add_info("Migrated legacy server configuration");

        Some(ServerConfig {
            host: legacy.host.unwrap_or_else(|| "127.0.0.1".to_string()),
            port: legacy.port.unwrap_or(8080),
            https: legacy.https.unwrap_or(false),
            cert_file: None,
            key_file: None,
            max_body_size: Some(10 * 1024 * 1024),
            timeout_seconds: Some(30),
        })
    }

    /// Check if environment variable configuration exists.
    fn has_env_config(&self) -> bool {
        std::env::var("CRUCIBLE_PROFILE").is_ok()
            || std::env::var("CRUCIBLE_EMBEDDING_API_KEY").is_ok()
            || std::env::var("CRUCIBLE_DATABASE_URL").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("OLLAMA_BASE_URL").is_ok()
    }

    /// Find legacy configuration files.
    fn find_legacy_files(&self) -> Vec<String> {
        let mut legacy_files = Vec::new();

        let potential_paths = [
            "crucible.yaml",
            "crucible.yml",
            "crucible.json",
            ".crucible.yaml",
            ".crucible.yml",
            ".crucible.json",
            "config/crucible.yaml",
            "config/crucible.yml",
            "config/crucible.json",
        ];

        for path in potential_paths.iter() {
            if std::path::Path::new(path).exists() {
                legacy_files.push(path.to_string());
            }
        }

        legacy_files
    }
}

impl Default for ConfigMigrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function to perform automatic migration.
pub fn auto_migrate() -> Vec<MigrationResult> {
    let migrator = ConfigMigrator::new();
    migrator.auto_migrate()
}

/// Utility function to migrate from environment variables.
pub fn migrate_from_env() -> MigrationResult {
    let migrator = ConfigMigrator::new();
    migrator.migrate_from_env_vars()
}
