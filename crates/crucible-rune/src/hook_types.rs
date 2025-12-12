//! Hook types for the Rune event system
//!
//! Hooks are event handlers discovered from Rune scripts using the `#[hook(...)]` attribute.
//!
//! ## Example
//!
//! ```rune
//! /// Summarize search results for LLM consumption
//! #[hook(event = "tool:after", pattern = "gh_search_*", priority = 50)]
//! pub fn summarize_search(ctx, event) {
//!     let result = event.result;
//!     // ... transformation logic ...
//!     event.result = transformed;
//!     event  // Return modified event
//! }
//! ```

use crate::attribute_discovery::{attr_parsers, FromAttributes};
use crate::RuneError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A discovered Rune hook (event handler)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneHook {
    /// Hook name (derived from function name)
    pub name: String,

    /// Event type to handle (e.g., "tool:before", "tool:after", "note:parsed")
    pub event_type: String,

    /// Pattern to match event identifiers (glob-style, e.g., "just_*", "gh_search_*")
    pub pattern: String,

    /// Priority for handler ordering (lower = earlier, default: 100)
    pub priority: i64,

    /// Description of what this hook does
    pub description: String,

    /// Path to the .rn file
    pub path: PathBuf,

    /// Function to call in the script
    pub handler_fn: String,

    /// Whether the hook is enabled
    pub enabled: bool,
}

impl RuneHook {
    /// Create a new RuneHook with default values
    pub fn new(name: impl Into<String>, event_type: impl Into<String>, path: PathBuf) -> Self {
        let name = name.into();
        Self {
            description: format!("Rune hook: {}", name),
            event_type: event_type.into(),
            pattern: "*".to_string(),
            priority: 100,
            name: name.clone(),
            path,
            handler_fn: name,
            enabled: true,
        }
    }

    /// Set the pattern
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Check if this hook matches an event
    pub fn matches(&self, event_type: &str, identifier: &str) -> bool {
        if self.event_type != event_type {
            return false;
        }

        // Simple glob matching for patterns
        match_glob(&self.pattern, identifier)
    }
}

impl FromAttributes for RuneHook {
    fn attribute_name() -> &'static str {
        "hook"
    }

    fn from_attrs(attrs: &str, fn_name: &str, path: &Path, docs: &str) -> Result<Self, RuneError> {
        // Event type is required
        let event_type = attr_parsers::extract_string(attrs, "event").ok_or_else(|| {
            RuneError::Discovery(format!(
                "Hook '{}' missing required 'event' attribute",
                fn_name
            ))
        })?;

        // Pattern defaults to "*" (all)
        let pattern =
            attr_parsers::extract_string(attrs, "pattern").unwrap_or_else(|| "*".to_string());

        // Priority defaults to 100
        let priority = attr_parsers::extract_int(attrs, "priority").unwrap_or(100);

        // Description from attr or doc comment
        let description = attr_parsers::extract_string(attrs, "desc")
            .or_else(|| attr_parsers::extract_string(attrs, "description"))
            .or_else(|| attr_parsers::extract_doc_description(docs))
            .unwrap_or_else(|| format!("Rune hook: {}", fn_name));

        // Enabled defaults to true
        let enabled = attr_parsers::extract_bool(attrs, "enabled").unwrap_or(true);

        Ok(RuneHook {
            name: fn_name.to_string(),
            event_type,
            pattern,
            priority,
            description,
            path: path.to_path_buf(),
            handler_fn: fn_name.to_string(),
            enabled,
        })
    }
}

