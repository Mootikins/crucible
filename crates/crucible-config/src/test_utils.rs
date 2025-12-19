//! Test utilities for configuration testing.

use crate::{
    Config, ConfigLoader, DatabaseConfig, Environment, LoggingConfig, ProfileConfig, ServerConfig,
};
use crate::{
    EmbeddingProviderConfig, EnrichmentConfig, OllamaConfig, OpenAIConfig, PipelineConfig,
};
use std::collections::HashMap;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

/// Test configuration builder for creating test configurations easily.
pub struct TestConfigBuilder {
    config: Config,
}

impl TestConfigBuilder {
    /// Create a new test configuration builder.
    pub fn new() -> Self {
        Self {
            config: Config::new(),
        }
    }

    /// Set the active profile and add it to profiles if not already present.
    pub fn profile<S: Into<String>>(mut self, profile_name: S) -> Self {
        let profile_name = profile_name.into();
        self.config.profile = Some(profile_name.clone());

        // If the profile doesn't exist yet, create a basic one
        self.config
            .profiles
            .entry(profile_name)
            .or_insert_with_key(|name| ProfileConfig::new(name.clone(), Environment::Test));

        self
    }

    /// Add a profile configuration.
    pub fn add_profile(mut self, profile: ProfileConfig) -> Self {
        self.config.profiles.insert(profile.name.clone(), profile);
        self
    }

    /// Add an enrichment configuration.
    pub fn enrichment_config(mut self, enrichment: EnrichmentConfig) -> Self {
        self.config.enrichment = Some(enrichment);
        self
    }

