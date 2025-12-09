//! Discovery path configuration for Rune resources

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for discovery paths
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryPathsConfig {
    /// Per-type discovery configurations (e.g., "tools", "hooks", "events")
    #[serde(default)]
    pub type_configs: HashMap<String, TypeDiscoveryConfig>,
}

/// Configuration for a specific resource type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDiscoveryConfig {
    /// Additional paths to search (beyond defaults)
    #[serde(default)]
    pub additional_paths: Vec<PathBuf>,

    /// Whether to include default paths (default: true)
    #[serde(default = "default_true")]
    pub use_defaults: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TypeDiscoveryConfig {
    fn default() -> Self {
        Self {
            additional_paths: Vec::new(),
            use_defaults: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryPathsConfig::default();
        assert!(config.type_configs.is_empty());
    }

    #[test]
    fn test_type_discovery_config_default() {
        let config = TypeDiscoveryConfig::default();
        assert!(config.additional_paths.is_empty());
        assert!(config.use_defaults);
    }

    #[test]
    fn test_discovery_config_parse_toml() {
        let toml_content = r#"
[type_configs.tools]
additional_paths = ["/custom/tools", "/shared/tools"]
use_defaults = true

[type_configs.hooks]
additional_paths = ["/custom/hooks"]
use_defaults = false

[type_configs.events]
use_defaults = true
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.type_configs.len(), 3);

        // Check tools config
        let tools = config.type_configs.get("tools").unwrap();
        assert_eq!(tools.additional_paths.len(), 2);
        assert_eq!(tools.additional_paths[0], PathBuf::from("/custom/tools"));
        assert_eq!(tools.additional_paths[1], PathBuf::from("/shared/tools"));
        assert!(tools.use_defaults);

        // Check hooks config
        let hooks = config.type_configs.get("hooks").unwrap();
        assert_eq!(hooks.additional_paths.len(), 1);
        assert_eq!(hooks.additional_paths[0], PathBuf::from("/custom/hooks"));
        assert!(!hooks.use_defaults);

        // Check events config (no additional paths)
        let events = config.type_configs.get("events").unwrap();
        assert!(events.additional_paths.is_empty());
        assert!(events.use_defaults);
    }

    #[test]
    fn test_discovery_config_empty_type_configs() {
        let toml_content = r#"
type_configs = {}
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();
        assert!(config.type_configs.is_empty());
    }

    #[test]
    fn test_type_discovery_config_use_defaults_default() {
        let toml_content = r#"
additional_paths = ["/custom/path"]
"#;

        let config: TypeDiscoveryConfig = toml::from_str(toml_content).unwrap();
        assert!(config.use_defaults); // Should default to true
        assert_eq!(config.additional_paths.len(), 1);
    }

    #[test]
    fn test_discovery_config_serialization() {
        let mut type_configs = HashMap::new();
        type_configs.insert(
            "tools".to_string(),
            TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/test/path")],
                use_defaults: true,
            },
        );

        let config = DiscoveryPathsConfig { type_configs };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: DiscoveryPathsConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.type_configs.len(), 1);
        let tools = parsed.type_configs.get("tools").unwrap();
        assert_eq!(tools.additional_paths[0], PathBuf::from("/test/path"));
    }

    #[test]
    fn test_discovery_config_with_relative_paths() {
        let toml_content = r#"
[type_configs.tools]
additional_paths = ["./local/tools", "../shared/tools"]
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();
        let tools = config.type_configs.get("tools").unwrap();

        assert_eq!(tools.additional_paths.len(), 2);
        assert_eq!(tools.additional_paths[0], PathBuf::from("./local/tools"));
        assert_eq!(tools.additional_paths[1], PathBuf::from("../shared/tools"));
    }
}
