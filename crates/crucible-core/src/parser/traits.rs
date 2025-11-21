//! Traits for markdown parsing
//!
//! This module defines the core MarkdownParser trait that all parser implementations must follow.
//! This is the canonical location for parser traits (Dependency Inversion Principle).

use crate::parser::error::ParserResult;
use crate::parser::types::ParsedNote;
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

    /// Create capabilities for a minimal parser
    pub fn minimal() -> Self {
        Self {
            name: "minimal",
            version: "0.0.0",
            yaml_frontmatter: false,
            toml_frontmatter: false,
            wikilinks: true,
            tags: true,
            headings: false,
            code_blocks: false,
            tables: false,
            callouts: false,
            latex_expressions: false,
            footnotes: false,
            blockquotes: false,
            horizontal_rules: false,
            full_content: true,
            max_file_size: Some(1024 * 1024), // 1 MB
            extensions: vec!["md"],
        }
    }
}

impl Default for ParserCapabilities {
    fn default() -> Self {
        Self::full()
    }
}

/// Requirements for parser selection
///
/// Used to select an appropriate parser implementation based on
/// required features.
#[derive(Debug, Clone, Default)]
pub struct ParserRequirements {
    /// Requires YAML frontmatter support
    pub yaml_frontmatter: bool,

    /// Requires TOML frontmatter support
    pub toml_frontmatter: bool,

    /// Requires wikilink extraction
    pub wikilinks: bool,

    /// Requires tag extraction
    pub tags: bool,

    /// Requires heading extraction
    pub headings: bool,

    /// Requires code block extraction
    pub code_blocks: bool,

    /// Minimum supported file size
    pub max_file_size: Option<usize>,
}

impl ParserRequirements {
    /// Requirements for Crucible kiln parsing (all features)
    pub fn crucible_kiln() -> Self {
        Self {
            yaml_frontmatter: true,
            toml_frontmatter: false, // Optional
            wikilinks: true,
            tags: true,
            headings: true,
            code_blocks: true,
            max_file_size: Some(10 * 1024 * 1024), // 10 MB
        }
    }

    /// Minimal requirements (links and tags only)
    pub fn links_and_tags_only() -> Self {
        Self {
            yaml_frontmatter: false,
            toml_frontmatter: false,
            wikilinks: true,
            tags: true,
            headings: false,
            code_blocks: false,
            max_file_size: None,
        }
    }
}

/// Extension trait for ParserCapabilities
pub trait ParserCapabilitiesExt {
    fn supports_all(&self, requirements: &ParserRequirements) -> bool;
}

impl ParserCapabilitiesExt for ParserCapabilities {
    /// Check if all required features are supported
    fn supports_all(&self, requirements: &ParserRequirements) -> bool {
        (!requirements.yaml_frontmatter || self.yaml_frontmatter)
            && (!requirements.toml_frontmatter || self.toml_frontmatter)
            && (!requirements.wikilinks || self.wikilinks)
            && (!requirements.tags || self.tags)
            && (!requirements.headings || self.headings)
            && (!requirements.code_blocks || self.code_blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_supports_all() {
        let caps = ParserCapabilities::full();
        let reqs = ParserRequirements::crucible_kiln();

        assert!(caps.supports_all(&reqs));
    }

    #[test]
    fn test_minimal_capabilities() {
        let caps = ParserCapabilities::minimal();
        let reqs = ParserRequirements::crucible_kiln();

        // Minimal caps don't support all crucible requirements
        assert!(!caps.supports_all(&reqs));

        // But does support minimal requirements
        let min_reqs = ParserRequirements::links_and_tags_only();
        assert!(caps.supports_all(&min_reqs));
    }
}
