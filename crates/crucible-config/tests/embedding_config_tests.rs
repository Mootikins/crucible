//! Integration Tests for Embedding Provider Configuration
//!
//! These tests verify that the embedding system properly uses crucible-config
//! structures instead of relying on environment variables. The tests follow TDD
//! methodology and are designed to fail initially, driving the implementation
//! of the new configuration system.
//!
//! ## Test Coverage
//!
//! - EmbeddingProviderConfig structure validation
//! - Configuration loading from files (YAML, JSON, TOML)
//! - Mock provider configuration with deterministic fixtures
//! - Configuration validation and error handling
//! - Test environment isolation and cleanup
//! - Integration with embedding thread pool configuration

use crucible_config::{
    ApiConfig, Config, ConfigError, ConfigFormat, ConfigLoader, EmbeddingProviderConfig,
    EmbeddingProviderType, Environment, ModelConfig, ProfileConfig, ProviderError,
};
// Import test utilities when feature is enabled
#[cfg(feature = "test-utils")]
use crucible_config::{TempConfig, TestConfig, TestConfigBuilder, TestEnv};
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::fs;

/// Test fixture data for deterministic embedding provider configuration
mod fixtures {
    use super::*;

    /// OpenAI provider fixture with deterministic test data
    pub fn openai_provider_fixture() -> EmbeddingProviderConfig {
        EmbeddingProviderConfig::openai(
            "sk-test-deterministic-key-12345".to_string(),
            Some("text-embedding-3-small".to_string()),
        )
    }

    /// Ollama provider fixture with deterministic test data
    pub fn ollama_provider_fixture() -> EmbeddingProviderConfig {
        EmbeddingProviderConfig::ollama(
            "http://localhost:11434".to_string(),
            "nomic-embed-text".to_string(),
        )
    }

    /// Candle provider fixture with deterministic test data
    pub fn candle_provider_fixture() -> EmbeddingProviderConfig {
        EmbeddingProviderConfig {
            provider_type: EmbeddingProviderType::Candle,
            api: ApiConfig {
                key: None,
                base_url: Some("local".to_string()),
                timeout_seconds: Some(120),
                retry_attempts: Some(1),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: "nomic-embed-text-v1.5".to_string(),
                dimensions: Some(768),
                max_tokens: Some(2048),
            },
            options: {
                let mut options = HashMap::new();
                options.insert("model_cache_dir".to_string(), json!("/tmp/candle-models"));
                options.insert("memory_limit_mb".to_string(), json!(4096));
                options.insert("device".to_string(), json!("cpu"));
                options
            },
        }
    }

    /// Custom provider fixture for testing extensibility
    pub fn custom_provider_fixture() -> EmbeddingProviderConfig {
        EmbeddingProviderConfig {
            provider_type: EmbeddingProviderType::Custom("huggingface".to_string()),
            api: ApiConfig {
                key: Some("hf-test-key-67890".to_string()),
                base_url: Some("https://api-inference.huggingface.co".to_string()),
                timeout_seconds: Some(45),
                retry_attempts: Some(2),
                headers: {
                    let mut headers = HashMap::new();
                    headers.insert(
                        "Authorization".to_string(),
                        "Bearer hf-test-key-67890".to_string(),
                    );
                    headers.insert("Content-Type".to_string(), "application/json".to_string());
                    headers
                },
            },
            model: ModelConfig {
                name: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
                dimensions: Some(384),
                max_tokens: Some(512),
            },
            options: {
                let mut options = HashMap::new();
                options.insert("use_cache".to_string(), json!(true));
                options.insert("wait_for_model".to_string(), json!(true));
                options
            },
        }
    }

    /// Complete configuration fixture with embedding provider
    pub fn complete_config_fixture() -> Config {
        TestConfigBuilder::new()
            .profile("test")
            .embedding_provider(openai_provider_fixture())
            .memory_database()
            .debug_logging()
            .build()
    }
}

/// Test suite for EmbeddingProviderConfig structure validation
#[cfg(test)]
mod embedding_provider_config_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn test_embedding_provider_config_structure_exists() {
        // This test verifies that EmbeddingProviderConfig can be created and has expected fields
        let provider = openai_provider_fixture();

