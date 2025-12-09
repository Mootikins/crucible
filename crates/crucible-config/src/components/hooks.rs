//! Hooks configuration for built-in event handlers

use serde::{Deserialize, Serialize};

/// Configuration for event system hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Built-in hooks configuration
    #[serde(default)]
    pub builtin: BuiltinHooksTomlConfig,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            builtin: BuiltinHooksTomlConfig::default(),
        }
    }
}

/// Configuration for built-in hooks (TOML-friendly version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinHooksTomlConfig {
    /// Test filter hook configuration
    #[serde(default)]
    pub test_filter: HookConfig,

    /// TOON transform hook configuration
    #[serde(default)]
    pub toon_transform: HookConfig,

    /// Recipe enrichment hook configuration
    #[serde(default)]
    pub recipe_enrichment: HookConfig,

    /// Tool selector hook configuration
    #[serde(default)]
    pub tool_selector: ToolSelectorHookConfig,
}

impl Default for BuiltinHooksTomlConfig {
    fn default() -> Self {
        Self {
            test_filter: HookConfig {
                enabled: true,
                pattern: Some("just_test*".to_string()),
                priority: Some(10),
            },
            toon_transform: HookConfig {
                enabled: false,
                pattern: Some("*".to_string()),
                priority: Some(50),
            },
            recipe_enrichment: HookConfig {
                enabled: true,
                pattern: Some("just_*".to_string()),
                priority: Some(5),
            },
            tool_selector: ToolSelectorHookConfig::default(),
        }
    }
}

/// Configuration for a simple hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Whether the hook is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Pattern for matching tool names (glob-style)
    #[serde(default)]
    pub pattern: Option<String>,

    /// Priority for handler execution (lower = earlier)
    #[serde(default)]
    pub priority: Option<i64>,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: Some("*".to_string()),
            priority: Some(100),
        }
    }
}

/// Configuration for the tool selector hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSelectorHookConfig {
    /// Whether the hook is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Pattern for matching tool names (glob-style)
    #[serde(default)]
    pub pattern: Option<String>,

    /// Priority for handler execution (lower = earlier)
    #[serde(default)]
    pub priority: Option<i64>,

    /// Whitelist of allowed tool patterns
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Blacklist of blocked tool patterns
    #[serde(default)]
    pub blocked_tools: Option<Vec<String>>,

    /// Prefix to add to tool names
    #[serde(default)]
    pub prefix: Option<String>,

    /// Suffix to add to tool names
    #[serde(default)]
    pub suffix: Option<String>,
}

impl Default for ToolSelectorHookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: Some("*".to_string()),
            priority: Some(5),
            allowed_tools: None,
            blocked_tools: None,
            prefix: None,
            suffix: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooks_config_default() {
        let config = HooksConfig::default();

        // Test filter should be enabled by default
        assert!(config.builtin.test_filter.enabled);
        assert_eq!(
            config.builtin.test_filter.pattern,
            Some("just_test*".to_string())
        );
        assert_eq!(config.builtin.test_filter.priority, Some(10));

        // TOON transform should be disabled by default
        assert!(!config.builtin.toon_transform.enabled);

        // Recipe enrichment should be enabled by default
        assert!(config.builtin.recipe_enrichment.enabled);
        assert_eq!(
            config.builtin.recipe_enrichment.pattern,
            Some("just_*".to_string())
        );

        // Tool selector should be disabled by default
        assert!(!config.builtin.tool_selector.enabled);
    }

    #[test]
    fn test_hooks_config_parse_toml() {
        let toml_content = r#"
[builtin.test_filter]
enabled = true
pattern = "just_test*"
priority = 10

[builtin.toon_transform]
enabled = true
pattern = "*"
priority = 50

[builtin.recipe_enrichment]
enabled = false
pattern = "just_*"

[builtin.tool_selector]
enabled = true
pattern = "upstream:*"
priority = 5
allowed_tools = ["search_*", "get_*"]
blocked_tools = ["delete_*"]
prefix = "ext_"
"#;

        let config: HooksConfig = toml::from_str(toml_content).unwrap();

        // Check test_filter
        assert!(config.builtin.test_filter.enabled);
        assert_eq!(
            config.builtin.test_filter.pattern,
            Some("just_test*".to_string())
        );
        assert_eq!(config.builtin.test_filter.priority, Some(10));

        // Check toon_transform
        assert!(config.builtin.toon_transform.enabled);
        assert_eq!(config.builtin.toon_transform.pattern, Some("*".to_string()));

        // Check recipe_enrichment
        assert!(!config.builtin.recipe_enrichment.enabled);

        // Check tool_selector
        assert!(config.builtin.tool_selector.enabled);
        assert_eq!(
            config.builtin.tool_selector.pattern,
            Some("upstream:*".to_string())
        );
        assert_eq!(config.builtin.tool_selector.priority, Some(5));
        assert_eq!(
            config.builtin.tool_selector.allowed_tools,
            Some(vec!["search_*".to_string(), "get_*".to_string()])
        );
        assert_eq!(
            config.builtin.tool_selector.blocked_tools,
            Some(vec!["delete_*".to_string()])
        );
        assert_eq!(config.builtin.tool_selector.prefix, Some("ext_".to_string()));
    }

    #[test]
    fn test_hook_config_minimal() {
        let toml_content = r#"
enabled = true
"#;

        let config: HookConfig = toml::from_str(toml_content).unwrap();
        assert!(config.enabled);
        assert!(config.pattern.is_none());
        assert!(config.priority.is_none());
    }

    #[test]
    fn test_tool_selector_config_minimal() {
        let toml_content = r#"
enabled = true
"#;

        let config: ToolSelectorHookConfig = toml::from_str(toml_content).unwrap();
        assert!(config.enabled);
        assert!(config.allowed_tools.is_none());
        assert!(config.blocked_tools.is_none());
        assert!(config.prefix.is_none());
        assert!(config.suffix.is_none());
    }

    #[test]
    fn test_hooks_config_serialization() {
        let config = HooksConfig {
            builtin: BuiltinHooksTomlConfig {
                test_filter: HookConfig {
                    enabled: true,
                    pattern: Some("test*".to_string()),
                    priority: Some(10),
                },
                toon_transform: HookConfig::default(),
                recipe_enrichment: HookConfig::default(),
                tool_selector: ToolSelectorHookConfig::default(),
            },
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: HooksConfig = toml::from_str(&toml_str).unwrap();

        assert!(parsed.builtin.test_filter.enabled);
        assert_eq!(
            parsed.builtin.test_filter.pattern,
            Some("test*".to_string())
        );
    }

    #[test]
    fn test_tool_selector_with_suffix() {
        let toml_content = r#"
enabled = true
suffix = "_tool"
"#;

        let config: ToolSelectorHookConfig = toml::from_str(toml_content).unwrap();
        assert!(config.enabled);
        assert_eq!(config.suffix, Some("_tool".to_string()));
        assert!(config.prefix.is_none());
    }

    #[test]
    fn test_builtin_hooks_partial_config() {
        let toml_content = r#"
[builtin.test_filter]
enabled = false

[builtin.tool_selector]
enabled = true
allowed_tools = ["safe_*"]
"#;

        let config: HooksConfig = toml::from_str(toml_content).unwrap();

        assert!(!config.builtin.test_filter.enabled);
        assert!(config.builtin.tool_selector.enabled);
        assert_eq!(
            config.builtin.tool_selector.allowed_tools,
            Some(vec!["safe_*".to_string()])
        );

        // Other hooks should use defaults
        assert!(!config.builtin.toon_transform.enabled);
    }
}
