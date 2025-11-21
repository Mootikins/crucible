//! Bridge between crucible-core and crucible-parser
//!
//! This module provides the dependency inversion bridge that allows
//! crucible-core to use the parser implementation without circular dependencies.

use crate::parser::error::ParserResult;
use crate::parser::traits::{MarkdownParser, ParserCapabilities};
use async_trait::async_trait;
use crate::parser::types::ParsedNote;
use std::path::Path;


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
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        self.inner.parse_file(path).await
    }

    async fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        self.inner.parse_content(content, source_path).await
    }

    fn capabilities(&self) -> ParserCapabilities {
        self.inner.capabilities()
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

        let result = adapter.parse_content(content, source_path).await;

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
