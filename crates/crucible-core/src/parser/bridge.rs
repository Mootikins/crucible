//! Bridge between crucible-core and crucible-parser
//!
//! This module provides the dependency inversion bridge that allows
//! crucible-core to use the parser implementation without circular dependencies.

use crate::parser::error::ParserResult;
use crate::parser::traits::{MarkdownParser, ParserCapabilities};
use async_trait::async_trait;
use crucible_parser::types::ParsedNote;
use crucible_parser::MarkdownParser as ParserMarkdownParser;
use std::path::Path;

/// Convert a parser crate ParsedNote to a core ParsedNote
///
/// NOTE: After type consolidation, ParsedNote is the same type in both crates.
/// This function is now a no-op that just returns the input directly.
fn convert_parsed_document(parser_doc: crucible_parser::ParsedNote) -> ParsedNote {
    parser_doc
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

/// Convert parser crate capabilities to core capabilities
fn convert_capabilities(parser_caps: crucible_parser::ParserCapabilities) -> ParserCapabilities {
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

/// Convert parser crate error to core error
fn convert_parser_error(e: crucible_parser::ParserError) -> crate::parser::error::ParserError {
    match e {
        crucible_parser::ParserError::Io(io_err) => {
            crate::parser::error::ParserError::Io(io_err)
        }
        crucible_parser::ParserError::FrontmatterError(msg) => {
            crate::parser::error::ParserError::FrontmatterError(msg)
        }
        crucible_parser::ParserError::FrontmatterTooLarge { size, max } => {
            crate::parser::error::ParserError::FrontmatterTooLarge { size, max }
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
}

#[async_trait]
impl MarkdownParser for ParserAdapter {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        self.inner
            .parse_file(path)
            .await
            .map(convert_parsed_document)
            .map_err(convert_parser_error)
    }

    fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        // Use tokio runtime handle to block on the async implementation
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.inner
                    .parse_content(content, source_path)
                    .await
                    .map(convert_parsed_document)
                    .map_err(convert_parser_error)
            })
        })
    }

    fn capabilities(&self) -> ParserCapabilities {
        convert_capabilities(self.inner.capabilities())
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
