//! Runtime configuration overlay with audit stack.
//!
//! This module provides [`RuntimeConfig`], which manages runtime modifications
//! to configuration values. It uses a sparse overlay pattern where only modified
//! values are stored, with automatic fallback to the base configuration.
//!
//! Each config path maintains a modification stack for auditability, allowing
//! users to see where values came from and reset to previous states.

use std::collections::HashMap;

use super::shortcuts::{CompletionSource, ShortcutRegistry, ShortcutTarget};
use super::stack::{ConfigMod, ConfigStack, ModSource};
use super::value::ConfigValue;

/// Errors that can occur when setting config values.
#[derive(Debug, Clone, PartialEq)]
pub enum SetError {
    /// Option was not found in shortcuts or base config.
    NotFound(String),
    /// Attempted boolean operation on non-boolean value.
    NotBoolean(String),
    /// Value type doesn't match expected type.
    TypeMismatch {
        key: String,
        expected: &'static str,
        actual: &'static str,
    },
    /// Invalid value for the option.
    InvalidValue { key: String, reason: String },
}

impl std::fmt::Display for SetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetError::NotFound(key) => write!(f, "Unknown option: {}", key),
            SetError::NotBoolean(key) => write!(f, "Option '{}' is not a boolean", key),
            SetError::TypeMismatch {
                key,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Type mismatch for '{}': expected {}, got {}",
                    key, expected, actual
                )
            }
            SetError::InvalidValue { key, reason } => {
                write!(f, "Invalid value for '{}': {}", key, reason)
            }
        }
    }
}

impl std::error::Error for SetError {}

/// Runtime configuration overlay.
///
/// Manages runtime modifications to configuration values using a sparse overlay
/// pattern. Only modified values are stored; unmodified values fall back to
/// the base configuration (serialized as JSON for path traversal).
///
/// Each modified path maintains a stack of modifications for auditability.
pub struct RuntimeConfig {
    /// Stacks keyed by full config path (only for modified values).
    stacks: HashMap<String, ConfigStack>,
    /// Base config as JSON for path traversal.
    base_json: serde_json::Value,
    /// Shortcut registry for resolving short names.
    shortcuts: ShortcutRegistry,
    /// Virtual options (TUI-only, not in base config).
    virtuals: HashMap<String, ConfigStack>,
}

impl RuntimeConfig {
    /// Create a new runtime config overlay from a base config.
    ///
    /// The base config is serialized to JSON for path-based access.
    pub fn new<T: serde::Serialize>(base: &T) -> Result<Self, serde_json::Error> {
        let base_json = serde_json::to_value(base)?;
        Ok(Self {
            stacks: HashMap::new(),
            base_json,
            shortcuts: ShortcutRegistry::new(),
            virtuals: HashMap::new(),
        })
    }

    /// Create a runtime config with an empty base (for testing).
    pub fn empty() -> Self {
        Self {
            stacks: HashMap::new(),
            base_json: serde_json::Value::Object(serde_json::Map::new()),
            shortcuts: ShortcutRegistry::new(),
            virtuals: HashMap::new(),
        }
    }

    /// Get effective value for a key (shortcut or full path).
    ///
    /// Resolution order:
    /// 1. Virtual options (if key is a virtual shortcut)
    /// 2. Overlay stacks (modified values)
    /// 3. Base config (via JSON path traversal)
    pub fn get(&self, key: &str) -> Option<ConfigValue> {
        let (path, is_virtual) = self.resolve_path(key);

        // Virtual options
        if is_virtual {
            return self.virtuals.get(&path).map(|s| s.value().clone());
        }

        // Check overlay stack
        if let Some(stack) = self.stacks.get(&path) {
            return Some(stack.value().clone());
        }

        // Fall back to base config
        self.get_from_base(&path)
    }

    /// Get effective value or return a default.
    pub fn get_or_default(&self, key: &str, default: ConfigValue) -> ConfigValue {
        self.get(key).unwrap_or(default)
    }

