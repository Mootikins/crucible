//! Basic Markdown Extension using markdown-it parser
//!
//! This extension handles fundamental markdown elements:
//! - Headings (h1-h6)
//! - Paragraphs
//!
//! It uses markdown-it for robust markdown parsing.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use super::error::ParseError;
use super::markdown_it::converter::AstConverter;
use super::types::NoteContent;

/// Extension for parsing basic markdown structures using markdown-it
#[derive(Debug, Clone)]
pub struct BasicMarkdownItExtension {
    md: Arc<markdown_it::MarkdownIt>,
}

impl BasicMarkdownItExtension {
    /// Create a new basic markdown extension using markdown-it
    pub fn new() -> Self {
        let mut md = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Add GFM tables support
        markdown_it::plugins::extra::tables::add(&mut md);

        Self { md: Arc::new(md) }
    }
}

impl Default for BasicMarkdownItExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicMarkdownItExtension {
    pub(super) fn can_handle(&self, _content: &str) -> bool {
        // This extension handles all markdown content
        true
    }

    pub(super) fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Parse with markdown-it, catching any panics (e.g., upstream bug in emph_pair.rs)
        // See: https://github.com/rlidwka/markdown-it.rs/issues/48
        let md = Arc::clone(&self.md);
        let content_owned = content.to_string();
        let parse_result = panic::catch_unwind(AssertUnwindSafe(|| md.parse(&content_owned)));

        let ast = match parse_result {
            Ok(ast) => ast,
            Err(panic_info) => {
                // Extract panic message for debugging
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };

                // Find potentially problematic emphasis patterns for diagnostics
                let problematic_patterns = find_emphasis_patterns(content);

                let error_detail = if problematic_patterns.is_empty() {
                    format!(
                        "markdown-it parser panicked: {}. Content length: {} chars",
                        panic_msg,
                        content.len()
                    )
                } else {
                    format!(
                        "markdown-it parser panicked: {}. Likely trigger patterns:\n{}",
                        panic_msg,
                        problematic_patterns.join("\n")
                    )
                };

                tracing::error!("{}", error_detail);

                errors.push(ParseError {
                    message: error_detail,
                    error_type: super::error::ParseErrorType::SyntaxError,
                    line: 0,
                    column: 0,
                    offset: 0,
                    severity: super::error::ErrorSeverity::Error,
                });

                return errors;
            }
        };

        // Convert AST to extract markdown structures
        match AstConverter::convert(&ast) {
            Ok(converted) => {
                // Merge extracted content from the AST conversion
                doc_content.headings.extend(converted.headings);
                doc_content.paragraphs.extend(converted.paragraphs);
                doc_content
                    .horizontal_rules
                    .extend(converted.horizontal_rules);
                doc_content.code_blocks.extend(converted.code_blocks);
                doc_content.lists.extend(converted.lists);
                doc_content.tables.extend(converted.tables);
            }
            Err(e) => {
                // The other extensions can still run, so the error is not fatal.
                tracing::error!(error = ?e, "markdown-it conversion failed");
            }
        }

        errors
    }
}

/// Find potentially problematic emphasis patterns that may trigger markdown-it bugs.
/// Returns a list of suspicious patterns with their line numbers.
fn find_emphasis_patterns(content: &str) -> Vec<String> {
    let mut patterns = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num + 1; // 1-indexed

        // Pattern 1: Emphasis marker at start of list item that spans lines
        // e.g., "- _foo" without closing on same line
        if (line.trim_start().starts_with("- _")
            || line.trim_start().starts_with("- *")
            || line.trim_start().starts_with("* _")
            || line.trim_start().starts_with("* *"))
            && !has_balanced_emphasis(line)
        {
            patterns.push(format!(
                "  Line {}: Unbalanced emphasis in list item: {}",
                line_num,
                truncate_line(line, 60)
            ));
        }

        // Pattern 2: Emphasis spanning indented continuation lines
        if line.starts_with("  ") && (line.contains("_") || line.contains("*")) {
            let trimmed = line.trim();
            if trimmed.ends_with('_') || trimmed.ends_with('*') {
                patterns.push(format!(
                    "  Line {}: Emphasis closing in indented block: {}",
                    line_num,
                    truncate_line(line, 60)
                ));
            }
        }

        // Pattern 3: Mixed emphasis markers that might confuse the parser
        if line.contains("_*") || line.contains("*_") {
            patterns.push(format!(
                "  Line {}: Mixed emphasis markers: {}",
                line_num,
                truncate_line(line, 60)
            ));
        }
    }

    // Limit to first 5 patterns to avoid spam
    patterns.truncate(5);
    patterns
}

/// Check if a line has balanced emphasis markers (rough heuristic)
fn has_balanced_emphasis(line: &str) -> bool {
    let underscores = line.chars().filter(|&c| c == '_').count();
    let asterisks = line.chars().filter(|&c| c == '*').count();
    underscores % 2 == 0 && asterisks % 2 == 0
}

/// Truncate a line for display, counting chars rather than bytes.
///
/// A byte-offset cut panics mid-codepoint, and this runs inside the recovery path
/// for an upstream markdown-it panic — an abort there loses the diagnostic.
fn truncate_line(line: &str, max_len: usize) -> String {
    if line.chars().count() <= max_len {
        line.to_string()
    } else {
        format!("{}...", line.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown_it_headings() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "# Heading 1\n\n## Heading 2\n\nParagraph text.",
            &mut content,
        );

        assert!(errors.is_empty());
        assert_eq!(content.headings.len(), 2);
        assert_eq!(content.headings[0].level, 1);
        assert_eq!(content.headings[0].text, "Heading 1");
        assert_eq!(content.headings[1].level, 2);
        assert_eq!(content.headings[1].text, "Heading 2");
    }

    #[test]
    fn test_basic_markdown_it_paragraphs() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "This is a paragraph.\n\nThis is another paragraph.",
            &mut content,
        );

        assert!(errors.is_empty());
        assert!(content.paragraphs.len() >= 2);
    }

    #[test]
    fn truncate_line_measures_the_limit_in_chars() {
        // 120 bytes but only 40 chars: under the limit, so it passes through whole.
        // This runs inside the panic-recovery diagnostic, so a byte-offset cut here
        // turned a recovered markdown-it panic into a process abort.
        let line = "\u{65E5}".repeat(40);

        assert_eq!(truncate_line(&line, 60), line);
    }

    #[test]
    fn truncate_line_cuts_multibyte_lines_on_a_char_boundary() {
        // Byte 60 falls inside a 日 here, which aborted the diagnostic outright.
        let line = format!("x{}", "\u{65E5}".repeat(70));

        let truncated = truncate_line(&line, 60);

        assert_eq!(truncated, format!("x{}...", "\u{65E5}".repeat(59)));
    }
}