    /// Add a mock OpenAI embedding provider.
    pub fn mock_openai_embedding(self) -> Self {
        self.enrichment_config(EnrichmentConfig {
            provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
                api_key: "test-api-key".to_string(),
                model: "text-embedding-3-small".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                timeout_seconds: 30,
                retry_attempts: 3,
                dimensions: 1536,
                headers: Default::default(),
            }),
            pipeline: PipelineConfig::default(),
        })
    }

    /// Add a mock Ollama embedding provider.
    pub fn mock_ollama_embedding(self) -> Self {
        self.enrichment_config(EnrichmentConfig {
            provider: EmbeddingProviderConfig::Ollama(OllamaConfig {
                model: "nomic-embed-text".to_string(),
                base_url: "http://localhost:11434".to_string(),
                timeout_seconds: 30,
                retry_attempts: 3,
                dimensions: 768,
                batch_size: 50,
            }),
            pipeline: PipelineConfig::default(),
        })
    }

    /// Add a custom Ollama embedding provider with specific endpoint and model.
    pub fn ollama_embedding<S1: Into<String>, S2: Into<String>>(
        self,
        endpoint: S1,
        model: S2,
    ) -> Self {
        self.enrichment_config(EnrichmentConfig {
            provider: EmbeddingProviderConfig::Ollama(OllamaConfig {
                model: model.into(),
                base_url: endpoint.into(),
                timeout_seconds: 30,
                retry_attempts: 3,
                dimensions: 768,
                batch_size: 50,
            }),
            pipeline: PipelineConfig::default(),
        })
    }

    /// Add a kiln path to the configuration.
    pub fn kiln_path<S: Into<String>>(self, path: S) -> Self {
        self.set("kiln_path", path.into())
    }

    /// Add a database configuration.
    pub fn database(mut self, database: DatabaseConfig) -> Self {
        self.config.database = Some(database);
        self
    }

    /// Add an in-memory SQLite database for testing.
    pub fn memory_database(self) -> Self {
        use crate::{DatabaseConfig, DatabaseType};
        self.database(DatabaseConfig {
            db_type: DatabaseType::Sqlite,
            url: ":memory:".to_string(),
            max_connections: Some(1),
            timeout_seconds: Some(30),
            options: HashMap::new(),
        })
    }

    /// Add a file-based SQLite database for testing.
    pub fn file_database<S: Into<String>>(self, path: S) -> Self {
        use crate::{DatabaseConfig, DatabaseType};
        self.database(DatabaseConfig {
            db_type: DatabaseType::Sqlite,
            url: path.into(),
            max_connections: Some(5),
            timeout_seconds: Some(30),
            options: HashMap::new(),
        })
    }

    /// Add a server configuration.
    pub fn server(mut self, server: ServerConfig) -> Self {
        self.config.server = Some(server);
        self
    }

    /// Add a development server configuration.
    pub fn dev_server(self) -> Self {
        self.server(ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            https: false,
            cert_file: None,
            key_file: None,
            max_body_size: Some(1024 * 1024),
            timeout_seconds: Some(30),
        })
    }

    /// Add logging configuration.
    pub fn logging(mut self, logging: LoggingConfig) -> Self {
        self.config.logging = Some(logging);
        self
    }

    /// Add debug logging configuration.
    pub fn debug_logging(self) -> Self {
        self.logging(LoggingConfig {
            level: "debug".to_string(),
            format: "text".to_string(),
            file: false,
            file_path: None,
            max_file_size: None,
            max_files: None,
            ..Default::default()
        })
    }

    /// Add a custom configuration value.
    pub fn set<S: Into<String>, V: serde::Serialize>(mut self, key: S, value: V) -> Self {
        let json_value = serde_json::to_value(value).unwrap();
        self.config.custom.insert(key.into(), json_value);
        self
    }

    /// Build the configuration.
    pub fn build(self) -> Config {
        self.config
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Test configuration utilities.
pub struct TestConfig;

impl TestConfig {
    /// Create a minimal test configuration.
    pub fn minimal() -> Config {
        let test_profile = ProfileConfig::new("test".to_string(), Environment::Test)
            .setting("cache_enabled".to_string(), false)
            .unwrap();

        TestConfigBuilder::new()
            .profile("test")
            .add_profile(test_profile)
            .memory_database()
            .mock_openai_embedding()
            .debug_logging()
            .build()
    }

    /// Create a comprehensive test configuration.
    pub fn comprehensive() -> Config {
        TestConfigBuilder::new()
            .profile("test")
            .add_profile(ProfileConfig::development())
            .add_profile(ProfileConfig::testing())
            .mock_openai_embedding()
            .memory_database()
            .dev_server()
            .debug_logging()
            .set("test_mode", true)
            .set("cache_enabled", false)
            .build()
    }

    /// Create a configuration with Ollama provider.
    pub fn with_ollama() -> Config {
        TestConfigBuilder::new()
            .profile("test")
            .mock_ollama_embedding()
            .memory_database()
            .debug_logging()
            .build()
    }

    /// Create a production-like configuration for testing.
    pub fn production_like() -> Config {
        TestConfigBuilder::new()
            .profile("production")
            .enrichment_config(EnrichmentConfig {
                provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
                    api_key: "prod-api-key".to_string(),
                    model: "text-embedding-3-large".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 30,
                    retry_attempts: 3,
                    dimensions: 3072,
                    headers: Default::default(),
                }),
                pipeline: PipelineConfig::default(),
            })
            .file_database("test.db")
            .server(ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                https: true,
                cert_file: Some("cert.pem".to_string()),
                key_file: Some("key.pem".to_string()),
                max_body_size: Some(10 * 1024 * 1024),
                timeout_seconds: Some(60),
            })
            .logging(LoggingConfig {
                level: "warn".to_string(),
                format: "json".to_string(),
                file: true,
                file_path: Some("test.log".to_string()),
                max_file_size: Some(100 * 1024 * 1024),
                max_files: Some(10),
                ..Default::default()
            })
            .build()
    }

    /// Create a test configuration with a temporary kiln.
    /// Returns the config and the TempDir (which must be kept alive).
    pub fn with_temp_kiln() -> (Config, TempDir) {
        let (temp_dir, kiln_path) = TestKiln::create_temp_kiln();
        let config = TestConfigBuilder::new()
            .profile("test")
            .kiln_path(kiln_path)
            .mock_ollama_embedding()
            .memory_database()
            .debug_logging()
            .build();
        (config, temp_dir)
    }

    /// Create a test configuration with a custom kiln path.
    pub fn with_kiln_path<S: Into<String>>(kiln_path: S) -> Config {
        TestConfigBuilder::new()
            .profile("test")
            .kiln_path(kiln_path)
            .mock_ollama_embedding()
            .memory_database()
            .debug_logging()
            .build()
    }
}