    /// Set a value (pushes to the modification stack).
    pub fn set(&mut self, key: &str, value: ConfigValue, source: ModSource) {
        let (path, is_virtual) = self.resolve_path(key);

        if is_virtual {
            let stack = self
                .virtuals
                .entry(path.clone())
                .or_insert_with(|| ConfigStack::new(ConfigValue::Bool(false), ModSource::Default));
            stack.push(value, source);
        } else {
            let default_base = self
                .get_from_base(&path)
                .unwrap_or(ConfigValue::String(String::new()));

            let stack = self
                .stacks
                .entry(path.clone())
                .or_insert_with(|| ConfigStack::new(default_base, ModSource::Default));
            stack.push(value, source);
        }
    }

    /// Set from string with type coercion.
    ///
    /// The value string is parsed using the current value as a type hint.
    pub fn set_str(&mut self, key: &str, value_str: &str, source: ModSource) {
        let type_hint = self.get(key);
        let value = ConfigValue::parse(value_str, type_hint.as_ref());
        self.set(key, value, source);
    }

    /// Toggle a boolean value.
    ///
    /// Returns the new value, or an error if the option is not boolean.
    pub fn toggle(&mut self, key: &str, source: ModSource) -> Result<bool, SetError> {
        let current = self
            .get(key)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| SetError::NotBoolean(key.to_string()))?;

