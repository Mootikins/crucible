//! Plugin registration types for the Rune plugin system
//!
//! These types represent the structure returned by plugin `init()` functions
//! and the registered hooks parsed from that data.

use glob::Pattern;
use rune::Unit;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for a single hook as parsed from plugin init()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Event type to listen for (default: "tool_result")
    #[serde(default = "default_event_type")]
    pub event: String,
    /// Glob pattern to match tool names
    pub pattern: String,
    /// Name of the handler function to call
    #[serde(default = "default_handler_name")]
    pub handler: String,
}

fn default_event_type() -> String {
    "tool_result".to_string()
}

fn default_handler_name() -> String {
    "handle".to_string()
}

/// Parsed plugin manifest from init() return value
#[derive(Debug, Clone, Default)]
pub struct PluginManifest {
    /// Registered hooks
    pub hooks: Vec<HookConfig>,
    // Future: tools, etc.
}

impl PluginManifest {
    /// Parse a manifest from JSON (the return value of init())
    pub fn from_json(value: &JsonValue) -> Result<Self, String> {
        let hooks = if let Some(hooks_arr) = value.get("hooks").and_then(|v| v.as_array()) {
            hooks_arr
                .iter()
                .map(|h| serde_json::from_value(h.clone()))
                .collect::<Result<Vec<HookConfig>, _>>()
                .map_err(|e| format!("Failed to parse hook config: {}", e))?
        } else {
            vec![]
        };

        Ok(Self { hooks })
    }
}

impl HookConfig {
    /// Convert to a RegisteredHook with compiled unit
    pub fn to_registered_hook(
        &self,
        plugin_path: PathBuf,
        unit: Option<Arc<Unit>>,
    ) -> Result<RegisteredHook, String> {
        let pattern = Pattern::new(&self.pattern)
            .map_err(|e| format!("Invalid glob pattern '{}': {}", self.pattern, e))?;

        Ok(RegisteredHook {
            event_type: self.event.clone(),
            pattern,
            handler_name: self.handler.clone(),
            plugin_path,
            unit,
        })
    }
}

/// A registered hook from a loaded plugin
#[derive(Debug, Clone)]
pub struct RegisteredHook {
    /// Event type this hook listens for
    pub event_type: String,
    /// Glob pattern to match against tool/event names
    pub pattern: Pattern,
    /// Name of the handler function in the Rune script
    pub handler_name: String,
    /// Path to the plugin file
    pub plugin_path: PathBuf,
    /// Compiled Rune unit (None in tests)
    pub unit: Option<Arc<Unit>>,
}

impl RegisteredHook {
    /// Check if this hook matches the given event type and name
    pub fn matches(&self, event_type: &str, name: &str) -> bool {
        self.event_type == event_type && self.pattern.matches(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_hook_pattern_matches_exact() {
        let hook = RegisteredHook {
            event_type: "tool_result".to_string(),
            pattern: Pattern::new("just_test").unwrap(),
            handler_name: "filter".to_string(),
            plugin_path: PathBuf::from("/test.rn"),
            unit: None, // For testing without actual compilation
        };

        assert!(hook.matches("tool_result", "just_test"));
        assert!(!hook.matches("tool_result", "just_build"));
    }

    #[test]
    fn test_registered_hook_pattern_matches_glob() {
        let hook = RegisteredHook {
            event_type: "tool_result".to_string(),
            pattern: Pattern::new("just_test*").unwrap(),
            handler_name: "filter".to_string(),
            plugin_path: PathBuf::from("/test.rn"),
            unit: None,
        };

        assert!(hook.matches("tool_result", "just_test"));
        assert!(hook.matches("tool_result", "just_test_verbose"));
        assert!(hook.matches("tool_result", "just_test_crate"));
        assert!(!hook.matches("tool_result", "just_build"));
    }

    #[test]
    fn test_registered_hook_pattern_no_match_wrong_event() {
        let hook = RegisteredHook {
            event_type: "tool_result".to_string(),
            pattern: Pattern::new("*").unwrap(),
            handler_name: "filter".to_string(),
            plugin_path: PathBuf::from("/test.rn"),
            unit: None,
        };

        assert!(hook.matches("tool_result", "anything"));
        assert!(!hook.matches("note_changed", "anything"));
    }

    #[test]
    fn test_plugin_manifest_parse_from_json() {
        let json = serde_json::json!({
            "hooks": [
                {
                    "event": "tool_result",
                    "pattern": "just_test*",
                    "handler": "filter_output"
                }
            ]
        });

        let manifest = PluginManifest::from_json(&json).unwrap();
        assert_eq!(manifest.hooks.len(), 1);
        assert_eq!(manifest.hooks[0].event, "tool_result");
        assert_eq!(manifest.hooks[0].pattern, "just_test*");
        assert_eq!(manifest.hooks[0].handler, "filter_output");
    }

    #[test]
    fn test_plugin_manifest_empty_hooks_ok() {
        let json = serde_json::json!({
            "hooks": []
        });

        let manifest = PluginManifest::from_json(&json).unwrap();
        assert!(manifest.hooks.is_empty());
    }

    #[test]
    fn test_plugin_manifest_missing_hooks_ok() {
        let json = serde_json::json!({});

        let manifest = PluginManifest::from_json(&json).unwrap();
        assert!(manifest.hooks.is_empty());
    }

    #[test]
    fn test_plugin_manifest_missing_handler_uses_default() {
        let json = serde_json::json!({
            "hooks": [
                {
                    "event": "tool_result",
                    "pattern": "*"
                }
            ]
        });

        let manifest = PluginManifest::from_json(&json).unwrap();
        assert_eq!(manifest.hooks[0].handler, "handle"); // default
    }

    #[test]
    fn test_plugin_manifest_missing_event_uses_default() {
        let json = serde_json::json!({
            "hooks": [
                {
                    "pattern": "*",
                    "handler": "my_handler"
                }
            ]
        });

        let manifest = PluginManifest::from_json(&json).unwrap();
        assert_eq!(manifest.hooks[0].event, "tool_result"); // default
    }

    #[test]
    fn test_hook_config_to_registered_hook() {
        let config = HookConfig {
            event: "tool_result".to_string(),
            pattern: "just_*".to_string(),
            handler: "filter".to_string(),
        };

        let hook = config
            .to_registered_hook(PathBuf::from("/plugin.rn"), None)
            .unwrap();
        assert!(hook.matches("tool_result", "just_test"));
        assert!(hook.matches("tool_result", "just_build"));
    }
}
