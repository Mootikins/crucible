//! CLI configuration for terminal display settings.

use serde::{Deserialize, Serialize};

/// CLI configuration for terminal display and behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Show progress bars for long operations.
    #[serde(default = "default_true")]
    pub show_progress: bool,
    /// Confirm destructive operations.
    #[serde(default = "default_true")]
    pub confirm_destructive: bool,
    /// Verbose logging.
    #[serde(default)]
    pub verbose: bool,
    /// Syntax highlighting configuration.
    #[serde(default)]
    pub highlighting: HighlightingConfig,
}

/// Configuration for syntax highlighting in code blocks and diffs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightingConfig {
    /// Enable syntax highlighting (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Theme name for syntax highlighting (default: "base16-ocean.dark").
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "base16-ocean.dark".to_string()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            show_progress: true,
            confirm_destructive: true,
            verbose: false,
            highlighting: HighlightingConfig::default(),
        }
    }
}

impl Default for HighlightingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            theme: default_theme(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_enabled_by_default() {
        let config = CliConfig::default();
        assert!(config.highlighting.enabled);
    }

    #[test]
    fn highlighting_theme_has_default() {
        let config = CliConfig::default();
        assert_eq!(config.highlighting.theme, "base16-ocean.dark");
    }

    #[test]
    fn highlighting_config_deserializes_from_toml() {
        let toml = r#"
            [highlighting]
            enabled = false
            theme = "Solarized (dark)"
        "#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        assert!(!config.highlighting.enabled);
        assert_eq!(config.highlighting.theme, "Solarized (dark)");
    }

    #[test]
    fn highlighting_config_uses_defaults_when_missing() {
        let toml = r#"
            show_progress = true
        "#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        assert!(config.highlighting.enabled);
        assert_eq!(config.highlighting.theme, "base16-ocean.dark");
    }
}
