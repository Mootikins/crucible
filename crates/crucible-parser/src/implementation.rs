//! Concrete implementation of the MarkdownParser trait

use async_trait::async_trait;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::block_extractor::{BlockExtractor, ExtractionConfig};
use crate::block_hasher::SimpleBlockHasher;
use crate::error::{ParserError, ParserResult};
use crate::extensions::ExtensionRegistry;
use crate::traits::{MarkdownParser, ParserCapabilities};
use crate::types::{
    Callout, FootnoteMap, LatexExpression, NoteContent, ParsedNote, ParsedNoteMetadata,
};

/// Default implementation of the MarkdownParser trait
///
/// This parser supports:
/// - Obsidian-compatible wikilinks and transclusions
/// - Frontmatter parsing (YAML/TOML)
/// - LaTeX mathematical expressions
/// - Callout blocks
/// - Extensible plugin architecture
pub struct DefaultMarkdownParser {
    /// Extensions registry for this parser
    #[allow(dead_code)]
    extensions: ExtensionRegistry,
    /// Block processing configuration
    pub block_config: BlockProcessingConfig,
}

/// Configuration for block-level processing
#[derive(Debug, Clone, Default)]
pub struct BlockProcessingConfig {
    /// Whether to enable block-level processing (Phase 2 optimize-data-flow)
    pub enabled: bool,
    /// Configuration for block extraction
    pub extraction_config: ExtractionConfig,
}

impl BlockProcessingConfig {
    /// Create a new block processing configuration
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            extraction_config: ExtractionConfig::default(),
        }
    }

    /// Create a new block processing configuration with custom extraction settings
    pub fn with_extraction_config(enabled: bool, extraction_config: ExtractionConfig) -> Self {
        Self {
            enabled,
            extraction_config,
        }
    }

    /// Enable block processing
    pub fn enabled() -> Self {
        Self::new(true)
    }

    /// Disable block processing
    pub fn disabled() -> Self {
        Self::new(false)
    }
}

/// Default implementation of the MarkdownParser trait
///
/// This parser supports:
/// - Obsidian-compatible wikilinks and transclusions
/// - Frontmatter parsing (YAML/TOML)
/// - LaTeX mathematical expressions
/// - Callout blocks
/// - Extensible plugin architecture
/// - Block-level processing with hash generation (Phase 2 optimize-data-flow)
#[derive(Debug, Clone)]
pub struct CrucibleParser {
    /// Extension registry for syntax extensions
    extensions: ExtensionRegistry,
    /// Maximum file size limit
    max_file_size: Option<usize>,
    /// Block processing configuration
    block_config: BlockProcessingConfig,
}

impl CrucibleParser {
    /// Create a new parser with default extensions (block processing disabled)
    pub fn new() -> Self {
        Self::with_default_extensions()
    }

    /// Create a parser with custom extension registry
    pub fn with_extensions(extensions: ExtensionRegistry) -> Self {
        Self {
            extensions,
            max_file_size: Some(10 * 1024 * 1024),
            block_config: BlockProcessingConfig::default(),
        }
    }

    /// Create a parser with default extensions (LaTeX, callouts, enhanced tags, and footnotes)
    pub fn with_default_extensions() -> Self {
        let mut builder = crate::ExtensionRegistryBuilder::new();

        // Add basic markdown extension based on parser feature
        #[cfg(feature = "markdown-it-parser")]
        {
            builder = builder.with_extension(crate::create_basic_markdown_it_extension());
        }

        let builder = builder
            .with_extension(crate::create_wikilink_extension()) // Wikilinks [[note]]
            .with_extension(crate::create_inline_link_extension()) // Inline links [text](url)
            .with_extension(crate::create_latex_extension())
            .with_extension(crate::create_callout_extension())
            .with_extension(crate::create_blockquote_extension())
            .with_extension(crate::create_enhanced_tags_extension())
            .with_extension(crate::create_footnote_extension());

        let extensions = builder.build();

        Self {
            extensions,
            max_file_size: Some(10 * 1024 * 1024),
            block_config: BlockProcessingConfig::default(),
        }
    }

    /// Create a parser with block processing enabled (Phase 2 optimize-data-flow)
    pub fn with_block_processing() -> Self {
        let mut parser = Self::with_default_extensions();
        parser.block_config = BlockProcessingConfig::enabled();
        parser
    }

    /// Create a parser with custom block processing configuration
    pub fn with_block_config(block_config: BlockProcessingConfig) -> Self {
        let mut parser = Self::with_default_extensions();
        parser.block_config = block_config;
        parser
    }

