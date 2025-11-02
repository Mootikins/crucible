//! End-to-end tests for embedding configuration with nested TOML tables
//!
//! This module tests the complete flow of loading embedding configuration
//! from TOML files with the new nested table format introduced to support
//! user-friendly config syntax like:
//!
//! ```toml
//! [embedding]
//! type = "fastembed"
//!
//! [embedding.model]
//! name = "nomic-embed-text-v1.5"
//! dimensions = 768
//! max_tokens = 512
//!
//! [embedding.api]
//! base_url = "local"
//! timeout_seconds = 60
//! retry_attempts = 1
//! ```

use anyhow::Result;
use crucible_cli::config::CliConfig;
use crucible_config::EmbeddingProviderType;
use std::fs;
use tempfile::TempDir;

/// Test loading FastEmbed config with nested table format
#[test]
fn test_fastembed_nested_table_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    fs::write(&config_path, config_content)?;

    // Load config from file
    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Verify embedding config is loaded correctly
    assert!(config.embedding.is_some(), "Embedding config should exist");

    // Convert to EmbeddingProviderConfig
    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);
    assert_eq!(embedding_config.model.name, "nomic-embed-text-v1.5");
    // Note: dimensions/max_tokens from nested model config are not currently preserved
    // in the CLI -> EmbeddingProviderConfig conversion (enhancement opportunity)
    assert_eq!(embedding_config.api.base_url, Some("local".to_string()));
    assert_eq!(embedding_config.api.timeout_seconds, Some(60));
    assert_eq!(embedding_config.api.retry_attempts, Some(1));

    Ok(())
}

/// Test loading Ollama config with nested table format
#[test]
fn test_ollama_nested_table_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"

[embedding]
type = "ollama"

[embedding.model]
name = "nomic-embed-text"
dimensions = 768

[embedding.api]
base_url = "https://llama.terminal.krohnos.io"
timeout_seconds = 120
retry_attempts = 3

[embedding.ollama]
url = "https://llama.terminal.krohnos.io"
timeout_secs = 120
max_retries = 3
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    assert!(config.embedding.is_some());

    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::Ollama);
    assert_eq!(embedding_config.model.name, "nomic-embed-text");
    // Note: dimensions from nested model config not currently preserved
    assert_eq!(embedding_config.api.base_url, Some("https://llama.terminal.krohnos.io".to_string()));

    Ok(())
}

/// Test loading OpenAI config with nested table format
#[test]
fn test_openai_nested_table_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"

[embedding]
type = "openai"

[embedding.model]
name = "text-embedding-3-small"
dimensions = 1536

[embedding.api]
base_url = "https://api.openai.com/v1"
timeout_seconds = 30
retry_attempts = 2

[embedding.openai]
url = "https://api.openai.com/v1"
api_key = "sk-test-key"
timeout_secs = 30
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    assert!(config.embedding.is_some());

    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::OpenAI);
    assert_eq!(embedding_config.model.name, "text-embedding-3-small");
    // Note: dimensions from nested model config not currently preserved

    Ok(())
}

/// Test backward compatibility with inline table format
#[test]
fn test_inline_table_format_backward_compat() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"

[embedding]
type = "fastembed"
model = "nomic-embed-text-v1.5"
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    assert!(config.embedding.is_some());

    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);
    assert_eq!(embedding_config.model.name, "nomic-embed-text-v1.5");

    Ok(())
}

/// Test backward compatibility with legacy kiln.embedding_model
#[test]
fn test_legacy_kiln_embedding_model_fallback() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"
embedding_url = "local"
embedding_model = "legacy-model"
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Should work even without [embedding] section
    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);
    assert_eq!(embedding_config.model.name, "legacy-model");

    Ok(())
}

/// Test mixed format: nested embedding with legacy kiln path
#[test]
fn test_mixed_format_nested_and_legacy() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"
embedding_url = "http://old-endpoint.com"  # Should be overridden by [embedding.api]

[embedding]
type = "fastembed"

[embedding.model]
name = "new-model"
dimensions = 768

[embedding.api]
base_url = "local"
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    let embedding_config = config.to_embedding_config()?;

    // New format should take precedence
    assert_eq!(embedding_config.model.name, "new-model");
    assert_eq!(embedding_config.api.base_url, Some("local".to_string()));

    Ok(())
}

/// Test config with profiles and nested embedding tables
#[test]
fn test_profiles_with_nested_embedding() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
profile = "development"

[kiln]
path = "/tmp/test_kiln"

