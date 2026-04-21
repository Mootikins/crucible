//! Context configuration for project rules and layered prompts

use serde::{Deserialize, Serialize};

/// Configuration for context loading (project rules, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Files to search for project rules, in priority order.
    ///
    /// Files are searched from git root down to workspace directory.
    /// All matching files are loaded hierarchically (root = lowest priority,
    /// workspace = highest priority).
    ///
    /// Default: ["AGENTS.md", ".rules", ".github/copilot-instructions.md"]
    #[serde(default = "default_rules_files")]
    pub rules_files: Vec<String>,
}

fn default_rules_files() -> Vec<String> {
    vec![
        "AGENTS.md".to_string(),
        ".rules".to_string(),
        ".github/copilot-instructions.md".to_string(),
    ]
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            rules_files: default_rules_files(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules_files() {
        let config = ContextConfig::default();
        assert_eq!(config.rules_files.len(), 3);
        assert_eq!(config.rules_files[0], "AGENTS.md");
        assert_eq!(config.rules_files[1], ".rules");
        assert_eq!(config.rules_files[2], ".github/copilot-instructions.md");
    }

    #[test]
    fn test_deserialize_custom_rules_files() {
        let toml = r#"
            rules_files = ["CUSTOM.md", "OTHER.txt"]
        "#;
        let config: ContextConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.rules_files.len(), 2);
        assert_eq!(config.rules_files[0], "CUSTOM.md");
    }

    #[test]
    fn test_deserialize_empty_rules_files() {
        let toml = r#"
            rules_files = []
        "#;
        let config: ContextConfig = toml::from_str(toml).unwrap();
        assert!(config.rules_files.is_empty());
    }

    #[test]
    fn test_deserialize_with_default() {
        // When rules_files is not specified, it should use the default
        let toml = "";
        let config: ContextConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.rules_files.len(), 3);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = ContextConfig {
            rules_files: vec!["A.md".to_string(), "B.md".to_string()],
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: ContextConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.rules_files, deserialized.rules_files);
    }
}
