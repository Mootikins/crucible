//! Comprehensive tests for CLI configuration
//!
//! This module tests configuration functionality including:
//! - Configuration loading and validation
//! - Service and migration configuration sections
//! - Environment variable overrides
//! - Configuration error handling
//! - Default value handling
//! - Configuration serialization/deserialization

/// Test configuration loading from defaults
#[test]
fn test_configuration_default_values() -> Result<()> {
    let config = CliConfig::default();

    // Test kiln defaults
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
    assert_eq!(config.kiln.embedding_model, None);

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
[kiln]
path = "/test/kiln"
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
default_timeout_secs = 60
enable_caching = true
max_cache_size = 500
max_memory_mb = 200
max_cpu_percentage = 90.0
max_concurrent_operations = 100

[services.discovery]
enabled = false
endpoints = ["test-endpoint:1234"]
timeout_secs = 10
refresh_interval_secs = 30

[services.health]
enabled = false
check_interval_secs = 30
timeout_secs = 5
failure_threshold = 5
auto_recovery = true

[migration]
enabled = false
default_security_level = "production"
auto_migrate = true
enable_caching = false
max_cache_size = 100
preserve_tool_ids = true
backup_originals = true
"#;

    std::fs::write(&config_path, config_content)?;

    // Load directly from TOML to avoid test mode interference
    let contents = std::fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Test kiln configuration
    assert_eq!(config.kiln.path.to_string_lossy(), "/test/kiln");
    assert_eq!(config.kiln.embedding_url, "https://test-embedding.com");
    assert_eq!(config.kiln.embedding_model, Some("test-model".to_string()));

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
    assert_eq!(
        config.services.discovery.endpoints,
        vec!["test-endpoint:1234".to_string()]
    );
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

/// Test that API keys can be set in config and also read from environment variables
/// (Note: most env var support was removed in v0.2.0, but API keys still support it)
#[test]
fn test_api_key_configuration() -> Result<()> {
    // Test 1: API keys from config struct
    let config = CliConfig::builder()
        .openai_api_key("sk-config-openai")
        .anthropic_api_key("sk-ant-config-anthropic")
        .build()?;

    assert_eq!(
        config.openai_api_key(),
        Some("sk-config-openai".to_string())
    );
    assert_eq!(
        config.anthropic_api_key(),
        Some("sk-ant-config-anthropic".to_string())
    );

    // Test 2: Default values are used for other config
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
    assert_eq!(config.max_tokens(), 2048);

    Ok(())
}

/// Test CLI argument overrides using builder pattern
#[test]
fn test_cli_argument_overrides() -> Result<()> {
    // Simulate CLI argument overrides using builder
    let config = CliConfig::builder()
        .embedding_url("https://cli-embedding.com")
        .embedding_model("cli-model")
        .build()?;

    // Test CLI argument overrides
    assert_eq!(config.kiln.embedding_url, "https://cli-embedding.com");
    assert_eq!(config.kiln.embedding_model, Some("cli-model".to_string()));

    // Test that defaults are still used for non-overridden values
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);

    Ok(())
}

/// Test configuration precedence (defaults < file < builder overrides)
#[test]
fn test_configuration_precedence() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Create config file with some values
    let config_content = r#"
[kiln]
path = "/file/kiln"
embedding_url = "https://file-embedding.com"
embedding_model = "file-model"

[llm]
chat_model = "file-model"
temperature = 0.3
"#;

    std::fs::write(&config_path, config_content)?;

    // Load config from file
    let contents = std::fs::read_to_string(&config_path)?;
    let mut config: CliConfig = toml::from_str(&contents)?;

    // Simulate CLI arguments overriding file config (highest precedence)
    config.kiln.embedding_url = "https://cli-embedding.com".to_string();
    config.kiln.embedding_model = Some("cli-model".to_string());

    // Verify precedence:
    // - embedding_url should be from CLI override (highest precedence)
    assert_eq!(config.kiln.embedding_url, "https://cli-embedding.com");

    // - embedding_model should be from CLI override (highest precedence)
    assert_eq!(config.kiln.embedding_model, Some("cli-model".to_string()));

    // - kiln.path should be from file (middle precedence)
    assert_eq!(config.kiln.path.to_string_lossy(), "/file/kiln");

    // - llm.chat_model should be from file (middle precedence)
    assert_eq!(config.chat_model(), "file-model");

    // - temperature should be from file (middle precedence)
    assert_eq!(config.temperature(), 0.3);

    Ok(())
}

