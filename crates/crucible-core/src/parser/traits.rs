//! Traits for markdown parsing
//!
//! This module re-exports the canonical MarkdownParser trait from crucible-parser.
//! Since crucible-core depends on crucible-parser (not vice versa), we re-export
//! to avoid duplication while maintaining API compatibility.

// Re-export the canonical traits from crucible-parser
pub use crucible_parser::traits::{MarkdownParser, ParserCapabilities};

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

// Helper methods for ParserCapabilities (can't impl on foreign type)
pub trait ParserCapabilitiesExt {
    fn supports_all(&self, requirements: &ParserRequirements) -> bool;
    fn minimal() -> ParserCapabilities;
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

    /// Create capabilities for a minimal parser
    fn minimal() -> ParserCapabilities {
        ParserCapabilities {
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
