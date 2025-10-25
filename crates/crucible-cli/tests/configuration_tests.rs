//! Comprehensive tests for CLI configuration
//!
//! This module tests configuration functionality including:
//! - Configuration loading and validation
//! - Service and migration configuration sections
//! - Environment variable overrides
//! - Configuration error handling
//! - Default value handling
//! - Configuration serialization/deserialization

mod test_utilities;

use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use serde_json;
use crucible_cli::test_utilities::*;
use crucible_cli::config::CliConfig;

/// Test configuration loading from defaults
#[test]
fn test_configuration_default_values() -> Result<()> {
    let config = CliConfig::default();

    // Test vault defaults
    assert_eq!(config.vault.embedding_url, "http://localhost:11434");
    assert_eq!(config.vault.embedding_model, None);

    // Test LLM defaults
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
    assert_eq!(config.max_tokens(), 2048);
    assert!(config.streaming());

    // Test services defaults
    assert!(config.services.script_engine.enabled);
    assert_eq!(config.services.script_engine.security_level, "safe");
    assert!(config.services.discovery.enabled);
    assert!(config.services.health.enabled);

    // Test migration defaults
    assert!(config.migration.enabled);
    assert_eq!(config.migration.default_security_level, "safe");
    assert!(!config.migration.auto_migrate);
    assert!(config.migration.enable_caching);

    Ok(())
}

/// Test configuration loading from file
#[test]
fn test_configuration_file_loading() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[vault]
path = "/test/vault"
embedding_url = "https://test-embedding.com"
embedding_model = "test-model"

[llm]
chat_model = "test-model"
temperature = 0.5
max_tokens = 1024
streaming = false

[services.script_engine]
enabled = false
security_level = "development"
max_source_size = 2048000

[services.discovery]
enabled = false
endpoints = ["test-endpoint:1234"]

[services.health]
enabled = false
check_interval_secs = 30

[migration]
enabled = false
auto_migrate = true
default_security_level = "production"
enable_caching = false
max_cache_size = 100
"#;

    std::fs::write(&config_path, config_content)?;

    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Test vault configuration
    assert_eq!(config.vault.path.to_string_lossy(), "/test/vault");
    assert_eq!(config.vault.embedding_url, "https://test-embedding.com");
    assert_eq!(config.vault.embedding_model, Some("test-model".to_string()));

    // Test LLM configuration
    assert_eq!(config.chat_model(), "test-model");
    assert_eq!(config.temperature(), 0.5);
    assert_eq!(config.max_tokens(), 1024);
    assert!(!config.streaming());

    // Test services configuration
    assert!(!config.services.script_engine.enabled);
    assert_eq!(config.services.script_engine.security_level, "development");
    assert_eq!(config.services.script_engine.max_source_size, 2048000);
    assert!(!config.services.discovery.enabled);
    assert_eq!(config.services.discovery.endpoints, vec!["test-endpoint:1234".to_string()]);
    assert!(!config.services.health.enabled);
    assert_eq!(config.services.health.check_interval_secs, 30);

    // Test migration configuration
    assert!(!config.migration.enabled);
    assert!(config.migration.auto_migrate);
    assert_eq!(config.migration.default_security_level, "production");
    assert!(!config.migration.enable_caching);
    assert_eq!(config.migration.max_cache_size, 100);

    Ok(())
}

/// Test environment variable overrides
#[test]
fn test_environment_variable_overrides() -> Result<()> {
    // Store original environment variables
    let original_vars = std::env::vars().collect::<HashMap<String, String>>();

    // Set test environment variables
    env::set_var("CRUCIBLE_TEST_MODE", "1"); // Skip loading user config
    env::set_var("OBSIDIAN_VAULT_PATH", "/test/env/vault");
    env::set_var("EMBEDDING_ENDPOINT", "https://env-embedding.com");
    env::set_var("EMBEDDING_MODEL", "env-model");
    env::set_var("CRUCIBLE_CHAT_MODEL", "env-chat-model");
    env::set_var("CRUCIBLE_TEMPERATURE", "0.8");
    env::set_var("CRUCIBLE_MAX_TOKENS", "4096");
    env::set_var("OLLAMA_ENDPOINT", "https://env-ollama.com");
    env::set_var("OPENAI_API_KEY", "sk-env-test");
    env::set_var("ANTHROPIC_API_KEY", "sk-ant-env-test");
    env::set_var("CRUCIBLE_TIMEOUT", "60");

    let config = CliConfig::load(None, None, None)?;

    // Test vault environment overrides
    assert_eq!(config.vault.path.to_string_lossy(), "/test/env/vault");
    assert_eq!(config.vault.embedding_url, "https://env-embedding.com");
    assert_eq!(config.vault.embedding_model, Some("env-model".to_string()));

    // Test LLM environment overrides
    assert_eq!(config.chat_model(), "env-chat-model");
    assert_eq!(config.temperature(), 0.8);
    assert_eq!(config.max_tokens(), 4096);
    assert_eq!(config.ollama_endpoint(), "https://env-ollama.com");
    assert_eq!(config.openai_api_key(), Some("sk-env-test".to_string()));
    assert_eq!(config.anthropic_api_key(), Some("sk-ant-env-test".to_string()));
    assert_eq!(config.timeout(), 60);

    // Restore original environment variables
    for (key, value) in original_vars {
        env::set_var(key, value);
    }

    Ok(())
}