/// Simple glob pattern matching
fn match_glob(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Convert glob to simple regex-like matching
    let mut pattern_idx = 0;
    let mut text_idx = 0;
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut star_idx: Option<usize> = None;
    let mut match_idx: Option<usize> = None;

    while text_idx < text_chars.len() {
        if pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
            star_idx = Some(pattern_idx);
            match_idx = Some(text_idx);
            pattern_idx += 1;
        } else if pattern_idx < pattern_chars.len()
            && (pattern_chars[pattern_idx] == text_chars[text_idx]
                || pattern_chars[pattern_idx] == '?')
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            match_idx = Some(match_idx.unwrap() + 1);
            text_idx = match_idx.unwrap();
        } else {
            return false;
        }
    }

    // Check for remaining stars in pattern
    while pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern_chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute_discovery::AttributeDiscovery;

    #[test]
    fn test_match_glob_star() {
        assert!(match_glob("*", "anything"));
        assert!(match_glob("just_*", "just_test"));
        assert!(match_glob("just_*", "just_build"));
        assert!(match_glob("*_test", "unit_test"));
        assert!(match_glob("*_test_*", "unit_test_foo"));
        assert!(!match_glob("just_*", "other_test"));
    }

    #[test]
    fn test_match_glob_exact() {
        assert!(match_glob("test", "test"));
        assert!(!match_glob("test", "testing"));
    }

    #[test]
    fn test_match_glob_question() {
        assert!(match_glob("test?", "tests"));
        assert!(match_glob("t?st", "test"));
        assert!(!match_glob("test?", "test"));
    }

    #[test]
    fn test_rune_hook_matches() {
        let hook = RuneHook::new("test_hook", "tool:after", PathBuf::from("test.rn"))
            .with_pattern("just_*");

        assert!(hook.matches("tool:after", "just_test"));
        assert!(hook.matches("tool:after", "just_build"));
        assert!(!hook.matches("tool:before", "just_test")); // wrong event type
        assert!(!hook.matches("tool:after", "other_test")); // wrong pattern
    }

    #[test]
    fn test_from_attributes_basic() {
        let result = RuneHook::from_attrs(
            r#"event = "tool:after", pattern = "just_*", priority = 50"#,
            "my_hook",
            Path::new("test.rn"),
            "",
        );

        let hook = result.unwrap();
        assert_eq!(hook.name, "my_hook");
        assert_eq!(hook.event_type, "tool:after");
        assert_eq!(hook.pattern, "just_*");
        assert_eq!(hook.priority, 50);
    }

    #[test]
    fn test_from_attributes_defaults() {
        let result = RuneHook::from_attrs(
            r#"event = "note:parsed""#,
            "simple_hook",
            Path::new("test.rn"),
            "/// Hook description",
        );

        let hook = result.unwrap();
        assert_eq!(hook.name, "simple_hook");
        assert_eq!(hook.pattern, "*"); // default
        assert_eq!(hook.priority, 100); // default
        assert_eq!(hook.description, "Hook description");
    }

    #[test]
    fn test_from_attributes_missing_event() {
        let result = RuneHook::from_attrs(
            r#"pattern = "just_*""#, // missing event
            "bad_hook",
            Path::new("test.rn"),
            "",
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_discover_hooks_from_source() {
        let content = r#"
/// Filter test output for LLM consumption
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_test_output(ctx, event) {
    event
}

/// Log all tool calls
#[hook(event = "tool:after")]
pub fn log_tools(ctx, event) {
    event
}
"#;

        let discovery = AttributeDiscovery::new();
        let hooks: Vec<RuneHook> = discovery
            .parse_from_source(content, Path::new("hooks.rn"))
            .unwrap();

        assert_eq!(hooks.len(), 2);

        assert_eq!(hooks[0].name, "filter_test_output");
        assert_eq!(hooks[0].event_type, "tool:after");
        assert_eq!(hooks[0].pattern, "just_test*");
        assert_eq!(hooks[0].priority, 10);
        assert_eq!(
            hooks[0].description,
            "Filter test output for LLM consumption"
        );

        assert_eq!(hooks[1].name, "log_tools");
        assert_eq!(hooks[1].pattern, "*"); // default
        assert_eq!(hooks[1].priority, 100); // default
    }
}
