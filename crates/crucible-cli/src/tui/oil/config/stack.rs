//! Configuration audit stack for tracking modifications.
//!
//! Each config path maintains a stack of modifications for auditability.
//! This allows tracking where values came from and reverting changes.
//!
//! # Example
//!
//! ```ignore
//! use crate::tui::oil::config::{ConfigStack, ConfigValue, ModSource};
//!
//! // Create a stack with a default value
//! let mut stack = ConfigStack::new(ConfigValue::Bool(true), ModSource::Default);
//!
//! // User changes via :set command
//! stack.push(ConfigValue::Bool(false), ModSource::Command);
//!
//! // Get current value
//! assert_eq!(stack.value(), &ConfigValue::Bool(false));
//! assert!(stack.is_modified());
//!
//! // Reset to base value
//! stack.reset();
//! assert!(!stack.is_modified());
//! ```

use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use super::value::ConfigValue;

/// Source of a configuration modification.
///
/// Tracks where a config value originated from for debugging and auditability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModSource {
    /// Loaded from a config file at the given path.
    File(PathBuf),
    /// Set via `:set` command in the TUI.
    Command,
    /// Set by a plugin with the given name.
    Plugin(String),
    /// Loaded from an environment variable with the given name.
    Env(String),
    /// Set via CLI flag.
    Cli,
    /// Built-in default value.
    Default,
}

impl fmt::Display for ModSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModSource::File(path) => write!(f, "file:{}", path.display()),
            ModSource::Command => write!(f, "command"),
            ModSource::Plugin(name) => write!(f, "plugin:{name}"),
            ModSource::Env(var) => write!(f, "env:{var}"),
            ModSource::Cli => write!(f, "cli"),
            ModSource::Default => write!(f, "default"),
        }
    }
}

/// A single configuration modification.
///
/// Records a value change along with when and where it originated.
#[derive(Debug, Clone)]
pub struct ConfigMod {
    /// The configuration value.
    pub value: ConfigValue,
    /// When this modification was made.
    pub timestamp: Instant,
    /// Where this modification came from.
    pub source: ModSource,
}

impl ConfigMod {
    /// Creates a new configuration modification with the current timestamp.
    pub fn new(value: ConfigValue, source: ModSource) -> Self {
        Self {
            value,
            timestamp: Instant::now(),
            source,
        }
    }
}

/// Stack of configuration modifications for a single config path.
///
/// Maintains a base value plus any runtime modifications, allowing
/// full auditability and the ability to revert changes.
#[derive(Debug, Clone)]
pub struct ConfigStack {
    /// The original/base value.
    base: ConfigMod,
    /// Runtime modifications, most recent last.
    mods: Vec<ConfigMod>,
}

impl ConfigStack {
    /// Creates a new config stack with the given base value and source.
    pub fn new(base_value: ConfigValue, source: ModSource) -> Self {
        Self {
            base: ConfigMod::new(base_value, source),
            mods: Vec::new(),
        }
    }

    /// Returns the effective (top) value.
    ///
    /// This is the most recently pushed value, or the base if no modifications exist.
    pub fn value(&self) -> &ConfigValue {
        self.mods
            .last()
            .map(|m| &m.value)
            .unwrap_or(&self.base.value)
    }

    /// Pushes a new modification onto the stack.
    pub fn push(&mut self, value: ConfigValue, source: ModSource) {
        self.mods.push(ConfigMod::new(value, source));
    }

    /// Removes and returns the most recent modification.
    ///
    /// Returns `None` if there are no modifications (only the base value remains).
    pub fn pop(&mut self) -> Option<ConfigMod> {
        self.mods.pop()
    }

    /// Clears all modifications, reverting to the base value.
    pub fn reset(&mut self) {
        self.mods.clear();
    }

    /// Returns an iterator over the full history (base + all modifications).
    ///
    /// The base value is yielded first, followed by modifications in chronological order.
    pub fn history(&self) -> impl Iterator<Item = &ConfigMod> {
        std::iter::once(&self.base).chain(self.mods.iter())
    }

    /// Returns `true` if there are any modifications on the stack.
    pub fn is_modified(&self) -> bool {
        !self.mods.is_empty()
    }

    /// Returns the source of the current effective value.
    pub fn current_source(&self) -> &ModSource {
        self.mods
            .last()
            .map(|m| &m.source)
            .unwrap_or(&self.base.source)
    }

    /// Returns the base modification (for inspection).
    pub fn base(&self) -> &ConfigMod {
        &self.base
    }

    /// Returns the number of modifications on the stack (excluding base).
    pub fn modification_count(&self) -> usize {
        self.mods.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bool_value(b: bool) -> ConfigValue {
        ConfigValue::Bool(b)
    }

    fn make_int_value(n: i64) -> ConfigValue {
        ConfigValue::Int(n)
    }

    fn make_string_value(s: &str) -> ConfigValue {
        ConfigValue::String(s.to_string())
    }

    #[test]
    fn test_stack_new_returns_base_value() {
        let stack = ConfigStack::new(make_bool_value(true), ModSource::Default);

        assert_eq!(stack.value(), &make_bool_value(true));
        assert!(!stack.is_modified());
    }

    #[test]
    fn test_stack_push_changes_value() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);

        stack.push(make_bool_value(false), ModSource::Command);