/// Test CLI argument overrides
#[test]
fn test_cli_argument_overrides() -> Result<()> {
    env::set_var("CRUCIBLE_TEST_MODE", "1"); // Skip loading user config
    env::set_var("OBSIDIAN_VAULT_PATH", "/test/cli/vault"); // Set required vault path

    let config = CliConfig::load(
        None,
        Some("https://cli-embedding.com".to_string()),
        Some("cli-model".to_string()),
    )?;

    // Test CLI argument overrides
    assert_eq!(config.vault.path.to_string_lossy(), "/test/cli/vault");
    assert_eq!(config.vault.embedding_url, "https://cli-embedding.com");
    assert_eq!(config.vault.embedding_model, Some("cli-model".to_string()));

    env::remove_var("CRUCIBLE_TEST_MODE");

    Ok(())
}

/// Test configuration precedence (defaults < file < env < args)
#[test]
fn test_configuration_precedence() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Create config file with some values
    let config_content = r#"
[vault]
path = "/file/vault"
embedding_url = "https://file-embedding.com"
embedding_model = "file-model"

[llm]
chat_model = "file-model"
temperature = 0.3
"#;

    std::fs::write(&config_path, config_content)?;

    // Set environment variables
    env::set_var("CRUCIBLE_TEST_MODE", "1");
    env::set_var("OBSIDIAN_VAULT_PATH", "/cli/vault"); // Set required vault path
    env::set_var("EMBEDDING_MODEL", "env-model");
    env::set_var("CRUCIBLE_TEMPERATURE", "0.7");

    // Load with CLI arguments
    let config = CliConfig::load(
        Some(config_path),
        Some("https://cli-embedding.com".to_string()),
        None, // Use environment model
    )?;

    // Verify precedence:
    // - vault.path should be from CLI args (highest precedence)
    assert_eq!(config.vault.path.to_string_lossy(), "/cli/vault");

    // - vault.embedding_url should be from CLI args
    assert_eq!(config.vault.embedding_url, "https://cli-embedding.com");

    // - vault.embedding_model should be from environment (middle precedence)
    assert_eq!(config.vault.embedding_model, Some("env-model".to_string()));

    // - llm.chat_model should be from file (lower precedence)
    assert_eq!(config.chat_model(), "file-model");

    // - temperature should be from environment (higher precedence than file)
    assert_eq!(config.temperature(), 0.7);

    // Clean up
    env::remove_var("CRUCIBLE_TEST_MODE");
    env::remove_var("EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_TEMPERATURE");

    Ok(())
}