        assert_eq!(provider.provider_type, EmbeddingProviderType::OpenAI);
        assert_eq!(
            provider.api.key,
            Some("sk-test-deterministic-key-12345".to_string())
        );
        assert_eq!(provider.model.name, "text-embedding-3-small");
        assert_eq!(provider.api.timeout_seconds, Some(30));
        assert_eq!(provider.api.retry_attempts, Some(3));
    }

    #[test]
    fn test_embedding_provider_config_validation() {
        let provider = openai_provider_fixture();

        // Should validate successfully
        assert!(provider.validate().is_ok());

        // Test missing API key for providers that require it
        let mut invalid_provider = provider.clone();
        invalid_provider.api.key = None;
        let validation_result = invalid_provider.validate();
        assert!(validation_result.is_err());
        assert!(matches!(
            validation_result.unwrap_err(),
            ProviderError::MissingField { .. }
        ));
    }

    #[test]
    fn test_embedding_provider_types() {
        // Test OpenAI provider
        let openai = openai_provider_fixture();
        assert!(openai.provider_type.requires_api_key());
        assert_eq!(
            openai.provider_type.default_base_url(),
            Some("https://api.openai.com/v1".to_string())
        );
        assert_eq!(
            openai.provider_type.default_model(),
            Some("text-embedding-3-small".to_string())
        );

        // Test Ollama provider
        let ollama = ollama_provider_fixture();
        assert!(!ollama.provider_type.requires_api_key());
        assert_eq!(
            ollama.provider_type.default_base_url(),
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(
            ollama.provider_type.default_model(),
            Some("nomic-embed-text".to_string())
        );

        // Test custom provider
        let custom = custom_provider_fixture();
        assert!(custom.provider_type.requires_api_key());
        assert_eq!(custom.provider_type.default_base_url(), None);
        assert_eq!(custom.provider_type.default_model(), None);

        // RED Phase: Test Candle provider (should fail initially)
        let candle = candle_provider_fixture();
        assert!(!candle.provider_type.requires_api_key());
        assert_eq!(
            candle.provider_type.default_base_url(),
            Some("local".to_string())
        );
        assert_eq!(
            candle.provider_type.default_model(),
            Some("nomic-embed-text-v1.5".to_string())
        );
    }

    #[test]
    fn test_provider_config_serialization() {
        let provider = openai_provider_fixture();

        // Test JSON serialization
        let json_str =
            serde_json::to_string_pretty(&provider).expect("Failed to serialize provider");
        let deserialized: EmbeddingProviderConfig =
            serde_json::from_str(&json_str).expect("Failed to deserialize provider");

        assert_eq!(provider, deserialized);

        // Test YAML serialization
        let yaml_str = serde_yaml::to_string(&provider).expect("Failed to serialize to YAML");
        let yaml_deserialized: EmbeddingProviderConfig =
            serde_yaml::from_str(&yaml_str).expect("Failed to deserialize from YAML");

        assert_eq!(provider, yaml_deserialized);
    }
}

/// Test suite for configuration loading from files
#[cfg(test)]
mod config_loading_tests {
    use super::*;
    use fixtures::*;

    #[tokio::test]
    async fn test_load_embedding_config_from_yaml_file() {
        let config = complete_config_fixture();
        let (_temp_file, config_path) =
            TempConfig::create_temp_file_with_format(&config, ConfigFormat::Yaml);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration from YAML file");

        // Verify embedding provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get embedding provider from loaded config");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );
        assert_eq!(
            embedding_provider.api.key,
            Some("sk-test-deterministic-key-12345".to_string())
        );
        assert_eq!(embedding_provider.model.name, "text-embedding-3-small");
    }

    #[tokio::test]
    async fn test_load_embedding_config_from_json_file() {
        let config = complete_config_fixture();
        let (_temp_file, config_path) =
            TempConfig::create_temp_file_with_format(&config, ConfigFormat::Json);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration from JSON file");

        // Verify embedding provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get embedding provider from loaded config");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );
        assert_eq!(
            embedding_provider.api.key,
            Some("sk-test-deterministic-key-12345".to_string())
        );
    }

