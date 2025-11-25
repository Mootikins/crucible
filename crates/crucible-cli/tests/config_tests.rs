//! Configuration and error handling integration tests

use crucible_cli::config::CliConfig;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

// ============================================================================
// Configuration Loading Tests
// ============================================================================

#[test]
fn test_config_load_from_nonexistent_file() {
    let temp = TempDir::new().unwrap();
    let nonexistent = temp.path().join("nonexistent.toml");

    // Should fall back to defaults when file doesn't exist
    let result = CliConfig::load(Some(nonexistent), None, None);
    assert!(result.is_ok());
}

#[test]
fn test_config_load_with_invalid_toml() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("invalid.toml");

    // Write invalid TOML
    fs::write(&config_path, "this is not valid toml [[[").unwrap();

    let result = CliConfig::load(Some(config_path), None, None);
    assert!(result.is_err());
}

#[test]
fn test_config_load_with_valid_toml() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("valid.toml");

    fs::write(
        &config_path,
        r#"
[kiln]
path = "/tmp/test-kiln"
embedding_url = "https://example.com"
embedding_model = "test-model"
"#,
    )
    .unwrap();

    let config = CliConfig::load(Some(config_path), None, None).unwrap();
    assert_eq!(config.kiln.path.to_str().unwrap(), "/tmp/test-kiln");
    assert_eq!(config.kiln.embedding_url, "https://example.com");
    assert_eq!(config.kiln.embedding_model, Some("test-model".to_string()));
}

#[test]
fn test_config_cli_overrides() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[kiln]
path = "/tmp/test-kiln"
embedding_url = "https://file-url.com"
embedding_model = "file-model"
"#,
    )
    .unwrap();

    // CLI args should override file config
    let config = CliConfig::load(
        Some(config_path),
        Some("https://cli-url.com".to_string()),
        Some("cli-model".to_string()),
    )
    .unwrap();

    assert_eq!(config.kiln.embedding_url, "https://cli-url.com");
    assert_eq!(config.kiln.embedding_model, Some("cli-model".to_string()));
}

// ============================================================================
// Configuration Builder Tests
// ============================================================================

#[test]
fn test_config_builder_minimal() {
    let config = CliConfig::builder().build().unwrap();

    // Should have defaults
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
}

#[test]
fn test_config_builder_full() {
    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let config = CliConfig::builder()
        .kiln_path(&kiln_path)
        .embedding_url("https://example.com")
        .embedding_model("test-model")
        .chat_model("custom-model")
        .temperature(0.5)
        .max_tokens(1024)
        .streaming(false)
        .system_prompt("Custom prompt")
        .ollama_endpoint("https://ollama.example.com")
        .timeout_secs(60)
        .build()
        .unwrap();

    assert_eq!(config.kiln.path, kiln_path);
    assert_eq!(config.kiln.embedding_url, "https://example.com");
    assert_eq!(config.kiln.embedding_model, Some("test-model".to_string()));
    assert_eq!(config.chat_model(), "custom-model");
    assert_eq!(config.temperature(), 0.5);
    assert_eq!(config.max_tokens(), 1024);
    assert!(!config.streaming());
    assert_eq!(config.system_prompt(), "Custom prompt");
    assert_eq!(config.ollama_endpoint(), "https://ollama.example.com");
    assert_eq!(config.timeout(), 60);
}

// ============================================================================
// Path Derivation Tests
// ============================================================================

#[test]
#[serial]
fn test_database_path_unique_per_process() {
    // Set test mode to enable PID suffix for database name
    std::env::set_var("CRUCIBLE_TEST_MODE", "1");

    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let config1 = CliConfig::builder().kiln_path(&kiln_path).build().unwrap();

    let config2 = CliConfig::builder().kiln_path(&kiln_path).build().unwrap();

    // Database paths should be the same for the same process
    assert_eq!(config1.database_path(), config2.database_path());

    // But should contain the process ID for uniqueness
    let db_path = config1.database_path();
    let filename = db_path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("kiln-"));
    assert!(filename.ends_with(".db"));

    // Cleanup
    std::env::remove_var("CRUCIBLE_TEST_MODE");
}

#[test]
fn test_custom_database_path() {
    let temp = TempDir::new().unwrap();
    let custom_db = temp.path().join("custom.db");

    let config = CliConfig::builder()
        .database_path(&custom_db)
        .build()
        .unwrap();

    assert_eq!(config.database_path(), custom_db);
}

#[test]
fn test_tools_path_derivation() {
    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let config = CliConfig::builder().kiln_path(&kiln_path).build().unwrap();

    let expected = kiln_path.join("tools");
    assert_eq!(config.tools_path(), expected);
}

// ============================================================================
// Configuration Display Tests
// ============================================================================

#[test]
fn test_display_as_toml() {
    let config = CliConfig::builder()
        .kiln_path("/tmp/test")
        .embedding_url("https://example.com")
        .build()
        .unwrap();

    let toml_str = config.display_as_toml().unwrap();
    assert!(toml_str.contains("[kiln]"));
    assert!(toml_str.contains("path"));
    assert!(toml_str.contains("/tmp/test"));
}

