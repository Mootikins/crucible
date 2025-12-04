//! Block types: tables, blockquotes, and horizontal rules

use serde::{Deserialize, Serialize};

/// A markdown table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Raw table content (with pipes and formatting)
    pub raw_content: String,
    /// Table headers
    pub headers: Vec<String>,
    /// Number of columns
    pub columns: usize,
    /// Number of data rows (excluding header)
    pub rows: usize,
    /// Character offset in source
    pub offset: usize,
}

impl Table {
    /// Create a new table
    pub fn new(
        raw_content: String,
        headers: Vec<String>,
        columns: usize,
        rows: usize,
        offset: usize,
    ) -> Self {
        Self {
            raw_content,
            headers,
            columns,
            rows,
            offset,
        }
    }
}

/// Blockquote content (not a callout)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blockquote {
    /// Blockquote content
    pub content: String,
    /// Nesting level (0 for single >, 1 for >>, etc.)
    pub nested_level: u8,
    /// Character offset in source
    pub offset: usize,
}

impl Blockquote {
    /// Create a new blockquote
    pub fn new(content: String, offset: usize) -> Self {
        Self {
            content,
            nested_level: 0,
            offset,
        }
    }

    /// Create a new blockquote with nesting level
    pub fn with_nesting(content: String, nested_level: u8, offset: usize) -> Self {
        Self {
            content,
            nested_level,
            offset,
        }
    }
}

/// A horizontal rule / thematic break
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HorizontalRule {
    /// Raw content (e.g., "---" or "***")
    pub raw_content: String,

    /// Style indicator (dash, asterisk, underscore)
    pub style: String,

    /// Character offset in source note
    pub offset: usize,
}

impl HorizontalRule {
    /// Create a new horizontal rule
    pub fn new(raw_content: String, style: String, offset: usize) -> Self {
        Self {
            raw_content,
            style,
            offset,
        }
    }

    /// Detect style from raw content
    pub fn detect_style(content: &str) -> String {
        if content.contains('-') {
            "dash".to_string()
        } else if content.contains('*') {
            "asterisk".to_string()
        } else if content.contains('_') {
            "underscore".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Get the length of the horizontal rule
    pub fn length(&self) -> usize {
        self.raw_content.len()
    }
}