    #[tokio::test]
    async fn test_load_config_with_ollama_provider() {
        let config = TestConfigBuilder::new()
            .profile("test")
            .embedding_provider(ollama_provider_fixture())
            .memory_database()
            .debug_logging()
            .build();

        let (_temp_file, config_path) = TempConfig::create_temp_file(&config);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration with Ollama provider");

        // Verify Ollama provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get Ollama embedding provider");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::Ollama
        );
        assert_eq!(
            embedding_provider.api.base_url,
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(embedding_provider.model.name, "nomic-embed-text");
        assert!(embedding_provider.api.key.is_none()); // Ollama doesn't require API key
    }

    #[tokio::test]
    async fn test_load_config_with_custom_provider() {
        let config = TestConfigBuilder::new()
            .profile("test")
            .embedding_provider(custom_provider_fixture())
            .memory_database()
            .debug_logging()
            .build();

        let (_temp_file, config_path) = TempConfig::create_temp_file(&config);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration with custom provider");

        // Verify custom provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get custom embedding provider");

        assert!(matches!(
            embedding_provider.provider_type,
            EmbeddingProviderType::Custom(_)
        ));
        assert_eq!(
            embedding_provider.api.key,
            Some("hf-test-key-67890".to_string())
        );
        assert_eq!(
            embedding_provider.model.name,
            "sentence-transformers/all-MiniLM-L6-v2"
        );
        assert_eq!(embedding_provider.model.dimensions, Some(384));
    }

    #[tokio::test]
    async fn test_load_config_with_missing_embedding_provider() {
        let config = TestConfigBuilder::new()
            .profile("test")
            .memory_database()
            .debug_logging()
            .build(); // No embedding provider

        let (_temp_file, config_path) = TempConfig::create_temp_file(&config);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration without embedding provider");

        // Should return error when trying to get embedding provider
        let result = loaded_config.embedding_provider();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::MissingValue { .. }
        ));
    }

    #[tokio::test]
    async fn test_load_config_from_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config = complete_config_fixture();

        // Create config file in directory
        let config_path = temp_dir.path().join("crucible.yaml");
        let config_content = serde_yaml::to_string(&config).expect("Failed to serialize config");
        fs::write(&config_path, config_content)
            .await
            .expect("Failed to write config file");

        // Load configuration from specific file path (not directory)
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration from directory");

        // Verify embedding provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get embedding provider from directory-loaded config");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );
    }
}

/// Test suite for mock embedding provider configuration
#[cfg(test)]
mod mock_provider_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn test_mock_openai_provider_configuration() {
        let mock_provider = TestConfigBuilder::new()
            .mock_openai_embedding()
            .build()
            .embedding_provider()
            .expect("Failed to get mock OpenAI provider");

        assert_eq!(mock_provider.provider_type, EmbeddingProviderType::OpenAI);
        assert_eq!(mock_provider.api.key, Some("test-api-key".to_string()));
        assert_eq!(mock_provider.model.name, "text-embedding-3-small");
        assert_eq!(mock_provider.api.timeout_seconds, Some(30));
        assert_eq!(mock_provider.api.retry_attempts, Some(3));
    }

    #[test]
    fn test_mock_ollama_provider_configuration() {
        let mock_provider = TestConfigBuilder::new()
            .mock_ollama_embedding()
            .build()
            .embedding_provider()
            .expect("Failed to get mock Ollama provider");

        assert_eq!(mock_provider.provider_type, EmbeddingProviderType::Ollama);
        assert_eq!(
            mock_provider.api.base_url,
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(mock_provider.model.name, "nomic-embed-text");
        assert!(mock_provider.api.key.is_none());
    }

    #[test]
    fn test_deterministic_fixture_data() {
        // Verify that fixtures provide deterministic, predictable data
        let provider1 = openai_provider_fixture();
        let provider2 = openai_provider_fixture();

        assert_eq!(provider1, provider2);
        assert_eq!(
            provider1.api.key.as_ref().unwrap(),
            "sk-test-deterministic-key-12345"
        );
        assert_eq!(provider1.model.name, "text-embedding-3-small");
    }

    #[test]
    fn test_custom_mock_provider() {
        let custom_provider = custom_provider_fixture();

        assert!(
            matches!(custom_provider.provider_type, EmbeddingProviderType::Custom(ref s) if s == "huggingface")
        );
        assert_eq!(
            custom_provider.api.key.as_ref().unwrap(),
            "hf-test-key-67890"
        );
        assert_eq!(
            custom_provider.model.name,
            "sentence-transformers/all-MiniLM-L6-v2"
        );
        assert_eq!(custom_provider.model.dimensions, Some(384));

        // Verify custom options
        assert!(custom_provider.options.contains_key("use_cache"));
        assert!(custom_provider.options.contains_key("wait_for_model"));
        assert_eq!(custom_provider.options.get("use_cache"), Some(&json!(true)));
    }
}