/// Test kiln utilities for managing temporary kilns in tests.
pub struct TestKiln;

impl TestKiln {
    /// Create a temporary kiln directory with sample markdown files.
    /// Returns the TempDir and its path as a string.
    pub fn create_temp_kiln() -> (TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path();

        // Create some sample markdown files
        let test_files = vec![
            ("README.md", "# README\n\nWelcome to the test kiln."),
            ("notes.md", "# Notes\n\nSome test notes here."),
            ("project.md", "# Project\n\nProject documentation."),
        ];

        for (filename, content) in test_files {
            std::fs::write(kiln_path.join(filename), content).unwrap();
        }

        let path_str = kiln_path.to_string_lossy().to_string();
        (temp_dir, path_str)
    }

    /// Create a temporary kiln with specific files.
    pub fn create_temp_kiln_with_files(files: Vec<(&str, &str)>) -> (TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path();

        // Create provided files
        for (filename, content) in files {
            let file_path = kiln_path.join(filename);
            // Create parent directories if needed
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(file_path, content).unwrap();
        }

        let path_str = kiln_path.to_string_lossy().to_string();
        (temp_dir, path_str)
    }
}

/// Temporary configuration utilities for testing.
pub struct TempConfig;

impl TempConfig {
    /// Create a temporary configuration file with the given configuration.
    pub fn create_temp_file(config: &Config) -> (NamedTempFile, String) {
        let mut temp_file = NamedTempFile::with_suffix("yaml").unwrap();
        let content = serde_yaml::to_string(config).unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        let path = temp_file.path().to_string_lossy().to_string();
        (temp_file, path)
    }

    /// Create a temporary configuration directory.
    pub fn create_temp_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    /// Create a temporary configuration file in a specific format.
    pub fn create_temp_file_with_format(
        config: &Config,
        format: crate::ConfigFormat,
    ) -> (NamedTempFile, String) {
        let temp_file = NamedTempFile::with_suffix(format.extension()).unwrap();
        let content = match format {
            crate::ConfigFormat::Yaml => serde_yaml::to_string(config).unwrap(),
            crate::ConfigFormat::Json => serde_json::to_string_pretty(config).unwrap(),
            #[cfg(feature = "toml")]
            crate::ConfigFormat::Toml => toml::to_string_pretty(config).unwrap(),
            #[cfg(not(feature = "toml"))]
            crate::ConfigFormat::Toml => serde_yaml::to_string(config).unwrap(),
            crate::ConfigFormat::Auto => serde_yaml::to_string(config).unwrap(),
        };
        let mut file = temp_file.as_file();
        file.write_all(content.as_bytes()).unwrap();
        let path = temp_file.path().to_string_lossy().to_string();
        (temp_file, path)
    }

    /// Create a configuration file with test data in a temporary directory.
    pub fn create_config_in_dir(dir: &TempDir, filename: &str, config: &Config) -> String {
        let config_path = dir.path().join(filename);
        let content = serde_yaml::to_string(config).unwrap();
        std::fs::write(&config_path, content).unwrap();
        config_path.to_string_lossy().to_string()
    }
}

/// Environment variable test utilities.
pub struct TestEnv;

impl TestEnv {
    /// Set environment variables for testing.
    pub fn set_test_vars() -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("CRUCIBLE_PROFILE".to_string(), "test".to_string());
        vars.insert(
            "CRUCIBLE_EMBEDDING_API_KEY".to_string(),
            "test-key".to_string(),
        );
        vars.insert("CRUCIBLE_DATABASE_URL".to_string(), ":memory:".to_string());
        vars.insert("CRUCIBLE_SERVER_HOST".to_string(), "127.0.0.1".to_string());
        vars.insert("CRUCIBLE_SERVER_PORT".to_string(), "3000".to_string());
        vars.insert("CRUCIBLE_LOG_LEVEL".to_string(), "debug".to_string());

