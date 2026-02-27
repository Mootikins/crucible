//! Edit and display interaction types.
//!
//! Types for artifact editing and content display.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Edit Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// Format hint for artifact content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    /// Markdown content.
    #[default]
    Markdown,
    /// Source code.
    Code,
    /// JSON data.
    Json,
    /// Plain text.
    Plain,
}

/// Request to edit an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRequest {
    /// The content to edit.
    pub content: String,
    /// Format hint for the editor.
    #[serde(default)]
    pub format: ArtifactFormat,
    /// Optional hint for what to focus on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl EditRequest {
    /// Create a new edit request.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            hint: None,
        }
    }

    /// Set the format hint.
    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the editing hint.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Response from editing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditResponse {
    /// The modified content.
    pub modified: String,
}

impl EditResponse {
    /// Create a new edit response.
    pub fn new(modified: impl Into<String>) -> Self {
        Self {
            modified: modified.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Show Request (no response)
// ─────────────────────────────────────────────────────────────────────────────

/// Request to show content to the user (display only, no response).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowRequest {
    /// The content to display.
    pub content: String,
    /// Format hint for rendering.
    #[serde(default)]
    pub format: ArtifactFormat,
    /// Optional title for the display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl ShowRequest {
    /// Create a new show request.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            title: None,
        }
    }

    /// Set the format hint.
    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_request_with_format() {
        let edit = EditRequest::new("# Plan\n\n1. Do thing")
            .format(ArtifactFormat::Markdown)
            .hint("Focus on the steps");

        assert_eq!(edit.format, ArtifactFormat::Markdown);
        assert_eq!(edit.hint, Some("Focus on the steps".into()));
    }

    #[test]
    fn show_request_with_title() {
        let show = ShowRequest::new("Some content")
            .title("Important Notice")
            .format(ArtifactFormat::Plain);

        assert_eq!(show.title, Some("Important Notice".into()));
        assert_eq!(show.format, ArtifactFormat::Plain);
    }
}