/// Test suite for configuration validation and error handling
#[cfg(test)]
mod validation_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn test_validate_openai_provider_with_missing_key() {
        let mut invalid_provider = openai_provider_fixture();
        invalid_provider.api.key = None;

        let result = invalid_provider.validate();
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, ProviderError::MissingField { field } if field == "api.key"));
    }

    #[test]
    fn test_validate_provider_with_empty_model_name() {
        let mut invalid_provider = openai_provider_fixture();
        invalid_provider.model.name = String::new();

        let result = invalid_provider.validate();
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, ProviderError::InvalidModel { model } if model.is_empty()));
    }

    #[test]
    fn test_validate_ollama_provider_without_key() {
        let ollama_provider = ollama_provider_fixture();

        // Ollama provider should validate successfully even without API key
        assert!(ollama_provider.validate().is_ok());
    }

    #[tokio::test]
    async fn test_invalid_configuration_file_format() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_path = temp_dir.path().join("invalid.yaml");

        // Write invalid YAML
        let invalid_yaml = "embedding_provider:\n  type: openai\n  api:\n    key: [invalid yaml";
        fs::write(&config_path, invalid_yaml)
            .await
            .expect("Failed to write invalid YAML");

        // Should fail to load invalid configuration
        let result = ConfigLoader::load_from_file(&config_path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Yaml(_)));
    }

    #[test]
    fn test_configuration_with_unknown_provider_type() {
        let config_json = json!({
            "profile": "test",
            "profiles": {
                "test": {
                    "name": "test",
                    "environment": "test"
                }
            },
            "embedding_provider": {
                "type": "unknown_provider",
                "api": {
                    "key": "test-key"
                },
                "model": {
                    "name": "test-model"
                }
            }
        });

        // Should fail to deserialize unknown provider type
        let result: Result<EmbeddingProviderConfig, _> =
            serde_json::from_value(config_json["embedding_provider"].clone());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_configuration_file_not_found() {
        let non_existent_path = "/tmp/non_existent_crucible_config.yaml";

        let result = ConfigLoader::load_from_file(non_existent_path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Io(_)));
    }

    #[test]
    fn test_invalid_timeout_values() {
        let mut provider = openai_provider_fixture();

        // Test zero timeout (should be invalid)
        provider.api.timeout_seconds = Some(0);
        // Note: This would need custom validation to be implemented

        // Test very large timeout (should be valid but may indicate configuration issues)
        provider.api.timeout_seconds = Some(u64::MAX);
        // Note: This would need custom validation to be implemented
    }
}

/// Test suite for test environment isolation
#[cfg(test)]
mod environment_isolation_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn test_test_environment_isolation() {
        // Verify that test environment variables don't interfere with tests
        let original_vars = std::env::vars().collect::<HashMap<_, _>>();

        // Set some test environment variables
        std::env::set_var("CRUCIBLE_TEST_VAR", "should_not_interfere");
        std::env::set_var("OPENAI_API_KEY", "should_not_interfere");

        let config = TestConfig::minimal();
        let embedding_provider = config
            .embedding_provider()
            .expect("Failed to get embedding provider in isolated test");

