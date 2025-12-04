//! Callout and LaTeX expression types

use serde::{Deserialize, Serialize};

/// Obsidian-style callout > [!type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Callout {
    /// Callout type (note, tip, warning, danger, etc.)
    pub callout_type: String,

    /// Callout title (optional)
    pub title: Option<String>,

    /// Callout content
    pub content: String,

    /// Character offset in source note
    pub offset: usize,

    /// Whether this is a known callout type
    pub is_standard_type: bool,
}

impl Callout {
    /// Create a new callout
    pub fn new(callout_type: impl Into<String>, content: String, offset: usize) -> Self {
        let callout_type = callout_type.into();
        let is_standard_type = matches!(
            callout_type.as_str(),
            "note"
                | "tip"
                | "warning"
                | "danger"
                | "info"
                | "abstract"
                | "summary"
                | "tldr"
                | "todo"
                | "question"
                | "success"
                | "failure"
                | "example"
                | "quote"
        );

        Self {
            callout_type,
            title: None,
            content,
            offset,
            is_standard_type,
        }
    }

    /// Create a callout with title
    pub fn with_title(
        callout_type: impl Into<String>,
        title: impl Into<String>,
        content: String,
        offset: usize,
    ) -> Self {
        let mut callout = Self::new(callout_type, content, offset);
        callout.title = Some(title.into());
        callout
    }

    /// Get the display type with fallback
    pub fn display_type(&self) -> &str {
        if self.is_standard_type {
            &self.callout_type
        } else {
            "note" // fallback to generic note type
        }
    }

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }

    /// Get the total length of the callout
    pub fn length(&self) -> usize {
        // Calculate total length including callout header and content
        let header_len = if let Some(title) = &self.title {
            format!("> [!{}] {}\n", self.callout_type, title).len()
        } else {
            format!("> [!{}]\n", self.callout_type).len()
        };
        header_len + self.content.len()
    }
}

/// LaTeX mathematical expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatexExpression {
    /// LaTeX expression content
    pub expression: String,

    /// Whether this is inline ($) or block ($$) math
    pub is_block: bool,

    /// Character offset in source note
    pub offset: usize,

    /// Length of the expression in source
    pub length: usize,
}

impl LatexExpression {
    /// Create a new LaTeX expression
    pub fn new(expression: String, is_block: bool, offset: usize, length: usize) -> Self {
        Self {
            expression,
            is_block,
            offset,
            length,
        }
    }

    /// Get the expression type as a string
    pub fn expression_type(&self) -> &'static str {
        if self.is_block {
            "block"
        } else {
            "inline"
        }
    }

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }
}