        assert_eq!(stack.value(), &make_bool_value(false));
        assert!(stack.is_modified());
    }

    #[test]
    fn test_stack_push_multiple() {
        let mut stack = ConfigStack::new(make_int_value(0), ModSource::Default);

        stack.push(make_int_value(1), ModSource::Command);
        stack.push(make_int_value(2), ModSource::Plugin("test".to_string()));
        stack.push(make_int_value(3), ModSource::Cli);

        assert_eq!(stack.value(), &make_int_value(3));
        assert_eq!(stack.modification_count(), 3);
    }

    #[test]
    fn test_stack_pop_returns_to_previous() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);
        stack.push(make_bool_value(false), ModSource::Command);

        let popped = stack.pop();

        assert!(popped.is_some());
        assert_eq!(popped.unwrap().value, make_bool_value(false));
        assert_eq!(stack.value(), &make_bool_value(true));
        assert!(!stack.is_modified());
    }

    #[test]
    fn test_stack_pop_on_empty_returns_none() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);

        let popped = stack.pop();

        assert!(popped.is_none());
        assert_eq!(stack.value(), &make_bool_value(true));
    }

    #[test]
    fn test_stack_pop_multiple() {
        let mut stack = ConfigStack::new(make_int_value(0), ModSource::Default);
        stack.push(make_int_value(1), ModSource::Command);
        stack.push(make_int_value(2), ModSource::Cli);

        assert_eq!(stack.pop().unwrap().value, make_int_value(2));
        assert_eq!(stack.value(), &make_int_value(1));

        assert_eq!(stack.pop().unwrap().value, make_int_value(1));
        assert_eq!(stack.value(), &make_int_value(0));

        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_reset_clears_all_mods() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);
        stack.push(make_bool_value(false), ModSource::Command);
        stack.push(make_bool_value(true), ModSource::Plugin("test".to_string()));
        stack.push(make_bool_value(false), ModSource::Cli);

        stack.reset();

        assert_eq!(stack.value(), &make_bool_value(true));
        assert!(!stack.is_modified());
        assert_eq!(stack.modification_count(), 0);
    }

    #[test]
    fn test_stack_history_includes_base_and_mods() {
        let mut stack = ConfigStack::new(make_int_value(0), ModSource::Default);
        stack.push(make_int_value(1), ModSource::Command);
        stack.push(make_int_value(2), ModSource::Cli);

        let history: Vec<_> = stack.history().collect();

        assert_eq!(history.len(), 3);
        assert_eq!(history[0].value, make_int_value(0));
        assert_eq!(history[1].value, make_int_value(1));
        assert_eq!(history[2].value, make_int_value(2));
    }

    #[test]
    fn test_stack_history_with_no_mods() {
        let stack = ConfigStack::new(make_string_value("test"), ModSource::Default);

        let history: Vec<_> = stack.history().collect();

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].value, make_string_value("test"));
    }

    #[test]
    fn test_is_modified_false_when_no_mods() {
        let stack = ConfigStack::new(make_bool_value(true), ModSource::Default);

        assert!(!stack.is_modified());
    }

    #[test]
    fn test_is_modified_true_when_has_mods() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);
        stack.push(make_bool_value(true), ModSource::Command); // Same value, still modified

        assert!(stack.is_modified());
    }

    #[test]
    fn test_current_source_returns_base_source_when_no_mods() {
        let stack = ConfigStack::new(make_bool_value(true), ModSource::Default);

        assert_eq!(stack.current_source(), &ModSource::Default);
    }

    #[test]
    fn test_current_source_returns_top_mod_source() {
        let mut stack = ConfigStack::new(make_bool_value(true), ModSource::Default);
        stack.push(make_bool_value(false), ModSource::Command);
        stack.push(
            make_bool_value(true),
            ModSource::Plugin("myplugin".to_string()),
        );

        assert_eq!(
            stack.current_source(),
            &ModSource::Plugin("myplugin".to_string())
        );
    }

    #[test]
    fn test_mod_source_display_file() {
        let source = ModSource::File(PathBuf::from("/home/user/.config/app.toml"));
        assert_eq!(source.to_string(), "file:/home/user/.config/app.toml");
    }

    #[test]
    fn test_mod_source_display_command() {
        let source = ModSource::Command;
        assert_eq!(source.to_string(), "command");
    }

    #[test]
    fn test_mod_source_display_plugin() {
        let source = ModSource::Plugin("syntax-highlight".to_string());
        assert_eq!(source.to_string(), "plugin:syntax-highlight");
    }

    #[test]
    fn test_mod_source_display_env() {
        let source = ModSource::Env("EDITOR".to_string());
        assert_eq!(source.to_string(), "env:EDITOR");
    }

    #[test]
    fn test_mod_source_display_cli() {
        let source = ModSource::Cli;
        assert_eq!(source.to_string(), "cli");
    }

    #[test]
    fn test_mod_source_display_default() {
        let source = ModSource::Default;
        assert_eq!(source.to_string(), "default");
    }

    #[test]
    fn test_config_mod_new_sets_timestamp() {
        let before = Instant::now();
        let config_mod = ConfigMod::new(make_bool_value(true), ModSource::Default);
        let after = Instant::now();

        assert!(config_mod.timestamp >= before);
        assert!(config_mod.timestamp <= after);
    }

    #[test]
    fn test_stack_base_returns_original() {
        let stack = ConfigStack::new(make_string_value("original"), ModSource::Default);

        assert_eq!(stack.base().value, make_string_value("original"));
        assert_eq!(stack.base().source, ModSource::Default);
    }

    #[test]
    fn test_stack_base_unchanged_after_mods() {
        let mut stack = ConfigStack::new(make_string_value("original"), ModSource::Default);
        stack.push(make_string_value("modified"), ModSource::Command);

        assert_eq!(stack.base().value, make_string_value("original"));
    }
}