        // Should use test configuration, not environment variables
        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );
        assert_eq!(embedding_provider.api.key.as_ref().unwrap(), "test-api-key");

        // Restore original environment
        for (key, _) in std::env::vars() {
            if !original_vars.contains_key(&key) {
                std::env::remove_var(&key);
            }
        }
        for (key, value) in original_vars {
            std::env::set_var(&key, value);
        }
    }

    #[tokio::test]
    async fn test_concurrent_config_loading() {
        let config = complete_config_fixture();

        // Create multiple temporary config files and keep them alive
        let temp_files: Vec<_> = (0..5)
            .map(|_| TempConfig::create_temp_file(&config))
            .collect();

        let paths: Vec<_> = temp_files.iter().map(|(_, path)| path.clone()).collect();

        // Load configurations concurrently
        let handles: Vec<_> = paths
            .into_iter()
            .map(|path| tokio::spawn(async move { ConfigLoader::load_from_file(&path).await }))
            .collect();

        // Wait for all loads to complete
        let results: Vec<_> = futures::future::join_all(handles).await;

        // All loads should succeed
        for result in results {
            let loaded_config = result
                .expect("Task panicked")
                .expect("Failed to load config");
            let embedding_provider = loaded_config
                .embedding_provider()
                .expect("Failed to get embedding provider");
            assert_eq!(
                embedding_provider.provider_type,
                EmbeddingProviderType::OpenAI
            );
        }

        // temp_files will be cleaned up when they go out of scope here
    }

    #[test]
    fn test_temporary_file_cleanup() {
        let config = complete_config_fixture();
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create temporary config file
        let config_path = temp_dir.path().join("test_config.yaml");
        let config_content = serde_yaml::to_string(&config).expect("Failed to serialize config");
        std::fs::write(&config_path, config_content).expect("Failed to write config file");

        // Verify file exists
        assert!(config_path.exists());

        // Load configuration
        let loaded_config: Config = {
            let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
            serde_yaml::from_str(&content).expect("Failed to deserialize config")
        };

        // Verify configuration loaded correctly
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get embedding provider");
        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );

        // Temporary directory will be cleaned up when temp_dir goes out of scope
    }

    #[test]
    fn test_environment_variable_override_isolation() {
        let config = complete_config_fixture();

        // Simulate environment variable overrides
        let original_vars = TestEnv::set_test_vars();

        // Apply overrides to a copy of the config
        let mut config_with_overrides = config.clone();
        ConfigLoader::apply_env_overrides(&mut config_with_overrides);

        // Verify overrides were applied
        assert_eq!(config_with_overrides.profile, Some("test".to_string()));

        let embedding_provider = config_with_overrides
            .embedding_provider()
            .expect("Failed to get embedding provider after overrides");
        assert_eq!(embedding_provider.api.key, Some("test-key".to_string()));

        let database = config_with_overrides
            .database()
            .expect("Failed to get database after overrides");
        assert_eq!(database.url, ":memory:");

        let server = config_with_overrides
            .server()
            .expect("Failed to get server after overrides");
        assert_eq!(server.host, "127.0.0.1");
        assert_eq!(server.port, 3000);

        let logging = config_with_overrides.logging();
        assert_eq!(logging.level, "debug");

        // Restore original environment
        TestEnv::clear_test_vars(original_vars);
    }
}

/// Integration tests for embedding configuration with thread pool
#[cfg(test)]
mod integration_tests {
    use super::*;
    use fixtures::*;

    #[tokio::test]
    async fn test_embedding_config_integration() {
        // This test verifies that the embedding system can use crucible-config
        // instead of environment variables
        let config = complete_config_fixture();

        // This should eventually replace the current LlmEmbeddingConfig::from_env() usage
        // in embedding_pool.rs
        let embedding_provider = config
            .embedding_provider()
            .expect("Failed to get embedding provider for integration test");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::OpenAI
        );
        assert!(embedding_provider.validate().is_ok());

