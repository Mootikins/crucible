//! Inline metadata types for Dataview-style metadata fields
//!
//! Supports Dataview-style inline metadata:
//! - Single value: `[key:: value]`
//! - Array value: `[key:: val1, val2, val3]`

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Inline metadata field in Dataview style
///
/// Examples:
/// - `[priority:: high]` - single value
/// - `[tags:: rust, parsing, metadata]` - array value
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineMetadata {
    /// Metadata key
    pub key: String,

    /// Metadata values (single value = vec of 1)
    pub values: Vec<String>,
}

impl InlineMetadata {
    /// Create new inline metadata with a single value
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            values: vec![value.into()],
        }
    }

    /// Create new inline metadata with multiple values
    pub fn new_array(key: impl Into<String>, values: Vec<String>) -> Self {
        Self {
            key: key.into(),
            values,
        }
    }

    /// Check if this metadata has multiple values (is an array)
    pub fn is_array(&self) -> bool {
        self.values.len() > 1
    }

    /// Get the value as a single string
    ///
    /// Returns the first value if this is an array.
    pub fn as_string(&self) -> Option<&str> {
        self.values.first().map(|s| s.as_str())
    }

    /// Get all values as a vector
    pub fn as_vec(&self) -> &[String] {
        &self.values
    }
}

/// Extract all inline metadata fields from text
///
/// Parses Dataview-style inline metadata: `[key:: value]` or `[key:: val1, val2, val3]`
///
/// # Examples
///
/// ```
/// use crucible_core::parser::types::InlineMetadata;
///
/// let text = "task [id:: 1.1] [status:: done]";
/// let fields = crucible_core::parser::types::extract_inline_metadata(text);
/// assert_eq!(fields.len(), 2);
/// assert_eq!(fields[0].key, "id");
/// assert_eq!(fields[1].key, "status");
/// ```
pub fn extract_inline_metadata(text: &str) -> Vec<InlineMetadata> {
    // Pattern: [key:: value] with double colon
    let re = Regex::new(r"\[([^:]+)::\s*([^\]]+)\]").expect("valid regex");

    re.captures_iter(text)
        .map(|cap| {
            let key = cap[1].trim().to_string();
            let value_str = cap[2].trim();

            // Split on comma to detect arrays
            let values: Vec<String> = value_str.split(',').map(|v| v.trim().to_string()).collect();

            InlineMetadata { key, values }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_metadata_new() {
        // Single value
        let meta = InlineMetadata::new("priority", "high");
        assert_eq!(meta.key, "priority");
        assert_eq!(meta.values.len(), 1);
        assert_eq!(meta.values[0], "high");

        // Array value
        let meta = InlineMetadata::new_array(
            "tags",
            vec![
                "rust".to_string(),
                "parsing".to_string(),
                "metadata".to_string(),
            ],
        );
        assert_eq!(meta.key, "tags");
        assert_eq!(meta.values.len(), 3);
        assert_eq!(meta.values[0], "rust");
        assert_eq!(meta.values[1], "parsing");
        assert_eq!(meta.values[2], "metadata");
    }

    #[test]
    fn inline_metadata_is_array() {
        // Single value should NOT be an array
        let single = InlineMetadata::new("status", "done");
        assert!(!single.is_array());

        // Multiple values should be an array
        let array =
            InlineMetadata::new_array("labels", vec!["bug".to_string(), "urgent".to_string()]);
        assert!(array.is_array());

        // Edge case: empty values
        let empty = InlineMetadata::new_array("empty", vec![]);
        assert!(!empty.is_array());
    }

    #[test]
    fn inline_metadata_as_string() {
        // Single value
        let single = InlineMetadata::new("author", "Alice");
        assert_eq!(single.as_string(), Some("Alice"));

        // Array - should return first value
        let array =
            InlineMetadata::new_array("contributors", vec!["Alice".to_string(), "Bob".to_string()]);
        assert_eq!(array.as_string(), Some("Alice"));

        // Empty values
        let empty = InlineMetadata::new_array("empty", vec![]);
        assert_eq!(empty.as_string(), None);
    }

    #[test]
    fn inline_metadata_as_array() {
        // Single value - should return vec with 1 element
        let single = InlineMetadata::new("priority", "high");
        let vec = single.as_vec();
        assert_eq!(vec.len(), 1);
        assert_eq!(vec[0], "high");

        // Multiple values
        let array =
            InlineMetadata::new_array("tags", vec!["rust".to_string(), "parser".to_string()]);
        let vec = array.as_vec();
        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0], "rust");
        assert_eq!(vec[1], "parser");
    }

    #[test]
    fn extract_single_field() {
        let text = "task [id:: 1.1]";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "id");
        assert_eq!(fields[0].values, vec!["1.1"]);
    }

    #[test]
    fn extract_multiple_fields() {
        let text = "task [id:: 1.1] [status:: done]";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].key, "id");
        assert_eq!(fields[0].values, vec!["1.1"]);
        assert_eq!(fields[1].key, "status");
        assert_eq!(fields[1].values, vec!["done"]);
    }

    #[test]
    fn extract_array_field() {
        let text = "[deps:: a, b, c]";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "deps");
        assert_eq!(fields[0].values, vec!["a", "b", "c"]);
    }

    #[test]
    fn extract_mixed_fields() {
        let text = "task [id:: 1.1] [deps:: a, b, c] [status:: done]";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].key, "id");
        assert_eq!(fields[0].values, vec!["1.1"]);
        assert_eq!(fields[1].key, "deps");
        assert_eq!(fields[1].values, vec!["a", "b", "c"]);
        assert_eq!(fields[2].key, "status");
        assert_eq!(fields[2].values, vec!["done"]);
    }

    #[test]
    fn no_fields_returns_empty() {
        let text = "just plain text without any metadata";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn malformed_field_ignored() {
        let text = "valid [id:: 1.1] invalid [key: value] also valid [status:: done]";
        let fields = super::extract_inline_metadata(text);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].key, "id");
        assert_eq!(fields[1].key, "status");
    }
}
