//! Configuration and error handling integration tests

use crucible_cli::config::CliConfig;
use crucible_config::EmbeddingProviderType;
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
kiln_path = "/tmp/test-kiln"

[embedding]
provider = "openai"
model = "test-model"
api_url = "https://example.com"

[acp]
default_agent = "claude-code"
enable_discovery = true

[chat]
model = "gpt-4"
enable_markdown = true

[cli]
show_progress = true
verbose = false
"#,
    )
    .unwrap();

    let config = CliConfig::load(Some(config_path), None, None).unwrap();
    assert_eq!(config.kiln_path.to_str().unwrap(), "/tmp/test-kiln");
    assert_eq!(config.embedding.provider, EmbeddingProviderType::OpenAI);
    assert_eq!(config.embedding.model, Some("test-model".to_string()));
    assert_eq!(
        config.embedding.api_url,
        Some("https://example.com".to_string())
    );
}

#[test]
fn test_config_cli_overrides() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
kiln_path = "/tmp/test-kiln"

[embedding]
provider = "ollama"
model = "file-model"
api_url = "https://file-url.com"
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

    // Verify CLI overrides take priority over file config
    assert_eq!(config.kiln_path.to_str().unwrap(), "/tmp/test-kiln");
    assert_eq!(config.embedding.provider, EmbeddingProviderType::Ollama);
    assert_eq!(config.embedding.model, Some("cli-model".to_string()));
    assert_eq!(
        config.embedding.api_url,
        Some("https://cli-url.com".to_string())
    );
}

// ============================================================================
// Configuration Default Tests
// ============================================================================

#[test]
fn test_config_default_minimal() {
    let config = CliConfig::default();

    // Should have defaults
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
    assert_eq!(config.embedding.provider, EmbeddingProviderType::FastEmbed);
    assert_eq!(config.embedding.batch_size, 16);
}

#[test]
fn test_config_with_custom_kiln_path() {
    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let mut config = CliConfig::default();
    config.kiln_path = kiln_path.clone();

    assert_eq!(config.kiln_path, kiln_path);
    assert_eq!(config.embedding.provider, EmbeddingProviderType::FastEmbed);
    assert_eq!(config.chat_model(), "llama3.2");
    assert_eq!(config.temperature(), 0.7);
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

    let mut config1 = CliConfig::default();
    config1.kiln_path = kiln_path.clone();

    let mut config2 = CliConfig::default();
    config2.kiln_path = kiln_path.clone();

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
#[serial]
fn test_database_path_derivation() {
    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let mut config = CliConfig::default();
    config.kiln_path = kiln_path.clone();

    // Database path should be derived from kiln path (no test mode = standard name)
    let expected_db_path = kiln_path.join(".crucible").join("kiln.db");
    assert_eq!(config.database_path(), expected_db_path);
}

#[test]
fn test_tools_path_derivation() {
    let temp = TempDir::new().unwrap();
    let kiln_path = temp.path().join("kiln");

    let mut config = CliConfig::default();
    config.kiln_path = kiln_path.clone();

    let expected = kiln_path.join("tools");
    assert_eq!(config.tools_path(), expected);
}

// ============================================================================
// Configuration Display Tests
// ============================================================================

#[test]
fn test_display_as_toml() {
    let mut config = CliConfig::default();
    config.kiln_path = "/tmp/test".into();

    let toml_str = config.display_as_toml().unwrap();
    assert!(toml_str.contains("kiln_path"));
    assert!(toml_str.contains("/tmp/test"));
    assert!(toml_str.contains("[embedding]"));
}

#[test]
fn test_display_as_json() {
    let mut config = CliConfig::default();
    config.kiln_path = "/tmp/test".into();

    let json_str = config.display_as_json().unwrap();
    assert!(json_str.contains("\"kiln_path\""));
    assert!(json_str.contains("/tmp/test"));
    assert!(json_str.contains("\"embedding\""));
}

// ============================================================================
// Embedding Configuration Tests
// ============================================================================

#[test]
fn test_embedding_config_defaults() {
    let config = CliConfig::default();

    assert_eq!(config.embedding.provider, EmbeddingProviderType::FastEmbed);
    assert_eq!(config.embedding.model, None); // Uses provider default
    assert_eq!(config.embedding.api_url, None);
    assert_eq!(config.embedding.batch_size, 16);
}

#[test]
fn test_embedding_config_openai() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("embedding.toml");

    fs::write(
        &config_path,
        r#"
kiln_path = "/tmp/test"

[embedding]
provider = "openai"
model = "text-embedding-3-small"
api_url = "https://api.openai.com/v1"
batch_size = 32
"#,
    )
    .unwrap();

    let config = CliConfig::load(Some(config_path), None, None).unwrap();
    assert_eq!(config.embedding.provider, EmbeddingProviderType::OpenAI);
    assert_eq!(
        config.embedding.model,
        Some("text-embedding-3-small".to_string())
    );
    assert_eq!(
        config.embedding.api_url,
        Some("https://api.openai.com/v1".to_string())
    );
    assert_eq!(config.embedding.batch_size, 32);
}

// ============================================================================
// API Key Resolution Tests
// ============================================================================

#[test]
#[serial]
fn test_openai_api_key_from_env() {
    std::env::set_var("OPENAI_API_KEY", "env-key");

    let config = CliConfig::default();
    assert_eq!(config.openai_api_key(), Some("env-key".to_string()));

    std::env::remove_var("OPENAI_API_KEY");
}

#[test]
#[serial]
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
    // Default endpoint should be standard localhost for Ollama
    assert_eq!(config.ollama_endpoint(), "http://localhost:11434");
    assert_eq!(config.timeout(), 30);

    // New embedding defaults
    assert_eq!(config.embedding.provider, EmbeddingProviderType::FastEmbed);
    assert_eq!(config.embedding.batch_size, 16);

    // ACP defaults
    assert_eq!(config.acp.default_agent, None);
    assert!(config.acp.enable_discovery);
    assert_eq!(config.acp.session_timeout_minutes, 30);
    assert_eq!(config.acp.max_message_size_mb, 25);

    // Chat defaults
    assert_eq!(config.chat.model, None);
    assert!(config.chat.enable_markdown);

    // CLI defaults
    assert!(config.cli.show_progress);
    assert!(config.cli.confirm_destructive);
    assert!(!config.cli.verbose);
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
    assert!(contents.contains("kiln_path"));
    assert!(contents.contains("[embedding]"));
    assert!(contents.contains("[acp]"));
    assert!(contents.contains("[chat]"));
    assert!(contents.contains("[cli]"));
}

#[test]
fn test_create_example_creates_parent_dirs() {
    let temp = TempDir::new().unwrap();
    let nested_path = temp.path().join("a/b/c/config.toml");

    CliConfig::create_example(&nested_path).unwrap();

    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
}