        // TODO: This test should eventually create an actual embedding thread pool
        // using the crucible-config structure instead of environment variables
        // let thread_pool = EmbeddingThreadPool::from_config(embedding_provider).await
        //     .expect("Failed to create thread pool from config");
    }

    #[tokio::test]
    async fn test_profile_based_embedding_config() {
        let mut config = Config::new();

        // Create a development profile with Ollama provider
        let _dev_profile = ProfileConfig::new("development".to_string(), Environment::Development);

        // Add embedding provider to the profile
        let ollama_provider = ollama_provider_fixture();

        let dev_profile_with_embedding = ProfileConfig {
            name: "development".to_string(),
            description: Some("Development environment with Ollama".to_string()),
            environment: Environment::Development,
            embedding_provider: Some(ollama_provider),
            database: None,
            server: None,
            logging: None,
            env_vars: std::collections::HashMap::new(),
            settings: std::collections::HashMap::new(),
        };

        config
            .profiles
            .insert("development".to_string(), dev_profile_with_embedding);
        config.profile = Some("development".to_string());

        // Get active profile and verify embedding provider
        let embedding_provider = config
            .embedding_provider()
            .expect("Failed to get embedding provider from profile");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::Ollama
        );
        assert_eq!(embedding_provider.model.name, "nomic-embed-text");
    }

    #[tokio::test]
    async fn test_config_migration_from_env_to_structured() {
        // This test simulates migrating from environment-based configuration
        // to structured configuration files

        // Set up environment variables (simulating current system)
        let env_vars = TestEnv::set_test_vars();

        // Create structured config that should replace environment variables
        let structured_config = TestConfigBuilder::new()
            .profile("migrated")
            .embedding_provider(EmbeddingProviderConfig::openai(
                env_vars.get("CRUCIBLE_EMBEDDING_API_KEY").unwrap().clone(),
                Some("text-embedding-3-small".to_string()),
            ))
            .memory_database()
            .debug_logging()
            .build();

        // Verify structured config provides same values as environment
        let embedding_provider = structured_config
            .embedding_provider()
            .expect("Failed to get embedding provider from migrated config");

        assert_eq!(
            embedding_provider.api.key,
            env_vars.get("CRUCIBLE_EMBEDDING_API_KEY").cloned()
        );

        // Cleanup
        TestEnv::clear_test_vars(env_vars);
    }

    #[test]
    fn test_configuration_comparisons() {
        let config1 = complete_config_fixture();
        let config2 = complete_config_fixture();

        // Identical configurations should be equal
        assert_eq!(
            config1.embedding_provider().unwrap(),
            config2.embedding_provider().unwrap()
        );

        // Different configurations should not be equal
        let different_config = TestConfigBuilder::new()
            .profile("test")
            .embedding_provider(ollama_provider_fixture())
            .memory_database()
            .debug_logging()
            .build();

        assert_ne!(
            config1.embedding_provider().unwrap(),
            different_config.embedding_provider().unwrap()
        );
    }
}

/// Performance and stress tests
#[cfg(test)]
mod performance_tests {
    use super::*;
    use fixtures::*;
    use std::time::Instant;

    #[test]
    fn test_config_serialization_performance() {
        let config = complete_config_fixture();
        let iterations = 1000;

        // Test JSON serialization performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _json_str = serde_json::to_string(&config).expect("Failed to serialize");
        }
        let json_duration = start.elapsed();

        // Test YAML serialization performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _yaml_str = serde_yaml::to_string(&config).expect("Failed to serialize");
        }
        let yaml_duration = start.elapsed();

        // Performance should be reasonable (these are loose bounds)
        assert!(
            json_duration.as_millis() < 1000,
            "JSON serialization too slow: {:?}",
            json_duration
        );
        assert!(
            yaml_duration.as_millis() < 2000,
            "YAML serialization too slow: {:?}",
            yaml_duration
        );

        println!(
            "JSON serialization: {} iterations in {:?}",
            iterations, json_duration
        );
        println!(
            "YAML serialization: {} iterations in {:?}",
            iterations, yaml_duration
        );
    }

    #[test]
    fn test_config_deserialization_performance() {
        let config = complete_config_fixture();
        let json_str = serde_json::to_string(&config).expect("Failed to serialize");
        let yaml_str = serde_yaml::to_string(&config).expect("Failed to serialize");

        let iterations = 1000;

        // Test JSON deserialization performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _: Config = serde_json::from_str(&json_str).expect("Failed to deserialize");
        }
        let json_duration = start.elapsed();

        // Test YAML deserialization performance
        let start = Instant::now();
        for _ in 0..iterations {
            let _: Config = serde_yaml::from_str(&yaml_str).expect("Failed to deserialize");
        }
        let yaml_duration = start.elapsed();

        // Performance should be reasonable
        assert!(
            json_duration.as_millis() < 1000,
            "JSON deserialization too slow: {:?}",
            json_duration
        );
        assert!(
            yaml_duration.as_millis() < 2000,
            "YAML deserialization too slow: {:?}",
            yaml_duration
        );

        println!(
            "JSON deserialization: {} iterations in {:?}",
            iterations, json_duration
        );
        println!(
            "YAML deserialization: {} iterations in {:?}",
            iterations, yaml_duration
        );
    }

    #[test]
    fn test_large_config_handling() {
        // Create a configuration with many profiles and settings
        let mut large_config = TestConfigBuilder::new()
            .profile("default")
            .embedding_provider(openai_provider_fixture())
            .memory_database()
            .debug_logging()
            .build();

        // Add many profiles
        for i in 0..100 {
            let profile = TestConfigBuilder::new()
                .embedding_provider(if i % 2 == 0 {
                    openai_provider_fixture()
                } else {
                    ollama_provider_fixture()
                })
                .memory_database()
                .debug_logging()
                .set(format!("profile_setting_{}", i), i)
                .build();

            large_config.profiles.insert(
                format!("profile_{}", i),
                profile.profiles.into_values().next().unwrap(),
            );
        }

        // Test serialization/deserialization of large config
        let json_str =
            serde_json::to_string(&large_config).expect("Failed to serialize large config");
        let deserialized: Config =
            serde_json::from_str(&json_str).expect("Failed to deserialize large config");

        assert_eq!(large_config.profiles.len(), deserialized.profiles.len());

        // Test accessing embedding provider from large config
        let embedding_provider = deserialized.embedding_provider();
        assert!(embedding_provider.is_ok());
    }
}