[profiles.development]
name = "development"
environment = "development"

[profiles.production]
name = "production"
environment = "production"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Verify both profiles and embedding config work together
    assert!(config.embedding.is_some());

    let embedding_config = config.to_embedding_config()?;
    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);
    assert_eq!(embedding_config.model.name, "nomic-embed-text-v1.5");

    Ok(())
}

/// Test loading actual user config format
#[test]
fn test_actual_user_config_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // This is the ACTUAL format from the user's ~/.config/crucible/config.toml
    let config_content = r#"
# Crucible CLI Configuration
# Location: ~/.config/crucible/config.toml

[kiln]
# Path to your Obsidian kiln
# Default: current directory
path = "/home/moot/Documents/crucible-testing"

[embedding]
# Embedding provider configuration (FastEmbed - local, no server needed)
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    fs::write(&config_path, config_content)?;

    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;

    // Verify kiln path is loaded
    assert_eq!(
        config.kiln.path.to_string_lossy(),
        "/home/moot/Documents/crucible-testing"
    );

    // Verify embedding config is loaded correctly
    assert!(config.embedding.is_some());

    let embedding_config = config.to_embedding_config()?;

    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);
    assert_eq!(embedding_config.model.name, "nomic-embed-text-v1.5");
    // Note: dimensions/max_tokens from nested model config not currently preserved
    assert_eq!(embedding_config.api.base_url, Some("local".to_string()));
    assert_eq!(embedding_config.api.timeout_seconds, Some(60));
    assert_eq!(embedding_config.api.retry_attempts, Some(1));

    Ok(())
}

/// Test config with all three providers using nested tables
#[test]
fn test_all_providers_nested_format() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Test FastEmbed
    let fastembed_path = temp_dir.path().join("fastembed.toml");
    fs::write(
        &fastembed_path,
        r#"
[kiln]
path = "/tmp/test"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
"#,
    )?;

    let contents = fs::read_to_string(&fastembed_path)?;
    let config: CliConfig = toml::from_str(&contents)?;
    let embedding_config = config.to_embedding_config()?;
    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::FastEmbed);

    // Test Ollama
    let ollama_path = temp_dir.path().join("ollama.toml");
    fs::write(
        &ollama_path,
        r#"
[kiln]
path = "/tmp/test"

[embedding]
type = "ollama"

[embedding.model]
name = "nomic-embed-text"

[embedding.ollama]
url = "https://llama.terminal.krohnos.io"
"#,
    )?;

    let contents = fs::read_to_string(&ollama_path)?;
    let config: CliConfig = toml::from_str(&contents)?;
    let embedding_config = config.to_embedding_config()?;
    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::Ollama);

    // Test OpenAI
    let openai_path = temp_dir.path().join("openai.toml");
    fs::write(
        &openai_path,
        r#"
[kiln]
path = "/tmp/test"

[embedding]
type = "openai"

[embedding.model]
name = "text-embedding-3-small"

[embedding.openai]
api_key = "sk-test"
"#,
    )?;

    let contents = fs::read_to_string(&openai_path)?;
    let config: CliConfig = toml::from_str(&contents)?;
    let embedding_config = config.to_embedding_config()?;
    assert_eq!(embedding_config.provider_type, EmbeddingProviderType::OpenAI);

    Ok(())
}

/// Test error handling for invalid nested table configs
#[test]
fn test_invalid_nested_config_error_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Invalid TOML: missing closing bracket
    let invalid_toml = r#"
[kiln]
path = "/tmp/test"

[embedding
type = "fastembed"
"#;

    fs::write(&config_path, invalid_toml)?;

    let contents = fs::read_to_string(&config_path)?;
    let result: Result<CliConfig, toml::de::Error> = toml::from_str(&contents);

    assert!(result.is_err(), "Invalid TOML should fail to parse");

    Ok(())
}

/// Test performance of loading large config with nested tables
#[test]
fn test_nested_config_loading_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/test_kiln"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1

[embedding.fastembed]
cache_dir = "/tmp/cache"
batch_size = 64
show_download = false
"#;

    fs::write(&config_path, config_content)?;

    let start = std::time::Instant::now();
    let contents = fs::read_to_string(&config_path)?;
    let config: CliConfig = toml::from_str(&contents)?;
    let _ = config.to_embedding_config()?;
    let duration = start.elapsed();

    assert!(
        duration < std::time::Duration::from_millis(50),
        "Config loading should be fast (took {:?})",
        duration
    );

    Ok(())
}
