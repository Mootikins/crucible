//! Basic Markdown Extension using markdown-it parser
//!
//! This extension handles fundamental markdown elements:
//! - Headings (h1-h6)
//! - Paragraphs
//!
//! It uses markdown-it for robust markdown parsing.

use async_trait::async_trait;
use std::sync::Arc;

use crate::error::ParseError;
use crate::extensions::SyntaxExtension;
use crate::markdown_it::converter::AstConverter;
use crate::types::NoteContent;

/// Extension for parsing basic markdown structures using markdown-it
#[derive(Debug, Clone)]
pub struct BasicMarkdownItExtension {
    enabled: bool,
    md: Arc<markdown_it::MarkdownIt>,
}

impl BasicMarkdownItExtension {
    /// Create a new basic markdown extension using markdown-it
    pub fn new() -> Self {
        let mut md = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Add GFM tables support
        markdown_it::plugins::extra::tables::add(&mut md);

        Self {
            enabled: true,
            md: Arc::new(md),
        }
    }

    /// Create a disabled instance
    pub fn disabled() -> Self {
        let mut md = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Add GFM tables support
        markdown_it::plugins::extra::tables::add(&mut md);

        Self {
            enabled: false,
            md: Arc::new(md),
        }
    }
}

impl Default for BasicMarkdownItExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for BasicMarkdownItExtension {
    fn name(&self) -> &'static str {
        "basic-markdown-it"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn description(&self) -> &'static str {
        "Parses basic markdown structures using markdown-it: headings, paragraphs, horizontal rules, and code blocks"
    }

    fn can_handle(&self, _content: &str) -> bool {
        // This extension handles all markdown content
        true
    }

    fn priority(&self) -> u8 {
        // Highest priority - should run first to establish note structure
        100
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let errors = Vec::new();

        // Parse with markdown-it
        let ast = self.md.parse(content);

        // Convert AST to extract markdown structures
        match AstConverter::convert(&ast) {
            Ok(converted) => {
                // Merge extracted content from the AST conversion
                doc_content.headings.extend(converted.headings);
                doc_content.paragraphs.extend(converted.paragraphs);
                doc_content.horizontal_rules.extend(converted.horizontal_rules);
                doc_content.code_blocks.extend(converted.code_blocks);
                doc_content.lists.extend(converted.lists);
                doc_content.tables.extend(converted.tables);
            }
            Err(e) => {
                // Log error but don't fail - other extensions can still run
                eprintln!("markdown-it conversion error: {:?}", e);
            }
        }

        errors
    }
}

/// Create a basic markdown extension using markdown-it parser
pub fn create_basic_markdown_it_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(BasicMarkdownItExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_markdown_it_headings() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse("# Heading 1\n\n## Heading 2\n\nParagraph text.", &mut content).await;

        assert!(errors.is_empty());
        assert_eq!(content.headings.len(), 2);
        assert_eq!(content.headings[0].level, 1);
        assert_eq!(content.headings[0].text, "Heading 1");
        assert_eq!(content.headings[1].level, 2);
        assert_eq!(content.headings[1].text, "Heading 2");
    }

    #[tokio::test]
    async fn test_basic_markdown_it_paragraphs() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse("This is a paragraph.\n\nThis is another paragraph.", &mut content).await;

        assert!(errors.is_empty());
        assert!(content.paragraphs.len() >= 2);
    }
}