    /// Set maximum file size limit
    pub fn with_max_file_size(mut self, max_size: usize) -> Self {
        self.max_file_size = Some(max_size);
        self
    }

    /// Enable or disable block processing
    pub fn with_block_processing_enabled(mut self, enabled: bool) -> Self {
        self.block_config.enabled = enabled;
        self
    }

    /// Set block extraction configuration
    pub fn with_extraction_config(mut self, extraction_config: ExtractionConfig) -> Self {
        self.block_config.extraction_config = extraction_config;
        self
    }

    /// Check if block processing is enabled
    pub fn is_block_processing_enabled(&self) -> bool {
        self.block_config.enabled
    }

    /// Get the current block processing configuration
    pub fn block_config(&self) -> &BlockProcessingConfig {
        &self.block_config
    }

    /// Process blocks for Phase 2 optimize-data-flow
    ///
    /// This method extracts AST blocks from the parsed note, computes their hashes,
    /// and builds a Merkle tree for efficient change detection.
    ///
    /// # Arguments
    ///
    /// * `note` - The parsed note to process (modified in place)
    ///
    /// # Returns
    ///
    /// Ok(()) if processing succeeded, Err(ParserError) if processing failed
    async fn process_blocks(&self, note: &mut ParsedNote) -> ParserResult<()> {
        // Create block extractor with configured settings
        let extractor = BlockExtractor::with_config(self.block_config.extraction_config.clone());

        // Extract AST blocks from the note
        let blocks = extractor
            .extract_blocks(note)
            .map_err(|e| ParserError::parse_failed(format!("Block extraction failed: {:?}", e)))?;

        // If no blocks were extracted, clear hash data and return
        if blocks.is_empty() {
            note.clear_hash_data();
            return Ok(());
        }

        // Compute block hashes using SimpleBlockHasher
        let block_hashes = self
            .compute_block_hashes(&blocks)
            .await
            .map_err(|e| ParserError::parse_failed(format!("Block hashing failed: {}", e)))?;

        // Build Merkle tree and compute root hash
        let merkle_root = self.compute_merkle_root(&blocks).await.map_err(|e| {
            ParserError::parse_failed(format!("Merkle tree construction failed: {}", e))
        })?;

        // Update note with hash data
        note.block_hashes = block_hashes;
        note.merkle_root = Some(merkle_root);

        Ok(())
    }

    /// Compute hashes for a collection of AST blocks
    ///
    /// # Arguments
    ///
    /// * `blocks` - The AST blocks to hash
    ///
    /// # Returns
    ///
    /// Vector of block hashes or error if hashing failed
    async fn compute_block_hashes(
        &self,
        blocks: &[crate::types::ASTBlock],
    ) -> Result<Vec<crate::types::BlockHash>, Box<dyn std::error::Error + Send + Sync>> {
        let hasher = SimpleBlockHasher::new();
        let hashes = hasher.hash_blocks_batch(blocks).await?;
        Ok(hashes)
    }

    /// Compute Merkle root hash for a collection of AST blocks
    ///
    /// # Arguments
    ///
    /// * `blocks` - The AST blocks to process
    ///
    /// # Returns
    ///
    /// Merkle root hash or error if computation failed
    async fn compute_merkle_root(
        &self,
        blocks: &[crate::types::ASTBlock],
    ) -> Result<crate::types::BlockHash, Box<dyn std::error::Error + Send + Sync>> {
        let hasher = SimpleBlockHasher::new();
        let merkle_root = hasher.build_merkle_root(blocks).await?;
        Ok(merkle_root)
    }