/// Test configuration validation
#[test]
fn test_configuration_validation() -> Result<()> {
    // Test valid configuration
    let valid_config = r#"
[vault]
path = "/valid/path"
embedding_url = "http://localhost:11434"
embedding_model = "valid-model"

[services.script_engine]
security_level = "safe"
max_source_size = 1048576

[migration]
default_security_level = "safe"
"#;

    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("valid_config.toml");
    std::fs::write(&config_path, valid_config)?;

    let result = CliConfig::from_file_or_default(Some(config_path));
    assert!(result.is_ok(), "Valid configuration should load successfully");

    // Test invalid TOML
    let invalid_toml = r#"
[vault
path = "/invalid/path"  # Missing closing bracket
embedding_url = "http://localhost:11434"
"#;

    let config_path = temp_dir.path().join("invalid_toml.toml");
    std::fs::write(&config_path, invalid_toml)?;

    let result = CliConfig::from_file_or_default(Some(config_path));
    assert!(result.is_err(), "Invalid TOML should fail to parse");

    Ok(())
}

/// Test service configuration defaults
#[test]
fn test_service_configuration_defaults() -> Result<()> {
    let config = CliConfig::default();

    // Test ScriptEngine defaults
    assert!(config.services.script_engine.enabled);
    assert_eq!(config.services.script_engine.security_level, "safe");
    assert_eq!(config.services.script_engine.max_source_size, 1024 * 1024);
    assert_eq!(config.services.script_engine.default_timeout_secs, 30);
    assert!(config.services.script_engine.enable_caching);
    assert_eq!(config.services.script_engine.max_cache_size, 1000);
    assert_eq!(config.services.script_engine.max_memory_mb, 100);
    assert_eq!(config.services.script_engine.max_cpu_percentage, 80.0);
    assert_eq!(config.services.script_engine.max_concurrent_operations, 50);

    // Test discovery defaults
    assert!(config.services.discovery.enabled);
    assert_eq!(config.services.discovery.endpoints, vec!["localhost:8080".to_string()]);
    assert_eq!(config.services.discovery.timeout_secs, 5);
    assert_eq!(config.services.discovery.refresh_interval_secs, 30);

    // Test health defaults
    assert!(config.services.health.enabled);
    assert_eq!(config.services.health.check_interval_secs, 10);
    assert_eq!(config.services.health.timeout_secs, 5);
    assert_eq!(config.services.health.failure_threshold, 3);
    assert!(config.services.health.auto_recovery);

    Ok(())
}

/// Test migration configuration defaults
#[test]
fn test_migration_configuration_defaults() -> Result<()> {
    let config = CliConfig::default();

    // Test migration defaults
    assert!(config.migration.enabled);
    assert_eq!(config.migration.default_security_level, "safe");
    assert!(!config.migration.auto_migrate);
    assert!(config.migration.enable_caching);
    assert_eq!(config.migration.max_cache_size, 500);
    assert!(config.migration.preserve_tool_ids);
    assert!(config.migration.backup_originals);

    // Test validation defaults
    assert!(config.migration.validation.auto_validate);
    assert!(!config.migration.validation.strict);
    assert!(config.migration.validation.validate_functionality);
    assert!(!config.migration.validation.validate_performance);
    assert_eq!(config.migration.validation.max_performance_degradation, 20.0);

    Ok(())
}

/// Test configuration serialization
#[test]
fn test_configuration_serialization() -> Result<()> {
    let config = CliConfig::default();

    // Test TOML serialization
    let toml_str = config.display_as_toml()?;
    assert!(toml_str.contains("[vault]"));
    assert!(toml_str.contains("[services]"));
    assert!(toml_str.contains("[migration]"));

    // Test JSON serialization
    let json_str = config.display_as_json()?;
    let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
    assert!(json_value.get("vault").is_some());
    assert!(json_value.get("services").is_some());
    assert!(json_value.get("migration").is_some());

    Ok(())
}

/// Test configuration file creation
#[test]
fn test_configuration_file_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("example_config.toml");

    CliConfig::create_example(&config_path)?;

    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path)?;

    // Verify example contains all sections
    assert!(content.contains("[vault]"));
    assert!(content.contains("[llm]"));
    assert!(content.contains("[network]"));
    assert!(content.contains("[services]"));
    assert!(content.contains("[migration]"));

    // Verify example has comments
    assert!(content.contains("#"));
    assert!(content.contains("Crucible CLI Configuration"));

    Ok(())
}

/// Test path derivation
#[test]
fn test_path_derivation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path().join("test_vault");

    // Set required environment variable for security
    env::set_var("OBSIDIAN_VAULT_PATH", &vault_path);

    let config = CliConfig::load(
        None,
        None,
        None,
    )?;

    // Test database path derivation
    let expected_db = vault_path.join(".crucible/embeddings.db");
    assert_eq!(config.database_path(), expected_db);

    // Test tools path derivation
    let expected_tools = vault_path.join("tools");
    assert_eq!(config.tools_path(), expected_tools);

    // Test string conversions
    assert_eq!(config.database_path_str()?, expected_db.to_str().unwrap());
    assert_eq!(config.vault_path_str()?, vault_path.to_str().unwrap());

    Ok(())
}

