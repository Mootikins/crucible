//! Footnote syntax extension for markdown parsing
//!
//! This module implements support for standard markdown footnotes:
//! - Reference footnotes: `[^1]`, `[^note]`, `[^custom-reference]`
//! - Definition footnotes: `[^1]: Footnote content here`
//! - Multi-line footnote definitions with proper indentation
//! - Inline footnotes: `This is text with a^footnote inline footnote`
//! - Validation for orphaned references and duplicate definitions
//! - Sequential numbering for ordered footnote display

use super::error::{ParseError, ParseErrorType};
use super::extensions::SyntaxExtension;
use super::types::{FootnoteDefinition, FootnoteReference, NoteContent};
use async_trait::async_trait;

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

static REFERENCE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\^([\w\-\s]+)\]").expect("footnote reference regex"));

static DEFINITION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\[\^([\w\-\s]+)\]:[ \t]*(.*)$").expect("footnote definition regex"));

/// Footnote syntax extension
pub struct FootnoteExtension;

impl FootnoteExtension {
    /// Create a new footnote extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for FootnoteExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for FootnoteExtension {
    fn name(&self) -> &'static str {
        "markdown-footnotes"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports standard markdown footnotes including references, definitions, and inline footnotes with validation"
    }

    fn can_handle(&self, content: &str) -> bool {
        content.contains("[^") || content.contains('^')
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();
        let footnotes = &mut doc_content.footnotes;

        // Track definitions and references for validation
        let mut definitions_found: HashMap<String, FootnoteDefinition> = HashMap::new();
        let mut references_found: Vec<FootnoteReference> = Vec::new();
        let mut duplicate_identifiers: HashSet<String> = HashSet::new();

        // Parse footnote definitions first
        for cap in DEFINITION_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let identifier = cap.get(1).unwrap().as_str().trim();
            let initial_content = cap.get(2).unwrap().as_str().trim();

            let offset = full_match.start();
            let line_number = content[..offset].matches('\n').count() + 1;

            // Check for duplicate definitions
            if definitions_found.contains_key(identifier) {
                if !duplicate_identifiers.contains(identifier) {
                    duplicate_identifiers.insert(identifier.to_string());
                    errors.push(ParseError::error(
                        format!("Duplicate footnote definition: '{}'", identifier),
                        ParseErrorType::DuplicateFootnoteDefinition,
                        line_number,
                        0,
                        offset,
                    ));
                }
                continue;
            }

            // Extract multi-line content
            let footnote_content = self.extract_multiline_definition(
                content,
                offset,
                full_match.len(),
                initial_content,
            );

            let definition = FootnoteDefinition::new(
                identifier.to_string(),
                footnote_content,
                offset,
                line_number,
            );

            definitions_found.insert(identifier.to_string(), definition);
        }

        // Parse footnote references
        for cap in REFERENCE_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let identifier = cap.get(1).unwrap().as_str().trim();
            let offset = full_match.start();

            let reference = FootnoteReference::new(identifier.to_string(), offset);
            references_found.push(reference);
        }

        // Parse inline footnotes manually
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '^' && i > 0 && chars[i - 1] != '[' {
                // Find closing ^
                if let Some(end_pos) = content[i + 1..].find('^') {
                    let inline_end = i + 1 + end_pos;
                    if inline_end < chars.len() {
                        let inline_content = &content[i + 1..inline_end];
                        // Only consider it a valid inline footnote if it has meaningful content
                        if !inline_content.is_empty() && inline_content.len() > 1 {
                            let offset = i;

                            // Create a unique identifier for inline footnotes
                            let inline_identifier = format!("inline-{}", offset);

                            // Create inline footnote definition
                            let definition = FootnoteDefinition::new(
                                inline_identifier.clone(),
                                inline_content.to_string(),
                                offset,
                                content[..offset].matches('\n').count() + 1,
                            );

                            // Create reference for inline footnote
                            let reference =
                                FootnoteReference::new(inline_identifier.clone(), offset);

                            definitions_found.insert(inline_identifier, definition);
                            references_found.push(reference);

                            i = inline_end + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }

        // Filter out references that are actually footnote definitions
        // But keep inline footnote references (they have same position as their definitions)
        let definition_positions: HashSet<usize> = definitions_found
            .values()
            .filter(|def| !def.identifier.starts_with("inline-"))
            .map(|def| def.offset)
            .collect();

        references_found.retain(|ref_| !definition_positions.contains(&ref_.offset));

        // Validate references and definitions
        self.validate_footnotes(&references_found, &definitions_found, &mut errors);

        // Assign order numbers to references based on note order
        let mut ordered_references = Vec::new();
        let mut seen_identifiers: HashSet<String> = HashSet::new();
        let mut order_counter = 1;

        for reference in references_found.iter() {
            if definitions_found.contains_key(&reference.identifier) {
                // Only count the first occurrence of each identifier for ordering
                if !seen_identifiers.contains(&reference.identifier) {
                    seen_identifiers.insert(reference.identifier.clone());
                    ordered_references.push(FootnoteReference::with_order(
                        reference.identifier.clone(),
                        reference.offset,
                        order_counter,
                    ));
                    order_counter += 1;
                } else {
                    // Duplicate reference without order number
                    ordered_references.push(FootnoteReference::new(
                        reference.identifier.clone(),
                        reference.offset,
                    ));
                }
            } else {
                // Orphaned reference - add without order number
                ordered_references.push(FootnoteReference::new(
                    reference.identifier.clone(),
                    reference.offset,
                ));
            }
        }

        // Update footnotes with parsed data
        footnotes.references = ordered_references;
        for (_, definition) in definitions_found {
            footnotes.add_definition(definition.identifier.clone(), definition);
        }

        errors
    }

    fn priority(&self) -> u8 {
        80 // High priority, but lower than core parsing
    }
}

impl FootnoteExtension {
    /// Extract multi-line footnote definition content
    ///
    /// Handles indented continuation lines and proper paragraph separation.
    fn extract_multiline_definition(
        &self,
        content: &str,
        start_pos: usize,
        _initial_len: usize,
        initial_content: &str,
    ) -> String {
        let mut full_content = initial_content.to_string();

        // Find the position after the definition line
        let lines: Vec<&str> = content.lines().collect();
        let start_line = content[..start_pos].matches('\n').count();

        // Process all subsequent lines that are properly indented
        for line in lines.iter().skip(start_line + 1) {
            if self.is_definition_continuation(line) {
                // Extract the content (remove indentation)
                let continuation_content = line.trim_start();
                if !full_content.is_empty() {
                    full_content.push(' ');
                }
                full_content.push_str(continuation_content);
            } else {
                // Stop at first non-indented line
                break;
            }
        }

        full_content.trim().to_string()
    }

    /// Check if a line is a continuation of a footnote definition
    ///
    /// A line is considered a continuation if it starts with at least 4 spaces
    /// or a tab character (standard markdown indentation).
    fn is_definition_continuation(&self, line: &str) -> bool {
        line.starts_with("    ") || line.starts_with('\t')
    }

    /// Validate footnote references and definitions
    ///
    /// Checks for orphaned references and unused definitions.
    fn validate_footnotes(
        &self,
        references: &[FootnoteReference],
        definitions: &HashMap<String, FootnoteDefinition>,
        errors: &mut Vec<ParseError>,
    ) {
        // Find orphaned references
        let mut reference_counts: HashMap<String, usize> = HashMap::new();
        for reference in references {
            *reference_counts
                .entry(reference.identifier.clone())
                .or_insert(0) += 1;
        }

        for reference in references {
            if !definitions.contains_key(&reference.identifier) {
                errors.push(ParseError::warning(
                    format!(
                        "Orphaned footnote reference: '{}' (no definition found)",
                        reference.identifier
                    ),
                    ParseErrorType::OrphanedFootnoteReference,
                    0, // We don't have line number here
                    0,
                    reference.offset,
                ));
            }
        }

        // Find unused definitions
        for definition_identifier in definitions.keys() {
            if !reference_counts.contains_key(definition_identifier) {
                errors.push(ParseError::warning(
                    format!(
                        "Unused footnote definition: '{}' (no references found)",
                        definition_identifier
                    ),
                    ParseErrorType::UnusedFootnoteDefinition,
                    definitions.get(definition_identifier).unwrap().line_number,
                    0,
                    definitions.get(definition_identifier).unwrap().offset,
                ));
            }
        }
    }
}

/// Factory function to create the footnote extension
pub fn create_footnote_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(FootnoteExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorSeverity;

    #[tokio::test]
    async fn test_footnote_detection() {
        let extension = FootnoteExtension::new();

        assert!(extension.can_handle("This has a footnote [^1] reference"));
        assert!(extension.can_handle("And here's a definition [^note]: Content"));
        assert!(extension.can_handle("Inline footnote^content here"));
        assert!(!extension.can_handle("Regular text without footnotes"));
    }

    #[tokio::test]
    async fn test_basic_footnote_parsing() {
        let extension = FootnoteExtension::new();
        let content = r#"This is text with a footnote[^1].

[^1]: This is the footnote content."#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 1);
        assert_eq!(doc_content.footnotes.definitions.len(), 1);

        let reference = &doc_content.footnotes.references[0];
        assert_eq!(reference.identifier, "1");
        assert_eq!(reference.order_number, Some(1));

        let definition = doc_content.footnotes.get_definition("1").unwrap();
        assert_eq!(definition.identifier, "1");
        assert_eq!(definition.content, "This is the footnote content.");
    }

    #[tokio::test]
    async fn test_multiline_footnote_definition() {
        let extension = FootnoteExtension::new();
        let content = r#"Text with footnote[^multiline].

[^multiline]: First line of footnote
    Second line indented
    Third line with more content
    Final paragraph of footnote"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.definitions.len(), 1);

        let definition = doc_content.footnotes.get_definition("multiline").unwrap();
        assert!(definition.content.contains("First line"));
        assert!(definition.content.contains("Second line"));
        assert!(definition.content.contains("Third line"));
        assert!(definition.content.contains("Final paragraph"));
    }

    #[tokio::test]
    async fn test_inline_footnotes() {
        let extension = FootnoteExtension::new();
        let content = "This text has an^inline footnote^ right in the middle.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 1);
        assert_eq!(doc_content.footnotes.definitions.len(), 1);

        let definition = doc_content
            .footnotes
            .get_definition(&doc_content.footnotes.references[0].identifier)
            .unwrap();
        assert_eq!(definition.content, "inline footnote");
    }

    #[tokio::test]
    async fn test_multiple_footnotes() {
        let extension = FootnoteExtension::new();
        let content = r#"First footnote[^1] and second footnote[^2].

[^1]: First footnote content.
[^2]: Second footnote content."#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 2);
        assert_eq!(doc_content.footnotes.definitions.len(), 2);

        // Check order numbering
        let ref1 = doc_content
            .footnotes
            .references
            .iter()
            .find(|r| r.identifier == "1")
            .unwrap();
        let ref2 = doc_content
            .footnotes
            .references
            .iter()
            .find(|r| r.identifier == "2")
            .unwrap();
        assert_eq!(ref1.order_number, Some(1));
        assert_eq!(ref2.order_number, Some(2));
    }

    #[tokio::test]
    async fn test_duplicate_footnote_definitions() {
        let extension = FootnoteExtension::new();
        let content = r#"Reference[^dup].

[^dup]: First definition
[^dup]: Second definition"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].error_type,
            ParseErrorType::DuplicateFootnoteDefinition
        );
        assert_eq!(errors[0].severity, ErrorSeverity::Error);
        // Should only keep the first definition
        assert_eq!(doc_content.footnotes.definitions.len(), 1);
    }

    #[tokio::test]
    async fn test_orphaned_footnote_reference() {
        let extension = FootnoteExtension::new();
        let content = "This has an orphaned footnote[^missing].";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].error_type,
            ParseErrorType::OrphanedFootnoteReference
        );
        assert_eq!(errors[0].severity, ErrorSeverity::Warning);
        assert_eq!(doc_content.footnotes.references.len(), 1);
        assert_eq!(doc_content.footnotes.definitions.len(), 0);
    }

    #[tokio::test]
    async fn test_unused_footnote_definition() {
        let extension = FootnoteExtension::new();
        let content = r#"This text has no references.

[^unused]: This definition is never referenced."#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].error_type,
            ParseErrorType::UnusedFootnoteDefinition
        );
        assert_eq!(errors[0].severity, ErrorSeverity::Warning);
        assert_eq!(doc_content.footnotes.references.len(), 0);
        assert_eq!(doc_content.footnotes.definitions.len(), 1);
    }

    #[tokio::test]
    async fn test_complex_footnote_identifiers() {
        let extension = FootnoteExtension::new();
        let content = r#"Complex identifiers[^custom-note] and numbers[^123].

[^custom-note]: Custom identifier with hyphens
[^123]: Numeric identifier"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 2);
        assert_eq!(doc_content.footnotes.definitions.len(), 2);
        assert!(doc_content
            .footnotes
            .get_definition("custom-note")
            .is_some());
        assert!(doc_content.footnotes.get_definition("123").is_some());
    }

    #[tokio::test]
    async fn test_repeated_footnote_references() {
        let extension = FootnoteExtension::new();
        let content = r#"First reference[^1] and second reference[^1].

[^1]: Shared footnote content"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 2);
        assert_eq!(doc_content.footnotes.definitions.len(), 1);

        // Only the first reference should have an order number
        let ordered_refs: Vec<_> = doc_content
            .footnotes
            .references
            .iter()
            .filter(|r| r.order_number.is_some())
            .collect();
        assert_eq!(ordered_refs.len(), 1);
    }

    #[tokio::test]
    async fn test_extension_metadata() {
        let extension = FootnoteExtension::new();

        assert_eq!(extension.name(), "markdown-footnotes");
        assert_eq!(extension.version(), "1.0.0");
        assert!(extension.description().contains("footnotes"));
        assert_eq!(extension.priority(), 80);
        assert!(extension.is_enabled());
    }

    #[tokio::test]
    async fn test_definition_continuation_detection() {
        let extension = FootnoteExtension::new();

        assert!(extension.is_definition_continuation("    Indented line"));
        assert!(extension.is_definition_continuation("\tTab indented line"));
        assert!(!extension.is_definition_continuation("Regular line"));
        assert!(!extension.is_definition_continuation("  Two spaces only"));
    }

    #[tokio::test]
    async fn test_mixed_content_with_footnotes() {
        let extension = FootnoteExtension::new();
        let content = r#"# Title

This note has various content:

- List item with footnote[^1]
- Another item with^inline footnote

[^1]: Footnote for list item

## Subsection

More text here."#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.footnotes.references.len(), 2);
        assert_eq!(doc_content.footnotes.definitions.len(), 2);
    }
}
