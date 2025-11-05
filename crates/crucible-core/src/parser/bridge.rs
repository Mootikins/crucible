//! Bridge between crucible-core and crucible-parser
//!
//! This module provides the dependency inversion bridge that allows
//! crucible-core to use the parser implementation without circular dependencies.

use crate::parser::error::ParserResult;
use crate::parser::traits::{MarkdownParser, ParserCapabilities};
use crate::parser::types::{DocumentContent, ParsedDocument};
use async_trait::async_trait;
use crucible_parser::MarkdownParserImplementation;
use std::path::Path;

/// Convert a parser crate ParsedDocument to a core ParsedDocument
fn convert_parsed_document(parser_doc: crucible_parser::ParsedDocument) -> ParsedDocument {
    // Create a simplified DocumentContent with just the essential fields
    let core_content = DocumentContent {
        plain_text: parser_doc.content.plain_text.clone(),
        word_count: parser_doc.content.word_count,
        char_count: parser_doc.content.char_count,
        headings: vec![],          // Skip complex conversion for now
        code_blocks: vec![],       // Skip complex conversion for now
        paragraphs: vec![],        // Skip complex conversion for now
        lists: vec![],             // Skip complex conversion for now
        latex_expressions: vec![], // Skip complex conversion for now
        callouts: vec![],          // Skip complex conversion for now
    };

    // Convert simple fields
    let core_wikilinks = vec![]; // Skip complex conversion for now
    let core_tags = vec![]; // Skip complex conversion for now
    let core_frontmatter = None; // Skip complex conversion for now

    // Create the core ParsedDocument using the legacy constructor for compatibility
    ParsedDocument::legacy(
        parser_doc.path,
        core_frontmatter,
        core_wikilinks,
        core_tags,
        core_content,
        parser_doc.parsed_at,
        parser_doc.content_hash,
        parser_doc.file_size,
    )
}

/// Adapter that implements core's MarkdownParser trait using the parser crate implementation
pub struct ParserAdapter {
    inner: crucible_parser::CrucibleParser,
}

impl ParserAdapter {
    /// Create a new adapter using the default parser implementation
    pub fn new() -> Self {
        Self {
            inner: crucible_parser::CrucibleParser::new(),
        }
    }

    /// Create an adapter with a custom parser implementation
    pub fn with_parser(parser: crucible_parser::CrucibleParser) -> Self {
        Self { inner: parser }
    }

    /// Get a reference to the inner parser
    pub fn inner(&self) -> &crucible_parser::CrucibleParser {
        &self.inner
    }
}

impl Default for ParserAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarkdownParser for ParserAdapter {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedDocument> {
        self.inner
            .parse_file(path)
            .await
            .map(convert_parsed_document)
            .map_err(|e| {
                // Convert crucible_parser::ParserError to crucible_core::parser::error::ParserError
                match e {
                    crucible_parser::ParserError::Io(io_err) => {
                        crate::parser::error::ParserError::Io(io_err)
                    }
                    crucible_parser::ParserError::FrontmatterError(msg) => {
                        crate::parser::error::ParserError::FrontmatterError(msg)
                    }
                    crucible_parser::ParserError::FileTooLarge { size, max } => {
                        crate::parser::error::ParserError::FileTooLarge { size, max }
                    }
                    crucible_parser::ParserError::EncodingError => {
                        crate::parser::error::ParserError::EncodingError
                    }
                    crucible_parser::ParserError::ParseFailed(msg) => {
                        crate::parser::error::ParserError::ParseFailed(msg)
                    }
                    crucible_parser::ParserError::Unsupported(msg) => {
                        crate::parser::error::ParserError::Unsupported(msg)
                    }
                    crucible_parser::ParserError::InvalidPath(msg) => {
                        crate::parser::error::ParserError::InvalidPath(msg)
                    }
                }
            })
    }

    fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedDocument> {
        // Use tokio runtime handle to block on the async implementation
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.inner
                    .parse_content(content, source_path)
                    .await
                    .map(convert_parsed_document)
                    .map_err(|e| {
                        // Convert error types
                        match e {
                            crucible_parser::ParserError::FrontmatterError(msg) => {
                                crate::parser::error::ParserError::FrontmatterError(msg)
                            }
                            crucible_parser::ParserError::ParseFailed(msg) => {
                                crate::parser::error::ParserError::ParseFailed(msg)
                            }
                            crucible_parser::ParserError::Unsupported(msg) => {
                                crate::parser::error::ParserError::Unsupported(msg)
                            }
                            _ => crate::parser::error::ParserError::ParseFailed(format!(
                                "Parser error: {:?}",
                                e
                            )),
                        }
                    })
            })
        })
    }

    fn capabilities(&self) -> ParserCapabilities {
        let parser_caps = self.inner.capabilities();

        // Convert from parser crate capabilities to core capabilities
        ParserCapabilities {
            name: parser_caps.name,
            version: parser_caps.version,
            yaml_frontmatter: parser_caps.yaml_frontmatter,
            toml_frontmatter: parser_caps.toml_frontmatter,
            wikilinks: parser_caps.wikilinks,
            tags: parser_caps.tags,
            headings: parser_caps.headings,
            code_blocks: parser_caps.code_blocks,
            full_content: parser_caps.full_content,
            max_file_size: parser_caps.max_file_size,
            extensions: parser_caps.extensions,
        }
    }

    fn can_parse(&self, path: &Path) -> bool {
        self.inner.can_parse(path)
    }
}

/// Factory function for creating parsers
pub fn create_parser() -> Box<dyn MarkdownParser> {
    Box::new(ParserAdapter::new())
}

/// Factory function for creating parsers with custom configuration
pub fn create_parser_with_config(config: ParserConfig) -> Box<dyn MarkdownParser> {
    let parser = match config {
        ParserConfig::Default => crucible_parser::CrucibleParser::new(),
        ParserConfig::WithMaxSize(size) => {
            crucible_parser::CrucibleParser::new().with_max_file_size(size)
        }
    };

    Box::new(ParserAdapter::with_parser(parser))
}

/// Parser configuration options
pub enum ParserConfig {
    /// Default parser configuration
    Default,
    /// Parser with custom maximum file size
    WithMaxSize(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_adapter_basic_parsing() {
        let adapter = ParserAdapter::new();
        let content = "# Test Note\n\nThis is a test.";
        let source_path = Path::new("test.md");

        let result = adapter.parse_content(content, source_path);

        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(doc.content.plain_text.contains("test"));
        assert_eq!(doc.content.word_count, 7); // "Test Note This is a test"
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = ParserAdapter::new();
        let caps = adapter.capabilities();

        assert!(caps.yaml_frontmatter);
        assert!(caps.wikilinks);
        assert!(caps.tags);
        assert!(caps.max_file_size.is_some());
    }

    #[test]
    fn test_adapter_can_parse() {
        let adapter = ParserAdapter::new();

        assert!(adapter.can_parse(Path::new("test.md")));
        assert!(adapter.can_parse(Path::new("test.markdown")));
        assert!(!adapter.can_parse(Path::new("test.txt")));
    }

    #[test]
    fn test_factory_functions() {
        let parser = create_parser();
        let caps = parser.capabilities();
        assert!(caps.wikilinks);

        let parser_with_config = create_parser_with_config(ParserConfig::WithMaxSize(1000));
        let caps_with_config = parser_with_config.capabilities();
        assert_eq!(caps_with_config.max_file_size, Some(1000));
    }
}
