//! Handler types for the Rune event system
//!
//! Handlers are event processors discovered from Rune scripts using the `#[handler(...)]` attribute.
//!
//! ## Example
//!
//! ```rune
//! /// Summarize search results for LLM consumption
//! #[handler(event = "tool:after", pattern = "gh_search_*", priority = 50)]
//! pub fn summarize_search(ctx, event) {
//!     let result = event.result;
//!     // ... transformation logic ...
//!     event.result = transformed;
//!     event  // Return modified event
//! }
//! ```

use crate::attribute_discovery::{attr_parsers, FromAttributes};
use crate::RuneError;
use crucible_core::utils::glob_match;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A discovered Rune handler (event processor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneHandler {
    /// Handler name (derived from function name)
    pub name: String,

    /// Event type to handle (e.g., "tool:before", "tool:after", "note:parsed")
    pub event_type: String,

    /// Pattern to match event identifiers (glob-style, e.g., "just_*", "gh_search_*")
    pub pattern: String,

    /// Priority for handler ordering (lower = earlier, default: 100)
    pub priority: i64,

    /// Description of what this handler does
    pub description: String,

    /// Path to the .rn file
    pub path: PathBuf,

    /// Function to call in the script
    pub handler_fn: String,

    /// Whether the handler is enabled
    pub enabled: bool,
}

impl RuneHandler {
    /// Create a new RuneHandler with default values
    pub fn new(name: impl Into<String>, event_type: impl Into<String>, path: PathBuf) -> Self {
        let name = name.into();
        Self {
            description: format!("Rune handler: {}", name),
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

    /// Check if this handler matches an event
    pub fn matches(&self, event_type: &str, identifier: &str) -> bool {
        if self.event_type != event_type {
            return false;
        }

        // Simple glob matching for patterns
        glob_match(&self.pattern, identifier)
    }
}

impl FromAttributes for RuneHandler {
    fn attribute_name() -> &'static str {
        "handler"
    }

    fn alternate_names() -> &'static [&'static str] {
        &["hook"] // Backwards compatibility
    }

    fn from_attrs(attrs: &str, fn_name: &str, path: &Path, docs: &str) -> Result<Self, RuneError> {
        // Event type is required
        let event_type = attr_parsers::extract_string(attrs, "event").ok_or_else(|| {
            RuneError::Discovery(format!(
                "Handler '{}' missing required 'event' attribute",
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
            .unwrap_or_else(|| format!("Rune handler: {}", fn_name));

        // Enabled defaults to true
        let enabled = attr_parsers::extract_bool(attrs, "enabled").unwrap_or(true);

        Ok(RuneHandler {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute_discovery::AttributeDiscovery;

    #[test]
    fn test_match_glob_star() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("just_*", "just_test"));
        assert!(glob_match("just_*", "just_build"));
        assert!(glob_match("*_test", "unit_test"));
        assert!(glob_match("*_test_*", "unit_test_foo"));
        assert!(!glob_match("just_*", "other_test"));
    }

    #[test]
    fn test_match_glob_exact() {
        assert!(glob_match("test", "test"));
        assert!(!glob_match("test", "testing"));
    }

    #[test]
    fn test_match_glob_question() {
        assert!(glob_match("test?", "tests"));
        assert!(glob_match("t?st", "test"));
        assert!(!glob_match("test?", "test"));
    }

    #[test]
    fn test_rune_handler_matches() {
        let handler = RuneHandler::new("test_handler", "tool:after", PathBuf::from("test.rn"))
            .with_pattern("just_*");

        assert!(handler.matches("tool:after", "just_test"));
        assert!(handler.matches("tool:after", "just_build"));
        assert!(!handler.matches("tool:before", "just_test")); // wrong event type
        assert!(!handler.matches("tool:after", "other_test")); // wrong pattern
    }

    #[test]
    fn test_from_attributes_basic() {
        let result = RuneHandler::from_attrs(
            r#"event = "tool:after", pattern = "just_*", priority = 50"#,
            "my_handler",
            Path::new("test.rn"),
            "",
        );

        let handler = result.unwrap();
        assert_eq!(handler.name, "my_handler");
        assert_eq!(handler.event_type, "tool:after");
        assert_eq!(handler.pattern, "just_*");
        assert_eq!(handler.priority, 50);
    }

    #[test]
    fn test_from_attributes_defaults() {
        let result = RuneHandler::from_attrs(
            r#"event = "note:parsed""#,
            "simple_handler",
            Path::new("test.rn"),
            "/// Handler description",
        );

        let handler = result.unwrap();
        assert_eq!(handler.name, "simple_handler");
        assert_eq!(handler.pattern, "*"); // default
        assert_eq!(handler.priority, 100); // default
        assert_eq!(handler.description, "Handler description");
    }

    #[test]
    fn test_from_attributes_missing_event() {
        let result = RuneHandler::from_attrs(
            r#"pattern = "just_*""#, // missing event
            "bad_handler",
            Path::new("test.rn"),
            "",
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_discover_handlers_from_source() {
        let content = r#"
/// Filter test output for LLM consumption
#[handler(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_test_output(ctx, event) {
    event
}

/// Log all tool calls
#[handler(event = "tool:after")]
pub fn log_tools(ctx, event) {
    event
}
"#;

        let discovery = AttributeDiscovery::new();
        let handlers: Vec<RuneHandler> = discovery
            .parse_from_source(content, Path::new("handlers.rn"))
            .unwrap();

        assert_eq!(handlers.len(), 2);

        assert_eq!(handlers[0].name, "filter_test_output");
        assert_eq!(handlers[0].event_type, "tool:after");
        assert_eq!(handlers[0].pattern, "just_test*");
        assert_eq!(handlers[0].priority, 10);
        assert_eq!(
            handlers[0].description,
            "Filter test output for LLM consumption"
        );

        assert_eq!(handlers[1].name, "log_tools");
        assert_eq!(handlers[1].pattern, "*"); // default
        assert_eq!(handlers[1].priority, 100); // default
    }

    #[test]
    fn test_discover_handlers_from_hook_attribute() {
        // Backwards compatibility: #[hook(...)] still works
        let content = r#"
/// Legacy hook using old attribute
#[hook(event = "tool:after", pattern = "*")]
pub fn legacy_handler(ctx, event) {
    event
}
"#;

        let discovery = AttributeDiscovery::new();
        let handlers: Vec<RuneHandler> = discovery
            .parse_from_source(content, Path::new("legacy.rn"))
            .unwrap();

        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name, "legacy_handler");
    }
}