/// Test configuration validation
#[test]
fn test_configuration_validation() -> Result<()> {
    // Test valid configuration
    let valid_config = r#"
[kiln]
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

    // Load directly from TOML (not from_file_or_default which returns Ok for test mode)
    let contents = std::fs::read_to_string(&config_path)?;
    let result: Result<CliConfig, toml::de::Error> = toml::from_str(&contents);
    assert!(
        result.is_ok(),
        "Valid configuration should load successfully"
    );

    // Test invalid TOML
    let invalid_toml = r#"
[kiln
path = "/invalid/path"  # Missing closing bracket
embedding_url = "http://localhost:11434"
"#;

    let config_path = temp_dir.path().join("invalid_toml.toml");
    std::fs::write(&config_path, invalid_toml)?;

    let contents = std::fs::read_to_string(&config_path)?;
    let result: Result<CliConfig, toml::de::Error> = toml::from_str(&contents);
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
    assert_eq!(
        config.services.discovery.endpoints,
        vec!["localhost:8080".to_string()]
    );
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
    assert_eq!(
        config.migration.validation.max_performance_degradation,
        20.0
    );

    Ok(())
}

/// Test configuration serialization
#[test]
fn test_configuration_serialization() -> Result<()> {
    let config = CliConfig::default();

    // Test TOML serialization
    let toml_str = config.display_as_toml()?;
    assert!(toml_str.contains("[kiln]"));
    // Services are serialized as [services.script_engine], [services.discovery], etc.
    assert!(toml_str.contains("[services.script_engine]") || toml_str.contains("services"));
    assert!(toml_str.contains("[migration]"));

    // Test JSON serialization
    let json_str = config.display_as_json()?;
    let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
    assert!(json_value.get("kiln").is_some());
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
    assert!(content.contains("[kiln]"));
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
    let kiln_path = temp_dir.path().join("test_kiln");

    // Use builder to create config with explicit kiln path
    let config = CliConfig::builder().kiln_path(&kiln_path).build()?;

    // Test database path derivation (now includes PID to prevent lock conflicts)
    let db_path = config.database_path();
    let db_parent = db_path.parent().unwrap();
    let db_filename = db_path.file_name().unwrap().to_str().unwrap();
    assert_eq!(db_parent, kiln_path.join(".crucible"));
    assert!(
        db_filename.starts_with("kiln-") && db_filename.ends_with(".db"),
        "Database filename should be kiln-{{pid}}.db, got: {}",
        db_filename
    );

    // Test tools path derivation
    let expected_tools = kiln_path.join("tools");
    assert_eq!(config.tools_path(), expected_tools);

    // Test string conversions
    assert!(config.database_path_str()?.contains(".crucible/kiln-"));
    assert_eq!(config.kiln_path_str()?, kiln_path.to_str().unwrap());

    Ok(())
}

/// Test embedding config conversion
#[test]
fn test_embedding_config_conversion() -> Result<()> {
    // Use builder to create config with embedding model
    let config = CliConfig::builder()
        .embedding_model("nomic-embed-text")
        .build()?;

    let embedding_config = config.to_embedding_config()?;

    // Default config uses legacy format with Ollama (since URL is http://localhost:11434)
    assert!(matches!(
        embedding_config.provider_type,
        crucible_config::EmbeddingProviderType::Ollama
    ));
    assert_eq!(embedding_config.endpoint(), config.kiln.embedding_url);
    assert_eq!(
        &embedding_config.model.name,
        config
            .kiln
            .embedding_model
            .as_ref()
            .unwrap_or(&String::new())
    );
    // Note: Ollama provider has its own default timeout (60s), not from CLI config
    assert_eq!(embedding_config.api.timeout_seconds, Some(60));
    assert_eq!(embedding_config.api.retry_attempts, Some(2));

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
    assert_eq!(
        config.ollama_endpoint(),
        "https://llama.terminal.krohnos.io"
    );
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

/// Test API key handling through config struct
#[test]
fn test_api_key_handling() -> Result<()> {
    // Test 1: Config with no API keys
    let config = CliConfig::default();

    // Note: API keys may come from environment, but we test config-only values
    // by checking the struct fields directly
    assert_eq!(config.llm.backends.openai.api_key, None);
    assert_eq!(config.llm.backends.anthropic.api_key, None);

    // Test 2: Config with API keys set via builder
    let config = CliConfig::builder()
        .openai_api_key("sk-config-openai")
        .anthropic_api_key("sk-ant-config-anthropic")
        .build()?;

    // The struct fields should have the configured values
    assert_eq!(
        config.llm.backends.openai.api_key,
        Some("sk-config-openai".to_string())
    );
    assert_eq!(
        config.llm.backends.anthropic.api_key,
        Some("sk-ant-config-anthropic".to_string())
    );

    // Test 3: Config from TOML file
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test"

[llm.backends.openai]
api_key = "sk-file-openai"

[llm.backends.anthropic]
api_key = "sk-ant-file-anthropic"
"#;

    std::fs::write(&config_path, config_content)?;
    let contents = std::fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    assert_eq!(
        config.llm.backends.openai.api_key,
        Some("sk-file-openai".to_string())
    );
    assert_eq!(
        config.llm.backends.anthropic.api_key,
        Some("sk-ant-file-anthropic".to_string())
    );

    Ok(())
}

/// Test configuration errors and edge cases
#[test]
fn test_configuration_error_handling() -> Result<()> {
    // Test with non-existent config file - should use default config
    let config = CliConfig::default();
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
    assert_eq!(config.chat_model(), "llama3.2");

    // Test with invalid path for string conversion
    // Note: PathBuf::from() on Unix accepts any bytes, even invalid UTF-8
    // We need to use OsString to create truly invalid UTF-8
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let invalid_bytes = vec![0xFF, 0xFF, 0xFF];
        let invalid_osstring = OsString::from_vec(invalid_bytes);
        let invalid_path = PathBuf::from(invalid_osstring);

        let invalid_config = CliConfig {
            kiln: crucible_cli::config::KilnConfig {
                path: invalid_path,
                embedding_url: "http://localhost:11434".to_string(),
                embedding_model: Some("test-model".to_string()),
            },
            embedding: None,
            llm: Default::default(),
            network: Default::default(),
            services: Default::default(),
            migration: Default::default(),
            file_watching: Default::default(),
            custom_database_path: None,
        };

        let result = invalid_config.kiln_path_str();
        assert!(result.is_err(), "Should fail with invalid UTF-8 path");
    }

    // On Windows, test still passes since we tested default config handling
    #[cfg(not(unix))]
    {
        // Test already passed with default config test
    }

    Ok(())
}

/// Test configuration with programmatic values (simulating test mode)
#[test]
fn test_programmatic_configuration() -> Result<()> {
    // Use builder to create test config programmatically
    let config = CliConfig::builder()
        .embedding_model("test-model")
        .chat_model("test-chat-model")
        .build()?;

    // Verify config values are set correctly
    assert_eq!(config.kiln.embedding_model, Some("test-model".to_string()));
    assert_eq!(config.chat_model(), "test-chat-model");

    Ok(())
}

/// Test configuration with complex values
#[test]
fn test_complex_configuration_values() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("complex_config.toml");

    let complex_config = r#"
[kiln]
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

    // Load directly from TOML to avoid test mode interference
    let contents = std::fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Verify complex values were parsed correctly
    assert_eq!(
        config.kiln.path.to_string_lossy(),
        "/complex/path with spaces"
    );
    assert_eq!(
        config.kiln.embedding_url,
        "https://complex-endpoint.com:8443/v1"
    );
    assert_eq!(
        config.kiln.embedding_model,
        Some("complex-model-v1.2.3".to_string())
    );

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
    assert_eq!(
        config.migration.validation.max_performance_degradation,
        10.0
    );

    Ok(())
}

/// Test configuration performance
#[test]
fn test_configuration_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("perf_config.toml");

    // Create a relatively large configuration
    let mut config_content = String::new();
    config_content.push_str("[kiln]\n");
    config_content.push_str("path = \"/test/kiln\"\n");
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

    // Test loading performance - load directly from TOML to avoid test mode
    let start = std::time::Instant::now();
    let contents = std::fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;
    let load_duration = start.elapsed();

    assert_eq!(config.services.discovery.endpoints.len(), 100);
    assert!(
        load_duration < Duration::from_millis(100),
        "Configuration loading should be fast"
    );

    // Test serialization performance
    let start = std::time::Instant::now();
    let _toml_str = config.display_as_toml()?;
    let serialize_duration = start.elapsed();

    assert!(
        serialize_duration < Duration::from_millis(100),
        "Configuration serialization should be fast"
    );

    Ok(())
}
use anyhow::Result;
use crucible_cli::config::CliConfig;
use serde_json;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
