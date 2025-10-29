//! Traits for markdown parsing

use super::error::ParserResult;
use super::types::ParsedDocument;
use async_trait::async_trait;
use std::path::Path;

/// Core trait for parsing markdown documents
///
/// This trait defines the interface for parsing markdown files into structured
/// `ParsedDocument` instances. Implementations should handle:
/// - Frontmatter extraction (YAML/TOML)
/// - Wikilink parsing [[note]]
/// - Tag extraction #tag
/// - Content structure (headings, code blocks, plain text)
///
/// # Performance Expectations
///
/// Implementations should target:
/// - ~1ms per 50 KB markdown file
/// - Zero-copy parsing where possible
/// - Incremental parsing for large files
///
/// # Thread Safety
///
/// Parsers must be Send + Sync to enable parallel parsing in worker pool.
/// Avoid interior mutability unless using atomic types or proper synchronization.
#[async_trait]
pub trait MarkdownParser: Send + Sync {
    /// Parse a markdown file from the filesystem
    ///
    /// This is the primary entry point for the parsing pipeline. It should:
    /// 1. Read the file contents
    /// 2. Validate file size against limits
    /// 3. Parse the content
    /// 4. Return a fully populated `ParsedDocument`
    ///
    /// # Errors
    ///
    /// Returns `ParserError` if:
    /// - File cannot be read (IO error)
    /// - File exceeds size limit
    /// - Content is not valid UTF-8
    /// - Parsing fails critically (invalid frontmatter structure)
    ///
    /// # Performance
    ///
    /// This method performs blocking IO and should be called from
    /// `tokio::task::spawn_blocking` in the pipeline.
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedDocument>;

    /// Parse markdown content from a string
    ///
    /// This method performs the actual parsing logic. It should be synchronous
    /// (not async) since parsing is CPU-bound, not IO-bound.
    ///
    /// # Arguments
    ///
    /// - `content`: The raw markdown content
    /// - `source_path`: The original file path (for metadata only)
    ///
    /// # Errors
    ///
    /// Returns `ParserError` if parsing fails. Note that malformed frontmatter
    /// should not be fatal - the parser should continue and report the error
    /// in the result.
    fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedDocument>;

    /// Get parser capabilities and configuration
    ///
    /// This method returns metadata about what features this parser supports
    /// and its configuration limits.
    fn capabilities(&self) -> ParserCapabilities;

    /// Validate if the parser can handle this file
    ///
    /// Quick validation before parsing. Checks file extension, size, etc.
    fn can_parse(&self, path: &Path) -> bool {
        // Default implementation: check extension
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "md" | "markdown"))
            .unwrap_or(false)
    }
}

/// Parser capabilities and configuration limits
///
/// Describes what features a parser implementation supports and its
/// operational limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserCapabilities {
    /// Parser implementation name
    pub name: &'static str,

    /// Parser version
    pub version: &'static str,

    /// Supports YAML frontmatter parsing
    pub yaml_frontmatter: bool,

    /// Supports TOML frontmatter parsing
    pub toml_frontmatter: bool,

    /// Supports wikilink extraction [[note]]
    pub wikilinks: bool,

    /// Supports tag extraction #tag
    pub tags: bool,

    /// Supports heading extraction
    pub headings: bool,

    /// Supports code block extraction
    pub code_blocks: bool,

    /// Supports full content parsing (plain text)
    pub full_content: bool,

    /// Maximum file size in bytes (None = no limit)
    pub max_file_size: Option<usize>,

    /// Supported file extensions
    pub extensions: Vec<&'static str>,
}

impl ParserCapabilities {
    /// Create capabilities for a full-featured parser
    pub fn full() -> Self {
        Self {
            name: "unknown",
            version: "0.0.0",
            yaml_frontmatter: true,
            toml_frontmatter: true,
            wikilinks: true,
            tags: true,
            headings: true,
            code_blocks: true,
            full_content: true,
            max_file_size: Some(10 * 1024 * 1024), // 10 MB default
            extensions: vec!["md", "markdown"],
        }
    }

    /// Create capabilities for a minimal parser
    pub fn minimal() -> Self {
        Self {
            name: "unknown",
            version: "0.0.0",
            yaml_frontmatter: false,
            toml_frontmatter: false,
            wikilinks: true,
            tags: true,
            headings: false,
            code_blocks: false,
            full_content: true,
            max_file_size: Some(1024 * 1024), // 1 MB
            extensions: vec!["md"],
        }
    }

    /// Check if all required features are supported
    pub fn supports_all(&self, requirements: &ParserRequirements) -> bool {
        (!requirements.yaml_frontmatter || self.yaml_frontmatter)
            && (!requirements.toml_frontmatter || self.toml_frontmatter)
            && (!requirements.wikilinks || self.wikilinks)
            && (!requirements.tags || self.tags)
            && (!requirements.headings || self.headings)
            && (!requirements.code_blocks || self.code_blocks)
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
