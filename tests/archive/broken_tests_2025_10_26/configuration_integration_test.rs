//! Configuration integration tests
//!
//! Tests the CLI configuration loading and validation functionality.

#[tokio::test]
async fn test_configuration_integration() -> Result<()> {
    // Create custom configuration
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("integration_config.toml");

    let config_content = r#"
[kiln]
path = "/tmp/integration_test_kiln"
embedding_url = "https://integration-test-embedding.com"
embedding_model = "integration-test-model"

[llm]
chat_model = "integration-test-chat-model"
temperature = 0.6
max_tokens = 1536

[daemon]
enabled = true
auto_start = true
watch_mode = true
"#;

    fs::write(&config_path, config_content)?;

    // Load configuration from file
    let config = CliConfig::from_file_or_default(&config_path)?;

    // Verify configuration values are properly applied
    assert_eq!(
        config.kiln.embedding_model,
        Some("integration-test-model".to_string())
    );
    assert_eq!(config.chat_model(), "integration-test-chat-model");
    assert_eq!(config.daemon.enabled, true);
    assert!(config.daemon.auto_start);
    assert!(config.daemon.watch_mode);

    Ok(())
}
use anyhow::Result;
use crucible_cli::config::CliConfig;
use std::fs;
use tempfile::TempDir;