#[test]
fn test_display_as_json() {
    let config = CliConfig::builder()
        .kiln_path("/tmp/test")
        .embedding_url("https://example.com")
        .build()
        .unwrap();

    let json_str = config.display_as_json().unwrap();
    assert!(json_str.contains("\"kiln\""));
    assert!(json_str.contains("\"path\""));
    assert!(json_str.contains("/tmp/test"));
}

// ============================================================================
// Embedding Configuration Tests
// ============================================================================

#[test]
fn test_embedding_config_mock_provider() {
    let mut config = CliConfig::default();
    config.kiln.embedding_model = Some("mock".to_string());

    let embedding_config = config.to_embedding_config().unwrap();
    // Mock provider uses "mock-test-model" as the default model name
    assert_eq!(embedding_config.model_name(), "mock-test-model");
}

#[test]
fn test_embedding_config_mock_test_model() {
    let mut config = CliConfig::default();
    config.kiln.embedding_model = Some("mock-test-model".to_string());

    let embedding_config = config.to_embedding_config().unwrap();
    assert_eq!(embedding_config.model_name(), "mock-test-model");
}

#[test]
fn test_embedding_config_missing_model_error() {
    let mut config = CliConfig::default();
    config.kiln.embedding_model = None;
    config.embedding = None;

    let result = config.to_embedding_config();
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Embedding model is not configured"));
}

// ============================================================================
// API Key Resolution Tests
// ============================================================================

#[test]
fn test_openai_api_key_from_config() {
    let mut config = CliConfig::default();
    config.llm.backends.openai.api_key = Some("config-key".to_string());

    assert_eq!(config.openai_api_key(), Some("config-key".to_string()));
}

#[test]
fn test_openai_api_key_from_env() {
    std::env::set_var("OPENAI_API_KEY", "env-key");

    let config = CliConfig::default();
    assert_eq!(config.openai_api_key(), Some("env-key".to_string()));

    std::env::remove_var("OPENAI_API_KEY");
}

#[test]
fn test_openai_api_key_config_precedence() {
    std::env::set_var("OPENAI_API_KEY", "env-key");

    let mut config = CliConfig::default();
    config.llm.backends.openai.api_key = Some("config-key".to_string());

    // Config should take precedence over env
    assert_eq!(config.openai_api_key(), Some("config-key".to_string()));

    std::env::remove_var("OPENAI_API_KEY");
}

#[test]
fn test_anthropic_api_key_from_config() {
    let mut config = CliConfig::default();
    config.llm.backends.anthropic.api_key = Some("config-key".to_string());

    assert_eq!(config.anthropic_api_key(), Some("config-key".to_string()));
}

#[test]
fn test_anthropic_api_key_from_env() {
    std::env::set_var("ANTHROPIC_API_KEY", "env-key");

    let config = CliConfig::default();
    assert_eq!(config.anthropic_api_key(), Some("env-key".to_string()));

    std::env::remove_var("ANTHROPIC_API_KEY");
}

// ============================================================================
// Default Configuration Tests
// ============================================================================

#[test]
fn test_default_config_values() {
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
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
}

#[test]
fn test_file_watcher_defaults() {
    let config = CliConfig::default();

    assert!(config.file_watching.enabled);
    assert_eq!(config.file_watching.debounce_ms, 500);
    assert!(config.file_watching.exclude_patterns.is_empty());
}

#[test]
fn test_network_config_defaults() {
    let config = CliConfig::default();

    assert_eq!(config.network.timeout_secs, Some(30));
    assert_eq!(config.network.pool_size, Some(10));
    assert_eq!(config.network.max_retries, Some(3));
}

#[test]
fn test_migration_config_defaults() {
    let config = CliConfig::default();

    assert!(config.migration.enabled);
    assert_eq!(config.migration.default_security_level, "safe");
    assert!(!config.migration.auto_migrate);
    assert!(config.migration.enable_caching);
    assert_eq!(config.migration.max_cache_size, 500);
    assert!(config.migration.preserve_tool_ids);
    assert!(config.migration.backup_originals);
}

// ============================================================================
// Example Configuration Tests
// ============================================================================

#[test]
fn test_create_example_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("example-config.toml");

    CliConfig::create_example(&config_path).unwrap();

    assert!(config_path.exists());

    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("Crucible CLI Configuration"));
    assert!(contents.contains("[kiln]"));
    assert!(contents.contains("[llm]"));
    assert!(contents.contains("[network]"));
    assert!(contents.contains("embedding_url"));
}

#[test]
fn test_create_example_creates_parent_dirs() {
    let temp = TempDir::new().unwrap();
    let nested_path = temp.path().join("a/b/c/config.toml");

    CliConfig::create_example(&nested_path).unwrap();

    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
}
