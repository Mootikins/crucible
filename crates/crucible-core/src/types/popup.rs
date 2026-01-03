//! Popup entry types for cross-platform popup support
//!
//! [`PopupEntry`] is a simple, serializable type for popup items that can be
//! used from TUI, web UI, or scripting contexts.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A simple popup entry for cross-platform use
///
/// This type is designed to be:
/// - Serializable (for web UI / IPC)
/// - Simple enough for scripting (Rune, Lua)
/// - Convertible from domain-specific PopupItem enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupEntry {
    /// Primary display text (required)
    pub label: String,

    /// Secondary descriptive text (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Arbitrary data returned to caller on selection (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl PopupEntry {
    /// Create a new entry with just a label
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
            data: None,
        }
    }

    /// Add a description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add arbitrary data
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_popup_entry_roundtrip() {
        let entry = PopupEntry::new("help")
            .with_description("Show help text")
            .with_data(json!({"source": "builtin"}));

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: PopupEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.label, "help");
        assert_eq!(parsed.description.as_deref(), Some("Show help text"));
        assert_eq!(parsed.data, Some(json!({"source": "builtin"})));
    }

    #[test]
    fn test_popup_entry_minimal() {
        let entry = PopupEntry::new("quit");

        assert_eq!(entry.label, "quit");
        assert!(entry.description.is_none());
        assert!(entry.data.is_none());
    }

    #[test]
    fn test_popup_entry_skips_none_in_serialization() {
        let entry = PopupEntry::new("test");
        let json = serde_json::to_string(&entry).unwrap();

        // Should only contain "label", not "description" or "data"
        assert!(json.contains("\"label\""));
        assert!(!json.contains("\"description\""));
        assert!(!json.contains("\"data\""));
    }
}
