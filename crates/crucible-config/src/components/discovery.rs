//! Discovery path configuration for Rune resources

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for discovery paths
///
/// Supports both flat format:
/// ```toml
/// [discovery.hooks]
/// additional_paths = ["~/.config/crucible/hooks"]
/// use_defaults = true
///
/// [discovery.tools]
/// additional_paths = ["/opt/crucible/tools"]
/// use_defaults = true
/// ```
///
/// And nested format (for backward compatibility):
/// ```toml
/// [discovery.type_configs.tools]
/// additional_paths = ["/custom/tools"]
/// use_defaults = true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryPathsConfig {
    /// Direct hooks configuration (flat format: [discovery.hooks])
    #[serde(default)]
    pub hooks: Option<TypeDiscoveryConfig>,

    /// Direct tools configuration (flat format: [discovery.tools])
    #[serde(default)]
    pub tools: Option<TypeDiscoveryConfig>,

    /// Direct events configuration (flat format: [discovery.events])
    #[serde(default)]
    pub events: Option<TypeDiscoveryConfig>,

    /// Per-type discovery configurations (nested format: [discovery.type_configs.*])
    /// This is for backward compatibility and other types
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

impl DiscoveryPathsConfig {
    /// Get configuration for a specific type, checking both flat and nested formats
    ///
    /// Priority:
    /// 1. Flat format (e.g., `config.hooks`, `config.tools`)
    /// 2. Nested format (e.g., `config.type_configs.get("hooks")`)
    pub fn get_type_config(&self, type_name: &str) -> Option<&TypeDiscoveryConfig> {
        // Check flat format first
        match type_name {
            "hooks" => self.hooks.as_ref(),
            "tools" => self.tools.as_ref(),
            "events" => self.events.as_ref(),
            _ => None,
        }
        .or_else(|| {
            // Fall back to nested format
            self.type_configs.get(type_name)
        })
    }

    /// Get configuration for a specific type, with default if not found
    pub fn get_type_config_or_default(&self, type_name: &str) -> TypeDiscoveryConfig {
        self.get_type_config(type_name).cloned().unwrap_or_default()
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

        let config = DiscoveryPathsConfig {
            hooks: None,
            tools: None,
            events: None,
            type_configs,
        };

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

    #[test]
    fn test_discovery_config_flat_format() {
        // When deserializing DiscoveryPathsConfig, we're already inside [discovery]
        // so [discovery.hooks] becomes [hooks]
        let toml_content = r#"
[hooks]
additional_paths = ["~/.config/crucible/hooks", "/opt/crucible/hooks"]
use_defaults = true

[tools]
additional_paths = ["/opt/crucible/tools"]
use_defaults = false

[events]
use_defaults = true
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();

        // Check hooks config
        assert!(config.hooks.is_some());
        let hooks = config.hooks.as_ref().unwrap();
        assert_eq!(hooks.additional_paths.len(), 2);
        assert_eq!(
            hooks.additional_paths[0],
            PathBuf::from("~/.config/crucible/hooks")
        );
        assert_eq!(
            hooks.additional_paths[1],
            PathBuf::from("/opt/crucible/hooks")
        );
        assert!(hooks.use_defaults);

        // Check tools config
        assert!(config.tools.is_some());
        let tools = config.tools.as_ref().unwrap();
        assert_eq!(tools.additional_paths.len(), 1);
        assert_eq!(
            tools.additional_paths[0],
            PathBuf::from("/opt/crucible/tools")
        );
        assert!(!tools.use_defaults);

        // Check events config
        assert!(config.events.is_some());
        let events = config.events.as_ref().unwrap();
        assert!(events.additional_paths.is_empty());
        assert!(events.use_defaults);
    }

    #[test]
    fn test_get_type_config_flat_format() {
        let toml_content = r#"
[hooks]
additional_paths = ["/custom/hooks"]
use_defaults = false

[tools]
additional_paths = ["/custom/tools"]
use_defaults = true
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();

        // Test get_type_config for flat format
        let hooks = config.get_type_config("hooks").unwrap();
        assert_eq!(hooks.additional_paths.len(), 1);
        assert!(!hooks.use_defaults);

        let tools = config.get_type_config("tools").unwrap();
        assert_eq!(tools.additional_paths.len(), 1);
        assert!(tools.use_defaults);

        // Test get_type_config_or_default
        let events = config.get_type_config_or_default("events");
        assert!(events.additional_paths.is_empty());
        assert!(events.use_defaults);
    }

    #[test]
    fn test_get_type_config_nested_format() {
        let toml_content = r#"
[type_configs.custom_type]
additional_paths = ["/custom/path"]
use_defaults = false
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();

        let custom = config.get_type_config("custom_type").unwrap();
        assert_eq!(custom.additional_paths.len(), 1);
        assert!(!custom.use_defaults);
    }

    #[test]
    fn test_get_type_config_priority_flat_over_nested() {
        let toml_content = r#"
[hooks]
additional_paths = ["/flat/hooks"]
use_defaults = true

[type_configs.hooks]
additional_paths = ["/nested/hooks"]
use_defaults = false
"#;

        let config: DiscoveryPathsConfig = toml::from_str(toml_content).unwrap();

        // Flat format should take priority
        let hooks = config.get_type_config("hooks").unwrap();
        assert_eq!(hooks.additional_paths[0], PathBuf::from("/flat/hooks"));
        assert!(hooks.use_defaults);
    }
}