        let new_value = !current;
        self.set(key, ConfigValue::Bool(new_value), source);
        Ok(new_value)
    }

    /// Enable a boolean value (set to true).
    ///
    /// Returns an error if the option is not boolean.
    pub fn enable(&mut self, key: &str, source: ModSource) -> Result<(), SetError> {
        // Check if it's a boolean
        if let Some(current) = self.get(key) {
            if current.as_bool().is_none() {
                return Err(SetError::NotBoolean(key.to_string()));
            }
        }
        self.set(key, ConfigValue::Bool(true), source);
        Ok(())
    }

    /// Disable a boolean value (set to false).
    ///
    /// Returns an error if the option is not boolean.
    pub fn disable(&mut self, key: &str, source: ModSource) -> Result<(), SetError> {
        // Check if it's a boolean
        if let Some(current) = self.get(key) {
            if current.as_bool().is_none() {
                return Err(SetError::NotBoolean(key.to_string()));
            }
        }
        self.set(key, ConfigValue::Bool(false), source);
        Ok(())
    }

    /// Reset to base config value (clear all modifications).
    pub fn reset(&mut self, key: &str) {
        let (path, is_virtual) = self.resolve_path(key);
        if is_virtual {
            self.virtuals.remove(&path);
        } else {
            self.stacks.remove(&path);
        }
    }

    /// Pop one modification level.
    ///
    /// Returns the popped modification, or None if at base.
    pub fn pop(&mut self, key: &str) -> Option<ConfigMod> {
        let (path, is_virtual) = self.resolve_path(key);
        if is_virtual {
            self.virtuals.get_mut(&path).and_then(|s| s.pop())
        } else {
            self.stacks.get_mut(&path).and_then(|s| s.pop())
        }
    }

    /// Get modification history for a key.
    pub fn history(&self, key: &str) -> Vec<&ConfigMod> {
        let (path, is_virtual) = self.resolve_path(key);
        if is_virtual {
            self.virtuals
                .get(&path)
                .map(|s| s.history().collect())
                .unwrap_or_default()
        } else {
            self.stacks
                .get(&path)
                .map(|s| s.history().collect())
                .unwrap_or_default()
        }
    }

    /// Check if a key has been modified from its base value.
    pub fn is_modified(&self, key: &str) -> bool {
        let (path, is_virtual) = self.resolve_path(key);
        if is_virtual {
            self.virtuals.get(&path).is_some_and(|s| s.is_modified())
        } else {
            self.stacks.get(&path).is_some_and(|s| s.is_modified())
        }
    }

    /// Get all modified keys with their current values.
    pub fn modified(&self) -> Vec<(String, ConfigValue)> {
        let mut result: Vec<_> = self
            .stacks
            .iter()
            .filter(|(_, stack)| stack.is_modified())
            .map(|(path, stack)| (path.clone(), stack.value().clone()))
            .collect();

        result.extend(
            self.virtuals
                .iter()
                .filter(|(_, stack)| stack.is_modified())
                .map(|(path, stack)| (path.clone(), stack.value().clone())),
        );

        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Get the shortcut registry for completion lookups.
    pub fn shortcuts(&self) -> &ShortcutRegistry {
        &self.shortcuts
    }

    /// Get completion source for a key.
    pub fn completions_for(&self, key: &str) -> CompletionSource {
        self.shortcuts.completions_for(key)
    }

    /// Check if a key is a known shortcut.
    pub fn is_shortcut(&self, key: &str) -> bool {
        self.shortcuts.is_shortcut(key)
    }

    /// Get description for a shortcut.
    pub fn description(&self, key: &str) -> Option<&'static str> {
        self.shortcuts.description(key)
    }

    // --- Formatting methods for :set display ---

    /// Format for `:set` (show modified options).
    pub fn format_modified(&self) -> String {
        let modified = self.modified();
        if modified.is_empty() {
            return "No options modified from defaults".to_string();
        }

        modified
            .iter()
            .map(|(key, value)| {
                let display_key = self.shortcuts.reverse_lookup(key).unwrap_or(key.as_str());
                format!("  {}={}", display_key, value)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format for `:set option?` (query value).
    pub fn format_query(&self, key: &str) -> String {
        let display_key = if self.shortcuts.is_shortcut(key) {
            key
        } else {
            self.shortcuts.reverse_lookup(key).unwrap_or(key)
        };

        match self.get(key) {
            Some(value) => format!("  {}={}", display_key, value),
            None => format!("  {} is not set", display_key),
        }
    }

    /// Format for `:set option??` (query with history).
    pub fn format_history(&self, key: &str) -> String {
        let history = self.history(key);
        let display_key = if self.shortcuts.is_shortcut(key) {
            key
        } else {
            self.shortcuts.reverse_lookup(key).unwrap_or(key)
        };

        if history.is_empty() {
            // No stack, try to get base value
            if let Some(value) = self.get(key) {
                return format!("  {} = {} (default)", display_key, value);
            }
            return format!("  {} has no history", display_key);
        }

        let mut lines = vec![format!("  {} history:", display_key)];
        for (i, m) in history.iter().enumerate() {
            let marker = if i == history.len() - 1 { "→" } else { " " };
            lines.push(format!("  {} {}  ({})", marker, m.value, m.source));
        }
        lines.join("\n")
    }

    /// Format for `:set all` (show all options).
    pub fn format_all(&self) -> String {
        let mut lines = Vec::new();

        // First show all shortcuts with their current values
        lines.push("Options:".to_string());
        for shortcut in self.shortcuts.all() {
            let value = self.get(shortcut.short);
            let value_str = value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(not set)".to_string());
            let modified = self.is_modified(shortcut.short);
            let marker = if modified { "*" } else { " " };
            lines.push(format!(
                " {} {}={}\t{}",
                marker, shortcut.short, value_str, shortcut.description
            ));
        }

        // Then show any modified full paths that aren't shortcuts
        let non_shortcut_mods: Vec<_> = self
            .stacks
            .iter()
            .filter(|(path, stack)| {
                stack.is_modified() && self.shortcuts.reverse_lookup(path).is_none()
            })
            .collect();

        if !non_shortcut_mods.is_empty() {
            lines.push(String::new());
            lines.push("Modified paths:".to_string());
            for (path, stack) in non_shortcut_mods {
                lines.push(format!("  {}={}", path, stack.value()));
            }
        }

        lines.join("\n")
    }

    // --- Private helpers ---

    /// Resolve a key to its full path and whether it's virtual.
    fn resolve_path(&self, key: &str) -> (String, bool) {
        // Full paths contain dots - use as-is
        if key.contains('.') {
            return (key.to_string(), false);
        }

        // Check if it's a shortcut
        if let Some(shortcut) = self.shortcuts.get(key) {
            match &shortcut.target {
                ShortcutTarget::Path(path) => ((*path).to_string(), false),
                ShortcutTarget::Dynamic => {
                    // For dynamic shortcuts, we need context to resolve.
                    // For now, use a placeholder. The actual resolution
                    // will be done by the caller who has context.
                    (format!("__dynamic__.{}", key), false)
                }
                ShortcutTarget::Virtual => (key.to_string(), true),
            }
        } else {
            // Not a shortcut, treat as literal path
            (key.to_string(), false)
        }
    }

    /// Get value from base config via JSON path traversal.
    fn get_from_base(&self, path: &str) -> Option<ConfigValue> {
        let mut current = &self.base_json;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        Some(ConfigValue::from(current.clone()))
    }

    /// Resolve dynamic shortcut to actual path.
    ///
    /// This is called by the TUI with the current context to resolve
    /// dynamic shortcuts like "model" which depend on the current provider.
    pub fn resolve_dynamic(&self, key: &str, current_provider: &str) -> String {
        if let Some(shortcut) = self.shortcuts.get(key) {
            if matches!(shortcut.target, ShortcutTarget::Dynamic) {
                // Currently only "model" is dynamic
                if key == "model" {
                    return format!("llm.providers.{}.default_model", current_provider);
                }
            }
        }
        key.to_string()
    }

    /// Set a dynamic shortcut value with provider context.
    pub fn set_dynamic(
        &mut self,
        key: &str,
        value: ConfigValue,
        source: ModSource,
        current_provider: &str,
    ) {
        let path = self.resolve_dynamic(key, current_provider);
        self.set(&path, value, source);
    }

    /// Get a dynamic shortcut value with provider context.
    pub fn get_dynamic(&self, key: &str, current_provider: &str) -> Option<ConfigValue> {
        let path = self.resolve_dynamic(key, current_provider);
        self.get(&path)
    }
}

impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConfig")
            .field("stacks", &self.stacks.keys().collect::<Vec<_>>())
            .field("virtuals", &self.virtuals.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> RuntimeConfig {
        RuntimeConfig::empty()
    }

    #[test]
    fn test_set_and_get() {
        let mut config = make_config();
        config.set(
            "test.key",
            ConfigValue::String("hello".into()),
            ModSource::Command,
        );
        assert_eq!(
            config.get("test.key"),
            Some(ConfigValue::String("hello".into()))
        );
    }

    #[test]
    fn test_set_str_with_type_hint() {
        let mut config = make_config();
        // First set as int
        config.set("count", ConfigValue::Int(10), ModSource::Default);
        // Then set_str should parse as int
        config.set_str("count", "42", ModSource::Command);
        assert_eq!(config.get("count"), Some(ConfigValue::Int(42)));
    }

    #[test]
    fn test_toggle_bool() {
        let mut config = make_config();
        config.set("flag", ConfigValue::Bool(false), ModSource::Default);

        let result = config.toggle("flag", ModSource::Command);
        assert_eq!(result, Ok(true));
        assert_eq!(config.get("flag"), Some(ConfigValue::Bool(true)));

        let result = config.toggle("flag", ModSource::Command);
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn test_toggle_non_bool_fails() {
        let mut config = make_config();
        config.set(
            "name",
            ConfigValue::String("test".into()),
            ModSource::Default,
        );

        let result = config.toggle("name", ModSource::Command);
        assert!(matches!(result, Err(SetError::NotBoolean(_))));
    }

    #[test]
    fn test_enable_disable() {
        let mut config = make_config();
        config.set("flag", ConfigValue::Bool(false), ModSource::Default);

        config.enable("flag", ModSource::Command).unwrap();
        assert_eq!(config.get("flag"), Some(ConfigValue::Bool(true)));

        config.disable("flag", ModSource::Command).unwrap();
        assert_eq!(config.get("flag"), Some(ConfigValue::Bool(false)));
    }

    #[test]
    fn test_reset() {
        let mut config = make_config();
        config.set("key", ConfigValue::Int(1), ModSource::Default);
        config.set("key", ConfigValue::Int(2), ModSource::Command);
        config.set("key", ConfigValue::Int(3), ModSource::Command);

        assert_eq!(config.get("key"), Some(ConfigValue::Int(3)));

        config.reset("key");
        // After reset, stack is removed, so we get None (no base)
        assert_eq!(config.get("key"), None);
    }

    #[test]
    fn test_pop() {
        let mut config = make_config();
        config.set("key", ConfigValue::Int(1), ModSource::Default);
        config.set("key", ConfigValue::Int(2), ModSource::Command);
        config.set("key", ConfigValue::Int(3), ModSource::Command);

        let popped = config.pop("key");
        assert!(popped.is_some());
        assert_eq!(config.get("key"), Some(ConfigValue::Int(2)));

        config.pop("key");
        assert_eq!(config.get("key"), Some(ConfigValue::Int(1)));
    }

    #[test]
    fn test_history() {
        let mut config = make_config();
        config.set("key", ConfigValue::Int(1), ModSource::Default);
        config.set("key", ConfigValue::Int(2), ModSource::Command);

        let history = config.history("key");
        assert_eq!(history.len(), 3);
        assert!(matches!(history[0].value, ConfigValue::String(_)));
        assert_eq!(history[1].value, ConfigValue::Int(1));
        assert_eq!(history[2].value, ConfigValue::Int(2));
    }

    #[test]
    fn test_modified() {
        let mut config = make_config();
        config.set("a", ConfigValue::Int(1), ModSource::Default);
        config.set("b", ConfigValue::Int(2), ModSource::Default);
        config.set("a", ConfigValue::Int(10), ModSource::Command);

        let modified = config.modified();
        assert_eq!(modified.len(), 2); // Both have stacks, both are "modified" from empty
    }

    #[test]
    fn test_virtual_options() {
        let mut config = make_config();

        // "thinking" is a virtual shortcut
        config.set("thinking", ConfigValue::Bool(true), ModSource::Command);
        assert_eq!(config.get("thinking"), Some(ConfigValue::Bool(true)));

        config.toggle("thinking", ModSource::Command).unwrap();
        assert_eq!(config.get("thinking"), Some(ConfigValue::Bool(false)));
    }

    #[test]
    fn test_shortcut_resolution() {
        let config = make_config();

        // "verbose" should resolve to "cli.verbose"
        let (path, is_virtual) = config.resolve_path("verbose");
        assert_eq!(path, "cli.verbose");
        assert!(!is_virtual);

        // "thinking" is virtual
        let (path, is_virtual) = config.resolve_path("thinking");
        assert_eq!(path, "thinking");
        assert!(is_virtual);

        // Full path stays as-is
        let (path, is_virtual) = config.resolve_path("llm.providers.local.temperature");
        assert_eq!(path, "llm.providers.local.temperature");
        assert!(!is_virtual);
    }

    #[test]
    fn test_format_query() {
        let mut config = make_config();
        config.set("verbose", ConfigValue::Bool(true), ModSource::Command);

        let output = config.format_query("verbose");
        assert!(output.contains("verbose=true"));
    }

    #[test]
    fn test_format_modified_empty() {
        let config = make_config();
        let output = config.format_modified();
        assert!(output.contains("No options modified"));
    }

    #[test]
    fn test_dynamic_resolution() {
        let mut config = make_config();

        // Set model for "local" provider
        config.set_dynamic(
            "model",
            ConfigValue::String("llama3.2".into()),
            ModSource::Command,
            "local",
        );

        // Get should resolve to the right path
        let value = config.get_dynamic("model", "local");
        assert_eq!(value, Some(ConfigValue::String("llama3.2".into())));

        // Different provider should not have the value
        let value = config.get_dynamic("model", "cloud");
        assert_eq!(value, None);
    }
}