    /// Parse frontmatter from content
    fn parse_frontmatter<'a>(
        &self,
        content: &'a str,
    ) -> (Option<String>, &'a str, crate::types::FrontmatterFormat) {
        // Check for YAML frontmatter
        if content.starts_with("---\n") || content.starts_with("---\r\n") {
            let start = if content.starts_with("---\r\n") { 5 } else { 4 };
            if let Some(end) = content
                .find("\n---\n")
                .or_else(|| content.find("\n---\r\n"))
            {
                let frontmatter = &content[start..end];
                let after = &content[end..];
                let skip = if after.starts_with("\n---\r\n") { 6 } else { 5 };
                let content = &content[end + skip..];
                return (
                    Some(frontmatter.to_string()),
                    content,
                    crate::types::FrontmatterFormat::Yaml,
                );
            }
            if content[start..].trim_end() == "---" || content.ends_with("\n---") {
                let end = content.len()
                    - if content.ends_with("\r\n---") {
                        5
                    } else if content.ends_with("\n---") {
                        4
                    } else {
                        3
                    };
                let frontmatter = &content[start..end];
                return (
                    Some(frontmatter.to_string()),
                    "",
                    crate::types::FrontmatterFormat::Yaml,
                );
            }
        }

        // Check for TOML frontmatter
        if content.starts_with("+++\n") || content.starts_with("+++\r\n") {
            let start = if content.starts_with("+++\r\n") { 5 } else { 4 };
            if let Some(end) = content
                .find("\n+++\n")
                .or_else(|| content.find("\n+++\r\n"))
            {
                let frontmatter = &content[start..end];
                let after = &content[end..];
                let skip = if after.starts_with("\n+++\r\n") { 6 } else { 5 };
                let content = &content[end + skip..];
                return (
                    Some(frontmatter.to_string()),
                    content,
                    crate::types::FrontmatterFormat::Toml,
                );
            }
            if content[start..].trim_end() == "+++" || content.ends_with("\n+++") {
                let end = content.len()
                    - if content.ends_with("\r\n+++") {
                        5
                    } else if content.ends_with("\n+++") {
                        4
                    } else {
                        3
                    };
                let frontmatter = &content[start..end];
                return (
                    Some(frontmatter.to_string()),
                    "",
                    crate::types::FrontmatterFormat::Toml,
                );
            }
        }

        (None, content, crate::types::FrontmatterFormat::None)
    }

    /// Validate file size against limits
    fn validate_file_size(&self, size: usize) -> ParserResult<()> {
        if let Some(max_size) = self.max_file_size {
            if size > max_size {
                return Err(ParserError::FileTooLarge {
                    size,
                    max: max_size,
                });
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
impl MarkdownParser for CrucibleParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
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

    async fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        // Parse frontmatter
        let (frontmatter_raw, content, frontmatter_format) = self.parse_frontmatter(content);

        // Parse frontmatter into Frontmatter struct if present
        let frontmatter = frontmatter_raw
            .map(|fm_raw| crate::types::Frontmatter::new(fm_raw, frontmatter_format));

        // Create initial note content
        let mut document_content = NoteContent {
            plain_text: content.to_string(),
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            headings: Vec::new(),
            code_blocks: Vec::new(),
            paragraphs: Vec::new(),
            lists: Vec::new(),
            inline_links: Vec::new(),
            wikilinks: Vec::new(),
            tags: Vec::new(),
            latex_expressions: Vec::new(),
            callouts: Vec::new(),
            blockquotes: Vec::new(),
            footnotes: crate::types::FootnoteMap::new(),
            tables: Vec::new(),
            horizontal_rules: Vec::new(),
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
                        eprintln!(
                            "Parse error in {:?} [{}:{}] (offset {}): {}",
                            source_path.file_name().unwrap_or_default(),
                            error.line,
                            error.column,
                            error.offset,
                            error.message
                        );
                    }
                }
            }
        }

        // Extract top-level fields from document_content before building
        let callouts = document_content.callouts.clone();
        let latex_expressions = document_content.latex_expressions.clone();
        let footnotes = document_content.footnotes.clone();
        let wikilinks = document_content.wikilinks.clone();
        let tags = document_content.tags.clone();
        let inline_links = document_content.inline_links.clone();

        // Extract structural metadata from parsed content
        let metadata =
            Self::extract_metadata(&document_content, &callouts, &latex_expressions, &footnotes);

        // Create the initial parsed note using builder pattern
        let mut parsed_doc = ParsedNote::builder(source_path.to_path_buf())
            .with_frontmatter(frontmatter)
            .with_content(document_content)
            .with_wikilinks(wikilinks)
            .with_tags(tags)
            .with_inline_links(inline_links)
            .with_callouts(callouts)
            .with_latex_expressions(latex_expressions)
            .with_footnotes(footnotes)
            .with_metadata(metadata)
            .build();

        // Apply block-level processing if enabled (Phase 2 optimize-data-flow)
        if self.block_config.enabled {
            if let Err(e) = self.process_blocks(&mut parsed_doc).await {
                // Log error but don't fail parsing - block processing is optional
                eprintln!("Block processing error: {}", e);
            }
        }

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
            tables: true,
            callouts: true,
            latex_expressions: true,
            footnotes: true,
            blockquotes: true,
            horizontal_rules: true,
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