/// RED Phase: Candle provider configuration tests (will fail until implementation)
#[cfg(test)]
mod candle_provider_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn test_candle_provider_configuration() {
        let candle_provider = candle_provider_fixture();

        assert_eq!(candle_provider.provider_type, EmbeddingProviderType::Candle);
        assert_eq!(candle_provider.api.base_url, Some("local".to_string()));
        assert_eq!(candle_provider.model.name, "nomic-embed-text-v1.5");
        assert_eq!(candle_provider.model.dimensions, Some(768));
        assert!(candle_provider.api.key.is_none()); // Candle doesn't require API key
        assert!(candle_provider.validate().is_ok());

        // Verify Candle-specific options
        assert_eq!(
            candle_provider.options.get("model_cache_dir"),
            Some(&json!("/tmp/candle-models"))
        );
        assert_eq!(
            candle_provider.options.get("memory_limit_mb"),
            Some(&json!(4096))
        );
        assert_eq!(candle_provider.options.get("device"), Some(&json!("cpu")));
    }

    #[test]
    fn test_candle_provider_different_models() {
        let models = vec![
            ("nomic-embed-text-v1.5", 768),
            ("jina-embeddings-v2-base-en", 768),
            ("jina-embeddings-v3-base-en", 768),
            ("all-MiniLM-L6-v2", 384),
            ("bge-small-en-v1.5", 384),
        ];

        for (model_name, expected_dims) in models {
            let mut candle_config = candle_provider_fixture();
            candle_config.model.name = model_name.to_string();
            candle_config.model.dimensions = Some(expected_dims);

            assert!(candle_config.validate().is_ok());
            assert_eq!(candle_config.model.name, model_name);
            assert_eq!(candle_config.model.dimensions, Some(expected_dims));
        }
    }

    #[test]
    fn test_candle_provider_device_options() {
        let devices = vec!["cpu", "cuda", "metal"];

        for device in devices {
            let mut candle_config = candle_provider_fixture();
            candle_config
                .options
                .insert("device".to_string(), json!(device));

            assert!(candle_config.validate().is_ok());
            assert_eq!(candle_config.options.get("device"), Some(&json!(device)));
        }
    }

    #[test]
    fn test_candle_provider_memory_configuration() {
        let memory_limits = vec![1024, 2048, 4096, 8192];

        for memory_mb in memory_limits {
            let mut candle_config = candle_provider_fixture();
            candle_config
                .options
                .insert("memory_limit_mb".to_string(), json!(memory_mb));

            assert!(candle_config.validate().is_ok());
            assert_eq!(
                candle_config.options.get("memory_limit_mb"),
                Some(&json!(memory_mb))
            );
        }
    }

    #[test]
    fn test_candle_provider_cache_configuration() {
        let cache_dirs = vec![
            "/tmp/candle-models",
            "/var/cache/candle",
            "/home/user/.cache/candle",
        ];

        for cache_dir in cache_dirs {
            let mut candle_config = candle_provider_fixture();
            candle_config
                .options
                .insert("model_cache_dir".to_string(), json!(cache_dir));

            assert!(candle_config.validate().is_ok());
            assert_eq!(
                candle_config.options.get("model_cache_dir"),
                Some(&json!(cache_dir))
            );
        }
    }

    #[test]
    fn test_candle_provider_serialization() {
        let candle_provider = candle_provider_fixture();

        // Test JSON serialization
        let json_str = serde_json::to_string_pretty(&candle_provider)
            .expect("Failed to serialize Candle provider");
        let deserialized: EmbeddingProviderConfig =
            serde_json::from_str(&json_str).expect("Failed to deserialize Candle provider");

        assert_eq!(candle_provider, deserialized);

        // Test YAML serialization
        let yaml_str = serde_yaml::to_string(&candle_provider)
            .expect("Failed to serialize Candle provider to YAML");
        let yaml_deserialized: EmbeddingProviderConfig = serde_yaml::from_str(&yaml_str)
            .expect("Failed to deserialize Candle provider from YAML");

        assert_eq!(candle_provider, yaml_deserialized);
    }

    #[tokio::test]
    async fn test_load_config_with_candle_provider() {
        let config = TestConfigBuilder::new()
            .profile("test")
            .embedding_provider(candle_provider_fixture())
            .memory_database()
            .debug_logging()
            .build();

        let (_temp_file, config_path) = TempConfig::create_temp_file(&config);

        // Load configuration from file
        let loaded_config = ConfigLoader::load_from_file(&config_path)
            .await
            .expect("Failed to load configuration with Candle provider");

        // Verify Candle provider configuration
        let embedding_provider = loaded_config
            .embedding_provider()
            .expect("Failed to get Candle embedding provider");

        assert_eq!(
            embedding_provider.provider_type,
            EmbeddingProviderType::Candle
        );
        assert_eq!(embedding_provider.model.name, "nomic-embed-text-v1.5");
        assert!(embedding_provider.api.key.is_none());
        assert_eq!(
            embedding_provider.options.get("device"),
            Some(&json!("cpu"))
        );
    }

    #[test]
    fn test_candle_provider_model_validation() {
        let valid_models = vec![
            "nomic-embed-text-v1.5",
            "jina-embeddings-v2-base-en",
            "jina-embeddings-v3-base-en",
            "all-MiniLM-L6-v2",
            "bge-small-en-v1.5",
        ];

        for model in valid_models {
            let mut candle_config = candle_provider_fixture();
            candle_config.model.name = model.to_string();

            assert!(
                candle_config.validate().is_ok(),
                "Model {} should be valid",
                model
            );
        }

        // Test invalid model name
        let mut candle_config = candle_provider_fixture();
        candle_config.model.name = "invalid-model-name".to_string();

        // This should still validate since we don't enforce strict model validation at the config level
        // The actual validation would happen at runtime when trying to load the model
        assert!(candle_config.validate().is_ok());
    }

    #[test]
    fn test_candle_provider_type_methods() {
        let candle_type = EmbeddingProviderType::Candle;

        assert!(!candle_type.requires_api_key());
        assert_eq!(candle_type.default_base_url(), Some("local".to_string()));
        assert_eq!(
            candle_type.default_model(),
            Some("nomic-embed-text-v1.5".to_string())
        );
    }

    #[test]
    fn test_candle_provider_edge_cases() {
        // Test with empty options
        let mut candle_config = candle_provider_fixture();
        candle_config.options.clear();
        assert!(candle_config.validate().is_ok());

        // Test with additional custom options
        candle_config
            .options
            .insert("custom_option".to_string(), json!("custom_value"));
        candle_config
            .options
            .insert("batch_size".to_string(), json!(32));
        assert!(candle_config.validate().is_ok());

        // Test with custom headers (should be allowed but not used by Candle)
        candle_config
            .api
            .headers
            .insert("Custom-Header".to_string(), "custom-value".to_string());
        assert!(candle_config.validate().is_ok());
    }

    #[test]
    fn test_candle_provider_timeout_configuration() {
        let timeouts = vec![30, 60, 120, 300];

        for timeout in timeouts {
            let mut candle_config = candle_provider_fixture();
            candle_config.api.timeout_seconds = Some(timeout);

            assert!(candle_config.validate().is_ok());
            assert_eq!(candle_config.api.timeout_seconds, Some(timeout));
        }
    }
}
