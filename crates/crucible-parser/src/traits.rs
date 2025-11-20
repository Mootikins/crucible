//! Traits for crucible-parser implementation

use crate::error::ParserResult;
use crate::types::ParsedNote;
use async_trait::async_trait;
use std::path::Path;

/// Core trait for parsing markdown documents
///
/// This trait defines the interface for parsing markdown files into structured
/// `ParsedNote` instances. This is the main parser trait used throughout
/// the crucible system.
#[async_trait]
pub trait MarkdownParser: Send + Sync {
    /// Parse a markdown file from the filesystem
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote>;

    /// Parse markdown content from a string
    async fn parse_content(
        &self,
        content: &str,
        source_path: &Path,
    ) -> ParserResult<ParsedNote>;

    /// Get parser capabilities
    fn capabilities(&self) -> ParserCapabilities;

    /// Validate if the parser can handle this file
    fn can_parse(&self, path: &Path) -> bool;
}

/// Parser capabilities and configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserCapabilities {
    pub name: &'static str,
    pub version: &'static str,
    pub yaml_frontmatter: bool,
    pub toml_frontmatter: bool,
    pub wikilinks: bool,
    pub tags: bool,
    pub headings: bool,
    pub code_blocks: bool,
    pub tables: bool,
    pub callouts: bool,
    pub latex_expressions: bool,
    pub footnotes: bool,
    pub blockquotes: bool,
    pub horizontal_rules: bool,
    pub full_content: bool,
    pub max_file_size: Option<usize>,
    pub extensions: Vec<&'static str>,
}

impl ParserCapabilities {
    pub fn full() -> Self {
        Self {
            name: "crucible-parser",
            version: env!("CARGO_PKG_VERSION"),
            yaml_frontmatter: true,
            toml_frontmatter: true,
            wikilinks: true,
            tags: true,
            headings: true,
            code_blocks: true,
            tables: true,
            callouts: true,
            latex_expressions: true,
            footnotes: true,
            blockquotes: true,
            horizontal_rules: true,
            full_content: true,
            max_file_size: Some(10 * 1024 * 1024),
            extensions: vec!["md", "markdown"],
        }
    }
}

impl Default for ParserCapabilities {
    fn default() -> Self {
        Self::full()
    }
}