/// Test embedding config conversion
#[test]
fn test_embedding_config_conversion() -> Result<()> {
    let config = CliConfig::default();

    let embedding_config = config.to_embedding_config()?;

    assert!(matches!(embedding_config.provider, crate::config::ProviderType::Ollama));
    assert_eq!(embedding_config.endpoint, config.vault.embedding_url);
    assert_eq!(embedding_config.model, config.vault.embedding_model);
    assert_eq!(embedding_config.timeout_secs, config.timeout());
    assert_eq!(embedding_config.max_retries, config.network.max_retries.unwrap_or(3));
    assert_eq!(embedding_config.batch_size, 1);

    Ok(())
}

/// Test LLM configuration helpers
#[test]
fn test_llm_configuration_helpers() -> Result<()> {
    let mut config = CliConfig::default();

    // Test with default values
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
    assert_eq!(config.max_tokens(), 2048);
    assert!(config.streaming());
    assert_eq!(config.system_prompt(), "You are a helpful assistant.");
    assert_eq!(config.ollama_endpoint(), "https://llama.terminal.krohnos.io");
    assert_eq!(config.timeout(), 30);

    // Test with custom values
    config.llm.chat_model = Some("custom-model".to_string());
    config.llm.temperature = Some(0.5);
    config.llm.max_tokens = Some(1024);
    config.llm.streaming = Some(false);
    config.llm.system_prompt = Some("Custom prompt".to_string());
    config.llm.backends.ollama.endpoint = Some("https://custom-ollama.com".to_string());
    config.network.timeout_secs = Some(60);

    assert_eq!(config.chat_model(), "custom-model");
    assert_eq!(config.temperature(), 0.5);
    assert_eq!(config.max_tokens(), 1024);
    assert!(!config.streaming());
    assert_eq!(config.system_prompt(), "Custom prompt");
    assert_eq!(config.ollama_endpoint(), "https://custom-ollama.com");
    assert_eq!(config.timeout(), 60);

    Ok(())
}

/// Test API key handling
#[test]
fn test_api_key_handling() -> Result<()> {
    let mut config = CliConfig::default();

    // Test with no API keys
    assert_eq!(config.openai_api_key(), None);
    assert_eq!(config.anthropic_api_key(), None);

    // Test with API keys in config
    config.llm.backends.openai.api_key = Some("sk-config-openai".to_string());
    config.llm.backends.anthropic.api_key = Some("sk-ant-config-anthropic".to_string());

    assert_eq!(config.openai_api_key(), Some("sk-config-openai".to_string()));
    assert_eq!(config.anthropic_api_key(), Some("sk-ant-config-anthropic".to_string()));

    // Test environment variable override
    env::set_var("OPENAI_API_KEY", "sk-env-openai");
    env::set_var("ANTHROPIC_API_KEY", "sk-env-anthropic");

    assert_eq!(config.openai_api_key(), Some("sk-env-openai".to_string()));
    assert_eq!(config.anthropic_api_key(), Some("sk-env-anthropic".to_string()));

    // Clean up
    env::remove_var("OPENAI_API_KEY");
    env::remove_var("ANTHROPIC_API_KEY");

    Ok(())
}

/// Test configuration errors and edge cases
#[test]
fn test_configuration_error_handling() -> Result<()> {
    // Test with non-existent config file
    let non_existent_path = PathBuf::from("/non/existent/path/config.toml");
    let config = CliConfig::from_file_or_default(Some(non_existent_path));
    assert!(config.is_ok(), "Should default to default config when file doesn't exist");

    // Test with invalid path for string conversion
    let invalid_config = CliConfig {
        vault: crate::config::VaultConfig {
            path: PathBuf::from("\0\0\0"), // Invalid UTF-8
            embedding_url: "http://localhost:11434".to_string(),
            embedding_model: "test-model".to_string(),
        },
        llm: Default::default(),
        network: Default::default(),
        services: Default::default(),
        migration: Default::default(),
    };

    let result = invalid_config.vault_path_str();
    assert!(result.is_err(), "Should fail with invalid UTF-8 path");

    Ok(())
}

/// Test configuration with test mode
#[test]
fn test_test_mode_configuration() -> Result<()> {
    // Enable test mode
    env::set_var("CRUCIBLE_TEST_MODE", "1");

    // Set some environment variables
    env::set_var("EMBEDDING_MODEL", "test-model");
    env::set_var("CRUCIBLE_CHAT_MODEL", "test-chat-model");

    let config = CliConfig::load(None, None, None)?;

    // In test mode, should still respect environment variables
    assert_eq!(config.vault.embedding_model, Some("test-model".to_string()));
    assert_eq!(config.chat_model(), "test-chat-model");

    // Clean up
    env::remove_var("CRUCIBLE_TEST_MODE");
    env::remove_var("EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_CHAT_MODEL");

    Ok(())
}