// Helper methods for CrucibleParser (not part of trait)
impl CrucibleParser {
    /// Extract structural metadata from parsed content
    ///
    /// Computes deterministic counts from AST structure:
    /// - Word/character counts
    /// - Element counts (headings, code blocks, lists, etc.)
    ///
    /// This follows industry standard pattern (Unified/Remark, Pandoc, Elasticsearch)
    /// where structural metadata is extracted during parsing, while computed metadata
    /// (complexity, reading time) is added during enrichment.
    fn extract_metadata(
        content: &NoteContent,
        callouts: &[Callout],
        latex: &[LatexExpression],
        footnotes: &FootnoteMap,
    ) -> ParsedNoteMetadata {
        ParsedNoteMetadata {
            word_count: content.word_count,
            char_count: content.char_count,
            heading_count: content.headings.len(),
            code_block_count: content.code_blocks.len(),
            list_count: content.lists.len(),
            paragraph_count: content.paragraphs.len(),
            callout_count: callouts.len(),
            latex_count: latex.len(),
            footnote_count: footnotes.definitions.len(),
        }
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
        // When no frontmatter title is present, title() returns the filename without extension
        assert_eq!(doc.title(), "test");
        assert_eq!(doc.content.word_count, 7);
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
        let (fm, content, format) = parser.parse_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(content, "Content");
        assert_eq!(format, crate::types::FrontmatterFormat::Yaml);

        // TOML frontmatter
        let content = "+++\ntitle = \"Test\"\n+++\nContent";
        let (fm, content, format) = parser.parse_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(content, "Content");
        assert_eq!(format, crate::types::FrontmatterFormat::Toml);

        // No frontmatter
        let content = "Just content";
        let (fm, content, format) = parser.parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(content, "Just content");
        assert_eq!(format, crate::types::FrontmatterFormat::None);
    }

    #[test]
    fn test_block_processing_config() {
        // Test default configuration (disabled)
        let config = BlockProcessingConfig::default();
        assert!(!config.enabled);

        // Test enabled configuration
        let enabled_config = BlockProcessingConfig::enabled();
        assert!(enabled_config.enabled);

        // Test disabled configuration
        let disabled_config = BlockProcessingConfig::disabled();
        assert!(!disabled_config.enabled);

        // Test custom configuration
        let extraction_config = ExtractionConfig {
            min_paragraph_length: 50,
            preserve_empty_blocks: true,
            merge_consecutive_paragraphs: true,
        };
        let custom_config = BlockProcessingConfig::with_extraction_config(true, extraction_config);
        assert!(custom_config.enabled);
        assert_eq!(custom_config.extraction_config.min_paragraph_length, 50);
    }

    #[test]
    fn test_parser_with_block_processing() {
        // Test default parser has block processing disabled
        let default_parser = CrucibleParser::new();
        assert!(!default_parser.is_block_processing_enabled());

        // Test parser with block processing enabled
        let block_parser = CrucibleParser::with_block_processing();
        assert!(block_parser.is_block_processing_enabled());

        // Test parser with custom block config
        let custom_config = BlockProcessingConfig::enabled();
        let custom_parser = CrucibleParser::with_block_config(custom_config);
        assert!(custom_parser.is_block_processing_enabled());

        // Test chaining configuration
        let chain_parser = CrucibleParser::new()
            .with_block_processing_enabled(true)
            .with_max_file_size(1024);
        assert!(chain_parser.is_block_processing_enabled());
        assert_eq!(chain_parser.max_file_size, Some(1024));
    }

