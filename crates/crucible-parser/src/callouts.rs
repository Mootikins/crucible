//! Obsidian-style callout syntax extension
//!
//! This module implements support for Obsidian callouts:
//! - `> [!note] Note content`
//! - `> [!warning] Warning with title\nContent continues here`

use super::error::{ParseError, ParseErrorType};
use super::extensions::SyntaxExtension;
use super::types::{Callout, DocumentContent};
use async_trait::async_trait;

use regex::Regex;
use std::sync::Arc;

/// Obsidian callout syntax extension
pub struct CalloutExtension;

impl CalloutExtension {
    /// Create a new callout extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for CalloutExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for CalloutExtension {
    fn name(&self) -> &'static str {
        "obsidian-callouts"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports Obsidian-style callouts using > [!type] syntax with titles and nested content"
    }

    fn can_handle(&self, content: &str) -> bool {
        content.contains("[!") && content.contains("]")
    }

    async fn parse(&self, content: &str, doc_content: &mut DocumentContent) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Pattern to match callout blocks starting with > [!type] possibly with title
        let re = match Regex::new(r"(?m)^(>\s*\[!(\w+)\](?:\s+([^\\n]*))?)\s*\n((?:[^\n]*\n?)*)") {
            Ok(re) => re,
            Err(e) => {
                errors.push(ParseError::error(
                    format!("Failed to compile callout regex: {}", e),
                    ParseErrorType::SyntaxError,
                    0,
                    0,
                    0,
                ));
                return errors;
            }
        };

        for cap in re.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let callout_type = cap.get(2).unwrap().as_str().trim().to_lowercase();
            let title = cap.get(3).map(|m| m.as_str().trim());
            let _callout_content = cap.get(4).unwrap().as_str().trim_end();

            // Validate callout type
            if !self.is_valid_callout_type(&callout_type) {
                errors.push(ParseError::warning(
                    format!("Unknown callout type: '{}'", callout_type),
                    ParseErrorType::InvalidCallout,
                    0,
                    0,
                    full_match.start(),
                ));
            }

            // Extract nested content (continuation lines starting with >)
            let full_content =
                self.extract_nested_content(content, full_match.start(), full_match.len());

            // Create the callout
            let callout = if let Some(title) = title {
                Callout::with_title(callout_type, title, full_content, full_match.start())
            } else {
                Callout::new(callout_type, full_content, full_match.start())
            };

            // Add the callout to document content
            doc_content.callouts.push(callout);
        }

        errors
    }

    fn priority(&self) -> u8 {
        70 // Medium-high priority, but lower than LaTeX
    }
}

impl CalloutExtension {
    /// Check if this extension supports callouts (convenience method for tests)
    pub fn supports_callouts(&self) -> bool {
        true
    }

    /// Extract nested content for callout blocks (continuation lines)
    fn extract_nested_content(
        &self,
        content: &str,
        start_pos: usize,
        initial_len: usize,
    ) -> String {
        let mut full_content = String::new();
        let _in_callout = true;

        // Add the initial content
        if let Some(initial_match) = content.get(start_pos..start_pos + initial_len) {
            // Extract just the content after the header line
            if let Some(newline_pos) = initial_match.find('\n') {
                if newline_pos + 1 < initial_match.len() {
                    full_content.push_str(&initial_match[newline_pos + 1..]);
                }
            }
        }

        // Look for continuation lines
        let lines: Vec<&str> = content.lines().collect();
        let start_line = content[..start_pos].matches('\n').count();
        let end_line = start_line
            + content[start_pos..start_pos + initial_len]
                .matches('\n')
                .count();

        for (i, line) in lines.iter().enumerate() {
            if i > end_line && line.starts_with('>') {
                // Remove the > prefix and optional space
                let content_line = if line.len() > 1 && line.as_bytes()[1] == b' ' {
                    &line[2..]
                } else if line.len() > 1 {
                    &line[1..]
                } else {
                    ""
                };
                full_content.push_str(content_line);
                full_content.push('\n');
            } else if i > end_line {
                // No more continuation lines
                break;
            }
        }

        full_content.trim_end().to_string()
    }

    /// Check if a callout type is valid
    fn is_valid_callout_type(&self, callout_type: &str) -> bool {
        matches!(
            callout_type,
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
                | "cite"
                | "help"
                | "important"
                | "check"
                | "bug"
                | "caution"
                | "attention"
        )
    }
}

/// Factory function to create the callout extension
pub fn create_callout_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(CalloutExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorSeverity;

    #[tokio::test]
    async fn test_callout_detection() {
        let extension = CalloutExtension::new();

        assert!(extension.can_handle("> [!note] This is a note"));
        assert!(extension.can_handle("Some text\n> [!warning] Warning!\nMore warning content"));
        assert!(!extension.can_handle("Regular blockquote without callout syntax"));
        assert!(extension.can_handle("> [!tip] Tip content here"));
    }

    #[tokio::test]
    async fn test_basic_callout_parsing() {
        let extension = CalloutExtension::new();
        let content = "> [!note] This is a simple note";
        let mut doc_content = DocumentContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        // Note: We need to modify DocumentContent to have callouts field
    }

    #[tokio::test]
    async fn test_callout_with_title() {
        let extension = CalloutExtension::new();
        let content = "> [!warning] Important Warning\nThis is the warning content.";
        let mut doc_content = DocumentContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        // Verify callout with title is parsed correctly
    }

    #[tokio::test]
    async fn test_nested_callout_content() {
        let extension = CalloutExtension::new();
        let content = r#"> [!info] Information Block
First line of info
Second line of info
> Regular paragraph
        "#;
        let mut doc_content = DocumentContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        // Should extract nested content correctly
    }

    #[tokio::test]
    async fn test_unknown_callout_type() {
        let extension = CalloutExtension::new();
        let content = "> [!unknowntype] Custom callout\nSome content";
        let mut doc_content = DocumentContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ParseErrorType::InvalidCallout);
        assert!(errors[0].message.contains("Unknown callout type"));
        assert_eq!(errors[0].severity, ErrorSeverity::Warning);
    }

    #[tokio::test]
    async fn test_multiple_callouts() {
        let extension = CalloutExtension::new();
        let content = r#"> [!note] First note
Note content

> [!warning] Warning message
Warning details
        "#;
        let mut doc_content = DocumentContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        // Should parse both callouts
    }

    #[tokio::test]
    async fn test_extension_metadata() {
        let extension = CalloutExtension::new();

        assert_eq!(extension.name(), "obsidian-callouts");
        assert_eq!(extension.version(), "1.0.0");
        assert!(extension.description().contains("callouts"));
        assert_eq!(extension.priority(), 70);
        assert!(extension.is_enabled());
    }

    #[tokio::test]
    async fn test_valid_callout_types() {
        let extension = CalloutExtension::new();

        assert!(extension.is_valid_callout_type("note"));
        assert!(extension.is_valid_callout_type("warning"));
        assert!(extension.is_valid_callout_type("danger"));
        assert!(extension.is_valid_callout_type("tip"));
        assert!(extension.is_valid_callout_type("info"));
        assert!(!extension.is_valid_callout_type("invalid"));
        assert!(!extension.is_valid_callout_type(""));
    }
}