/// Test configuration with complex values
#[test]
fn test_complex_configuration_values() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("complex_config.toml");

    let complex_config = r#"
[vault]
path = "/complex/path with spaces"
embedding_url = "https://complex-endpoint.com:8443/v1"
embedding_model = "complex-model-v1.2.3"

[llm]
chat_model = "complex-chat-model"
temperature = 1.5
max_tokens = 8192
streaming = true
system_prompt = "You are a complex assistant with special characters: !@#$%^&*()"

[llm.backends.ollama]
endpoint = "https://complex-ollama.com:11434"
auto_discover = false

[services.script_engine]
enabled = true
security_level = "production"
max_source_size = 5242880
default_timeout_secs = 120
enable_caching = true
max_cache_size = 2000
max_memory_mb = 512
max_cpu_percentage = 90.0
max_concurrent_operations = 100

[services.discovery]
enabled = true
endpoints = [
    "localhost:8080",
    "service1.example.com:8080",
    "service2.example.com:8080"
]
timeout_secs = 10
refresh_interval_secs = 60

[services.health]
enabled = true
check_interval_secs = 30
timeout_secs = 15
failure_threshold = 5
auto_recovery = true

[migration]
enabled = true
auto_migrate = true
default_security_level = "development"
enable_caching = true
max_cache_size = 1000
preserve_tool_ids = false
backup_originals = false

[migration.validation]
auto_validate = true
strict = true
validate_functionality = true
validate_performance = true
max_performance_degradation = 10.0
"#;

    std::fs::write(&config_path, complex_config)?;

    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Verify complex values were parsed correctly
    assert_eq!(config.vault.path.to_string_lossy(), "/complex/path with spaces");
    assert_eq!(config.vault.embedding_url, "https://complex-endpoint.com:8443/v1");
    assert_eq!(config.vault.embedding_model, Some("complex-model-v1.2.3".to_string()));

    assert_eq!(config.temperature(), 1.5);
    assert_eq!(config.max_tokens(), 8192);
    assert!(config.streaming());

    assert_eq!(config.services.script_engine.security_level, "production");
    assert_eq!(config.services.script_engine.max_source_size, 5242880);
    assert_eq!(config.services.script_engine.max_cpu_percentage, 90.0);

    assert_eq!(
        config.services.discovery.endpoints,
        vec![
            "localhost:8080".to_string(),
            "service1.example.com:8080".to_string(),
            "service2.example.com:8080".to_string(),
        ]
    );

    assert_eq!(config.services.health.check_interval_secs, 30);
    assert_eq!(config.services.health.failure_threshold, 5);

    assert!(config.migration.auto_migrate);
    assert_eq!(config.migration.default_security_level, "development");
    assert!(!config.migration.preserve_tool_ids);

    assert!(config.migration.validation.strict);
    assert!(config.migration.validation.validate_performance);
    assert_eq!(config.migration.validation.max_performance_degradation, 10.0);

    Ok(())
}

/// Test configuration performance
#[test]
fn test_configuration_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("perf_config.toml");

    // Create a relatively large configuration
    let mut config_content = String::new();
    config_content.push_str("[vault]\n");
    config_content.push_str("path = \"/test/vault\"\n");
    config_content.push_str("embedding_url = \"http://localhost:11434\"\n");
    config_content.push_str("embedding_model = \"test-model\"\n\n");

    config_content.push_str("[services.discovery]\n");
    config_content.push_str("enabled = true\n");
    config_content.push_str("endpoints = [\n");

    // Add many endpoints
    for i in 0..100 {
        config_content.push_str(&format!("    \"endpoint{}:8080\",\n", i));
    }

    config_content.push_str("]\n");

    std::fs::write(&config_path, config_content)?;

    // Test loading performance
    let start = std::time::Instant::now();
    let config = CliConfig::from_file_or_default(Some(config_path))?;
    let load_duration = start.elapsed();

    assert!(config.services.discovery.endpoints.len() == 100);
    assert!(load_duration < Duration::from_millis(100), "Configuration loading should be fast");

    // Test serialization performance
    let start = std::time::Instant::now();
    let _toml_str = config.display_as_toml()?;
    let serialize_duration = start.elapsed();

    assert!(serialize_duration < Duration::from_millis(100), "Configuration serialization should be fast");

    Ok(())
}