        for (key, value) in &vars {
            std::env::set_var(key, value);
        }

        vars
    }

    /// Clear environment variables after testing.
    pub fn clear_test_vars(vars: HashMap<String, String>) {
        for key in vars.keys() {
            std::env::remove_var(key);
        }
    }

    /// Run a test with temporary environment variables.
    pub fn with_test_vars<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let vars = Self::set_test_vars();
        let result = f();
        Self::clear_test_vars(vars);
        result
    }
}

/// Configuration validation test utilities.
pub struct ConfigValidation;

impl ConfigValidation {
    /// Validate that a configuration can be loaded and saved.
    pub fn test_round_trip(config: &Config) -> Result<(), crate::ConfigError> {
        let yaml_content = serde_yaml::to_string(config).unwrap();
        let loaded_config: Config = serde_yaml::from_str(&yaml_content)?;

        let json_content = serde_json::to_string(config).unwrap();
        let _loaded_config2: Config = serde_json::from_str(&json_content)?;

        // Verify key configurations match
        assert_eq!(config.profile, loaded_config.profile);

        // Compare enrichment configs if they exist
        match (
            config.enrichment_config(),
            loaded_config.enrichment_config(),
        ) {
            (Ok(config1), Ok(config2)) => assert_eq!(config1, config2),
            (Err(_), Err(_)) => {} // Both errors - that's fine for comparison
            _ => panic!("Enrichment config comparison failed"),
        }

        // Compare databases if they exist
        match (config.database(), loaded_config.database()) {
            (Ok(db1), Ok(db2)) => assert_eq!(db1, db2),
            (Err(_), Err(_)) => {} // Both errors - that's fine for comparison
            _ => panic!("Database comparison failed"),
        }

        Ok(())
    }

    /// Validate that environment variable overrides work.
    pub fn test_env_overrides(mut config: Config) -> Result<(), crate::ConfigError> {
        TestEnv::with_test_vars(|| {
            ConfigLoader::apply_env_overrides(&mut config);

            assert_eq!(config.profile, Some("test".to_string()));
            // Check enrichment config exists and has OpenAI provider with test key
            if let Ok(enrichment) = config.enrichment_config() {
                if let EmbeddingProviderConfig::OpenAI(openai_config) = enrichment.provider {
                    assert_eq!(openai_config.api_key, "test-key".to_string());
                }
            }
            assert_eq!(config.database().unwrap().url, ":memory:".to_string());
            assert_eq!(config.server().unwrap().host, "127.0.0.1".to_string());
            assert_eq!(config.server().unwrap().port, 3000);
            assert_eq!(config.logging().level, "debug".to_string());
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_config() {
        let config = TestConfig::minimal();
        assert!(config.enrichment_config().is_ok());
        assert!(config.database().is_ok());
        assert_eq!(config.profile, Some("test".to_string()));
    }

    #[test]
    fn test_comprehensive_config() {
        let config = TestConfig::comprehensive();
        assert!(config.enrichment_config().is_ok());
        assert!(config.database().is_ok());
        assert!(config.server().is_ok());
        assert_eq!(config.profiles.len(), 4); // default + test + development + testing
    }

    #[test]
    fn test_temp_config_file() {
        let config = TestConfig::minimal();
        let (_temp_file, path) =
            TempConfig::create_temp_file_with_format(&config, crate::ConfigFormat::Yaml);

        // Load using YAML format detection from content
        let content = std::fs::read_to_string(&path).unwrap();
        let loaded_config =
            ConfigLoader::load_from_str(&content, crate::ConfigFormat::Yaml).unwrap();
        assert_eq!(config.profile, loaded_config.profile);
    }

    #[test]
    fn test_env_overrides() {
        let config = TestConfig::minimal();
        ConfigValidation::test_env_overrides(config).unwrap();
    }

    #[test]
    fn test_round_trip() {
        let config = TestConfig::comprehensive();
        ConfigValidation::test_round_trip(&config).unwrap();
    }
}
