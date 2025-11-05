//! Concrete implementation of the MarkdownParser trait

use async_trait::async_trait;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::error::{ParserError, ParserResult};
use crate::extensions::ExtensionRegistry;
use crate::traits::{MarkdownParserImplementation, ParserCapabilities};
use crate::types::{DocumentContent, ParsedDocument};

/// Default implementation of the MarkdownParserImplementation trait
///
/// This parser supports:
/// - Obsidian-compatible wikilinks and transclusions
/// - Frontmatter parsing (YAML/TOML)
/// - LaTeX mathematical expressions
/// - Callout blocks
/// - Extensible plugin architecture
#[derive(Debug, Clone)]
pub struct CrucibleParser {
    /// Extension registry for syntax extensions
    extensions: ExtensionRegistry,
    /// Maximum file size limit
    max_file_size: Option<usize>,
}

impl CrucibleParser {
    /// Create a new parser with default extensions
    pub fn new() -> Self {
        Self::with_default_extensions()
    }

    /// Create a parser with custom extension registry
    pub fn with_extensions(extensions: ExtensionRegistry) -> Self {
        Self {
            extensions,
            max_file_size: Some(10 * 1024 * 1024),
        }
    }

    /// Create a parser with default extensions (LaTeX, callouts, enhanced tags, and footnotes)
    pub fn with_default_extensions() -> Self {
        let builder = crate::ExtensionRegistryBuilder::new()
            .with_extension(crate::create_latex_extension())
            .with_extension(crate::create_callout_extension())
            .with_extension(crate::create_enhanced_tags_extension())
            .with_extension(crate::create_footnote_extension());

        let extensions = builder.build();

        Self {
            extensions,
            max_file_size: Some(10 * 1024 * 1024),
        }
    }

    /// Set maximum file size limit
    pub fn with_max_file_size(mut self, max_size: usize) -> Self {
        self.max_file_size = Some(max_size);
        self
    }

    /// Parse frontmatter from content
    fn parse_frontmatter<'a>(&self, content: &'a str) -> (Option<String>, &'a str) {
        // Check for YAML frontmatter
        if content.starts_with("---\n") {
            if let Some(end) = content.find("\n---\n") {
                let frontmatter = &content[4..end];
                let content = &content[end + 5..];
                return (Some(frontmatter.to_string()), content);
            }
        }

        // Check for TOML frontmatter
        if content.starts_with("+++\n") {
            if let Some(end) = content.find("\n+++\n") {
                let frontmatter = &content[4..end];
                let content = &content[end + 5..];
                return (Some(frontmatter.to_string()), content);
            }
        }

        (None, content)
    }

    /// Validate file size against limits
    fn validate_file_size(&self, size: usize) -> ParserResult<()> {
        if let Some(max_size) = self.max_file_size {
            if size > max_size {
                return Err(ParserError::FileTooLarge { size, max: max_size });
            }
        }
        Ok(())
    }
}

impl Default for CrucibleParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarkdownParserImplementation for CrucibleParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedDocument> {
        // Read file contents
        let mut file = fs::File::open(path).await.map_err(ParserError::Io)?;

        let mut content = String::new();
        let size = file
            .read_to_string(&mut content)
            .await
            .map_err(ParserError::Io)?;

        // Validate file size
        self.validate_file_size(size)?;

        // Parse content
        self.parse_content(&content, path).await
    }

    async fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedDocument> {
        // Parse frontmatter
        let (_frontmatter_raw, content) = self.parse_frontmatter(content);

        // Create initial document content
        let mut document_content = DocumentContent {
            plain_text: content.to_string(),
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            headings: Vec::new(),
            code_blocks: Vec::new(),
            paragraphs: Vec::new(),
            lists: Vec::new(),
            latex_expressions: Vec::new(),
            callouts: Vec::new(),
            footnotes: crate::types::FootnoteMap::new(),
        };

        // Apply syntax extensions
        for extension in self.extensions.enabled_extensions() {
            if extension.can_handle(content) {
                let errors = extension.parse(content, &mut document_content).await;

                // For now, we'll log errors but not fail parsing
                // In a production system, we might want to collect these
                if !errors.is_empty() {
                    // Log errors but continue parsing
                    for error in errors {
                        eprintln!("Parse error: {}", error.message);
                    }
                }
            }
        }

        // Create the parsed document using builder pattern
        let parsed_doc = ParsedDocument::builder(source_path.to_path_buf())
            .with_content(document_content)
            .build();

        Ok(parsed_doc)
    }

    fn capabilities(&self) -> ParserCapabilities {
        ParserCapabilities {
            name: "crucible-parser",
            version: env!("CARGO_PKG_VERSION"),
            yaml_frontmatter: true,
            toml_frontmatter: true,
            wikilinks: true,
            tags: true,
            headings: true,
            code_blocks: true,
            full_content: true,
            max_file_size: self.max_file_size,
            extensions: vec!["md", "markdown"],
        }
    }

    fn can_parse(&self, path: &Path) -> bool {
        // Use default implementation from trait
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "md" | "markdown"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[tokio::test]
    async fn test_parse_basic_content() {
        let content = "# Test Note\n\nThis is a test.";
        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::new();

        let result = parser.parse_content(content, &path).await;

        assert!(result.is_ok());
        let doc = result.unwrap();
        assert_eq!(doc.title(), "Test Note");
        assert_eq!(doc.content.word_count, 4);
    }

    #[tokio::test]
    async fn test_parse_content_with_frontmatter() {
        let content = "---\ntitle: Test Note\ntags: [test]\n---\n# Content\n\nTest content.";
        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::new();

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert_eq!(doc.title(), "Test Note");
        assert!(doc.frontmatter.is_some());
    }

    #[test]
    fn test_capabilities() {
        let parser = CrucibleParser::new();
        let caps = parser.capabilities();

        assert_eq!(caps.name, "crucible-parser");
        assert!(caps.yaml_frontmatter);
        assert!(caps.wikilinks);
        assert!(caps.tags);
        assert!(caps.max_file_size.is_some());
    }

    #[test]
    fn test_can_parse() {
        let parser = CrucibleParser::new();

        assert!(parser.can_parse(Path::new("test.md")));
        assert!(parser.can_parse(Path::new("test.markdown")));
        assert!(!parser.can_parse(Path::new("test.txt")));
        assert!(!parser.can_parse(Path::new("test")));
    }

    #[test]
    fn test_file_size_validation() {
        let parser = CrucibleParser::new().with_max_file_size(10);

        // Small file should pass
        assert!(parser.validate_file_size(5).is_ok());

        // Large file should fail
        assert!(parser.validate_file_size(15).is_err());
    }

    #[test]
    fn test_parse_frontmatter() {
        let parser = CrucibleParser::new();

        // YAML frontmatter
        let content = "---\ntitle: Test\n---\nContent";
        let (fm, content) = parser.parse_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(content, "Content");

        // TOML frontmatter
        let content = "+++\ntitle = \"Test\"\n+++\nContent";
        let (fm, content) = parser.parse_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(content, "Content");

        // No frontmatter
        let content = "Just content";
        let (fm, content) = parser.parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(content, "Just content");
    }
}