    #[tokio::test]
    async fn test_parse_content_with_block_processing_disabled() {
        let content = r#"# Test Note

This is a paragraph.

## Section 2

```rust
let x = 42;
```

- Item 1
- Item 2

> [!NOTE]
> This is a callout"#;

        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::new(); // Block processing disabled by default

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        // No frontmatter title, so title() returns the filename without extension
        assert_eq!(doc.title(), "test");

        // Should have no block hashes when disabled
        assert!(!doc.has_block_hashes());
        assert_eq!(doc.block_hash_count(), 0);
        assert!(!doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), None);
    }

    #[tokio::test]
    async fn test_parse_content_with_block_processing_enabled() {
        let content = r#"# Test Note

This is a paragraph.

## Section 2

```rust
let x = 42;
```

- Item 1
- Item 2"#;

        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::with_block_processing();

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        // No frontmatter title, so title() returns the filename without extension
        assert_eq!(doc.title(), "test");

        // Should have block hashes when enabled
        assert!(doc.has_block_hashes());
        assert!(doc.block_hash_count() > 0);
        assert!(doc.has_merkle_root());
        assert!(doc.get_merkle_root().is_some());

        // Verify hashes are non-zero
        for hash in &doc.block_hashes {
            assert!(!hash.is_zero());
        }

        assert!(!doc.get_merkle_root().unwrap().is_zero());
    }

    #[tokio::test]
    async fn test_block_processing_error_handling() {
        let content = "# Minimal Content";
        let path = PathBuf::from("test.md");

        // Create parser with block processing enabled
        let parser = CrucibleParser::with_block_processing();

        // Should still succeed even if block processing has issues
        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        // No frontmatter title, so title() returns the filename without extension
        assert_eq!(doc.title(), "test");
        // Note should still be valid even if block processing fails
    }

    #[tokio::test]
    async fn test_empty_document_block_processing() {
        let content = "";
        let path = PathBuf::from("empty.md");
        let parser = CrucibleParser::with_block_processing();

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        // Empty documents should have no block hashes
        assert!(!doc.has_block_hashes());
        assert!(!doc.has_merkle_root());
    }

    #[test]
    fn test_backward_compatibility() {
        // Test that existing code still works without modifications
        let parser = CrucibleParser::new();
        let parser_with_extensions = CrucibleParser::with_default_extensions();
        let parser_with_custom =
            CrucibleParser::with_extensions(crate::ExtensionRegistryBuilder::new().build());

        // All should have block processing disabled by default
        assert!(!parser.is_block_processing_enabled());
        assert!(!parser_with_extensions.is_block_processing_enabled());
        assert!(!parser_with_custom.is_block_processing_enabled());

        // Test that capabilities remain unchanged
        let caps = parser.capabilities();
        assert_eq!(caps.name, "crucible-parser");
        assert!(caps.yaml_frontmatter);
        assert!(caps.wikilinks);
    }

    #[tokio::test]
    async fn test_deterministic_block_hashes() {
        let content = r#"# Test Title

Same content.

```rust
let x = 42;
```"#;

        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::with_block_processing();

        // Parse the same content twice
        let result1 = parser.parse_content(content, &path).await.unwrap();
        let result2 = parser.parse_content(content, &path).await.unwrap();

        // Should produce identical hashes
        assert_eq!(result1.block_hashes, result2.block_hashes);
        assert_eq!(result1.merkle_root, result2.merkle_root);

        // Both should have hashes
        assert!(result1.has_block_hashes());
        assert!(result1.has_merkle_root());
        assert!(result2.has_block_hashes());
        assert!(result2.has_merkle_root());
    }

    #[tokio::test]
    async fn test_different_content_different_hashes() {
        let content1 = r#"# Title 1

Content 1."#;

        let content2 = r#"# Title 2

Content 2."#;

        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::with_block_processing();

        let result1 = parser.parse_content(content1, &path).await.unwrap();
        let result2 = parser.parse_content(content2, &path).await.unwrap();

        // Should produce different hashes
        assert_ne!(result1.block_hashes, result2.block_hashes);
        assert_ne!(result1.merkle_root, result2.merkle_root);

        // But both should have hashes
        assert!(result1.has_block_hashes());
        assert!(result1.has_merkle_root());
        assert!(result2.has_block_hashes());
        assert!(result2.has_merkle_root());
    }

    #[tokio::test]
    async fn test_large_document_block_processing() {
        // Create a larger note to test performance and stability
        let mut content = String::from("# Large Note\n\n");

        for i in 0..20 {
            content.push_str(&format!("## Section {}\n\n", i + 1));
            content.push_str("This is a paragraph with some content.\n\n");

            if i % 3 == 0 {
                content.push_str(&format!(
                    "```rust\nfn test_{}() {{\n    println!(\"test\");\n}}\n```\n\n",
                    i
                ));
            }

            if i % 2 == 0 {
                content.push_str("- List item 1\n- List item 2\n- List item 3\n\n");
            }
        }

        let path = PathBuf::from("large.md");
        let parser = CrucibleParser::with_block_processing();

        let start = std::time::Instant::now();
        let result = parser.parse_content(&content, &path).await;
        let duration = start.elapsed();

        assert!(result.is_ok());

        let doc = result.unwrap();
        // No frontmatter title, so title() returns the filename without extension
        assert_eq!(doc.title(), "large");
        assert!(doc.has_block_hashes());
        assert!(doc.has_merkle_root());

        // Should have processed multiple blocks
        assert!(doc.block_hash_count() > 10);

        // Performance check: should complete within reasonable time
        assert!(
            duration.as_secs() < 5,
            "Block processing took too long: {:?}",
            duration
        );

        println!(
            "Processed {} blocks in {:?}",
            doc.block_hash_count(),
            duration
        );
    }
}
