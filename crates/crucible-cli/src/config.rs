//! Configuration management for Crucible CLI
//!
//! This module re-exports the canonical configuration types from crucible-config
//! and provides CLI-specific utilities and backward compatibility.

// Re-export the canonical configuration types from crucible-config
// Note: We re-export CliAppConfig as CliConfig for CLI backward compatibility
// - CliAppConfig is the top-level composite config (kiln_path, embedding, acp, chat, cli, etc.)
// - crucible_config::CliConfig is the small CLI-specific settings (show_progress, verbose, etc.)
pub use crucible_config::{
    AcpConfig,
    ChatConfig,
    CliAppConfig as CliConfig, // Top-level config type for CLI
    CliConfig as CliAppConfig, // Small CLI settings (renamed for clarity)
    EmbeddingConfig,
    EmbeddingProviderConfig,
    EmbeddingProviderType as ProviderType,
};

// Legacy type aliases for backward compatibility
pub type EmbeddingConfigSection = crucible_config::EmbeddingConfig;
pub type LlmConfig = crucible_config::AcpConfig;

/// Builder for programmatically constructing CliConfig (top-level CLI configuration)
pub struct CliConfigBuilder {
    kiln_path: Option<std::path::PathBuf>,
}

impl CliConfigBuilder {
    /// Create a new builder with defaults
    pub fn new() -> Self {
        Self { kiln_path: None }
    }

    /// Set kiln path
    pub fn kiln_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.kiln_path = Some(path.into());
        self
    }

    /// Build the CliConfig (returns the top-level CLI configuration)
    pub fn build(self) -> anyhow::Result<CliConfig> {
        // Create default config and override kiln_path if provided
        // Note: CliConfig here is crucible_config::CliAppConfig via the re-export alias
        let mut config = CliConfig::default();
        if let Some(path) = self.kiln_path {
            config.kiln_path = path;
        }
        Ok(config)
    }
}

impl Default for CliConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::PathBuf;

    use std::fs;
    use tempfile::TempDir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

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
    #[serial]
    fn test_config_load_with_valid_toml() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("valid.toml");
        let kiln_path = test_path("test-kiln");

        fs::write(
            &config_path,
            format!(
                r#"
kiln_path = "{}"

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
                kiln_path.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();

        let config = CliConfig::load(Some(config_path), None, None).unwrap();
        assert_eq!(config.kiln_path, kiln_path);
        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::OpenAI
        );
        assert_eq!(config.embedding.model, Some("test-model".to_string()));
        assert_eq!(
            config.embedding.api_url,
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_database_path_derivation() {
        let temp = TempDir::new().unwrap();
        let _kiln_path = temp.path().join("kiln");

        let config = CliConfig::default();
        // Note: We can't set kiln_path via builder in this simplified version,
        // so we test the default behavior

        // Database path should be derived from kiln path (no test mode = standard name)
        let expected_db_path = config.kiln_path.join(".crucible").join("kiln.db");
        assert_eq!(config.database_path(), expected_db_path);
    }

    #[test]
    fn test_tools_path_derivation() {
        let config = CliConfig::default();
        let expected_tools = config.kiln_path.join("tools");
        assert_eq!(config.tools_path(), expected_tools);
    }

    #[test]
    fn test_display_as_toml() {
        let config = CliConfig::default();
        let toml_str = config.display_as_toml().unwrap();
        assert!(toml_str.contains("kiln_path"));
        assert!(toml_str.contains("[embedding]"));
    }

    #[test]
    fn test_display_as_json() {
        let config = CliConfig::default();
        let json_str = config.display_as_json().unwrap();
        assert!(json_str.contains("\"kiln_path\""));
        assert!(json_str.contains("\"embedding\""));
    }

    #[test]
    fn test_embedding_config_defaults() {
        let config = CliConfig::default();

        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::FastEmbed
        );
        assert_eq!(config.embedding.batch_size, 16);
    }

    #[test]
    fn test_default_config_values() {
        let config = CliConfig::default();

        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.max_tokens(), 2048);
        assert!(config.streaming());

        // New embedding defaults
        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::FastEmbed
        );
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

    // Additional comprehensive tests for CLI config reading

    #[test]
    fn test_config_load_preserves_defaults_when_missing() {
        let config = CliConfig::default();

        // Verify all important defaults
        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.max_tokens(), 2048);
        assert!(config.streaming());
        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::FastEmbed
        );
        assert_eq!(config.embedding.batch_size, 16);
    }

    #[test]
    #[serial]
    fn test_partial_config_loads_with_defaults() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("partial.toml");

        // Only specify some fields
        fs::write(
            &config_path,
            r#"
kiln_path = "/partial/kiln"
[embedding]
provider = "openai"
"#,
        )
        .unwrap();

        let config = CliConfig::load(Some(config_path), None, None).unwrap();

        // Specified fields
        assert_eq!(config.kiln_path.to_str().unwrap(), "/partial/kiln");
        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::OpenAI
        );

        // Default fields should still be present
        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(config.temperature(), 0.7);
        assert_eq!(config.embedding.batch_size, 16);
    }

    #[test]
    fn test_config_file_not_found_uses_defaults() {
        let temp = TempDir::new().unwrap();
        let nonexistent = temp.path().join("nonexistent.toml");

        // Should not error when file doesn't exist
        let config = CliConfig::load(Some(nonexistent), None, None).unwrap();

        // Should have all defaults
        assert_eq!(config.chat_model(), "llama3.2");
        assert_eq!(
            config.embedding.provider,
            crucible_config::EmbeddingProviderType::FastEmbed
        );
    }
}
