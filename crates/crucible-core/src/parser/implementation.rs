//! The markdown parser.

use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

use super::error::ParseErrorType;
use super::error::{ParserError, ParserResult};
use super::extensions::ExtensionRegistry;
use super::traits::ParserCapabilities;
use super::types::{
    BlockKind, Callout, FootnoteMap, LatexExpression, NoteContent, ParseError, ParsedNote,
    ParsedNoteMetadata,
};

/// The markdown parser.
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

    /// Create a parser with every extension that the crate compiles.
    pub fn with_default_extensions() -> Self {
        Self::with_extensions(ExtensionRegistry::with_defaults())
    }

    /// Set maximum file size limit
    pub fn with_max_file_size(mut self, max_size: usize) -> Self {
        self.max_file_size = Some(max_size);
        self
    }

    /// Parse frontmatter from content
    fn parse_frontmatter<'a>(
        &self,
        content: &'a str,
    ) -> (Option<String>, &'a str, super::types::FrontmatterFormat) {
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
                    super::types::FrontmatterFormat::Yaml,
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
                    super::types::FrontmatterFormat::Yaml,
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
                    super::types::FrontmatterFormat::Toml,
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
                    super::types::FrontmatterFormat::Toml,
                );
            }
        }

        (None, content, super::types::FrontmatterFormat::None)
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

impl CrucibleParser {
    /// Parse a markdown file from the filesystem.
    pub async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
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

    /// Parse markdown content from a string.
    pub async fn parse_content(
        &self,
        content: &str,
        source_path: &Path,
    ) -> ParserResult<ParsedNote> {
        // Parse frontmatter. The body is always a SUFFIX slice of the input,
        // so the frontmatter's byte length (the body's file-absolute offset)
        // is the length delta — recorded as `body_offset` so consumers can
        // convert body-relative extension offsets to file positions.
        let original_len = content.len();

        // Hash the WHOLE input, before the frontmatter is split off. This is
        // the note's identity for change detection: `NoteRecord::content_hash`
        // is documented as "BLAKE3 content hash (32 bytes) for change
        // detection", and the plain-text and canvas paths already fill it.
        // Markdown did not, so `BlockHash::from_hex("")` failed and every
        // markdown note stored `BlockHash::zero()` — which is why the daemon
        // needed a separate in-memory map to know what it had already indexed,
        // and why every restart reindexed the whole kiln.
        //
        // Hashing before the split is what makes it comparable to the bytes on
        // disk: a change confined to the frontmatter still changes the file.
        let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        let (frontmatter_raw, content, frontmatter_format) = self.parse_frontmatter(content);
        let body_offset = original_len - content.len();

        let mut parse_errors = Vec::new();

        if let Some(frontmatter_text) = &frontmatter_raw {
            match frontmatter_format {
                super::types::FrontmatterFormat::Yaml => {
                    if let Err(error) = serde_yaml::from_str::<serde_yaml::Value>(frontmatter_text)
                    {
                        let (line, column) = error
                            .location()
                            .map(|location| {
                                (
                                    location.line().saturating_sub(1),
                                    location.column().saturating_sub(1),
                                )
                            })
                            .unwrap_or((0, 0));

                        parse_errors.push(ParseError::warning(
                            format!("Failed to parse YAML frontmatter: {error}"),
                            ParseErrorType::FrontmatterSyntax,
                            line,
                            column,
                            0,
                        ));
                    }
                }
                super::types::FrontmatterFormat::Toml => {
                    if let Err(error) = toml::from_str::<toml::Value>(frontmatter_text) {
                        parse_errors.push(ParseError::warning(
                            format!("Failed to parse TOML frontmatter: {error}"),
                            ParseErrorType::FrontmatterSyntax,
                            0,
                            0,
                            0,
                        ));
                    }
                }
                super::types::FrontmatterFormat::None => {}
            }
        }

        // Parse frontmatter into Frontmatter struct if present
        let frontmatter = frontmatter_raw
            .map(|fm_raw| super::types::Frontmatter::new(fm_raw, frontmatter_format));

        // Create initial note content
        let mut document_content = NoteContent {
            plain_text: content.to_string(),
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            blocks: Vec::new(),
            inline_links: Vec::new(),
            wikilinks: Vec::new(),
            tags: Vec::new(),
            latex_expressions: Vec::new(),
            callouts: Vec::new(),
            footnotes: super::types::FootnoteMap::new(),
        };

        parse_errors.extend(self.extensions.apply(content, &mut document_content));

        // The extensions fill the lists in `document_content`. The note owns
        // them from here on, so move them out. The content copies stay empty;
        // a reader of `content.wikilinks` sees nothing, which makes the one
        // source of truth visible in a test.
        let callouts = std::mem::take(&mut document_content.callouts);
        let latex_expressions = std::mem::take(&mut document_content.latex_expressions);
        let footnotes = std::mem::take(&mut document_content.footnotes);
        let wikilinks = std::mem::take(&mut document_content.wikilinks);
        let tags = std::mem::take(&mut document_content.tags);
        let inline_links = std::mem::take(&mut document_content.inline_links);

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
            .with_body_offset(body_offset)
            .with_content_hash(content_hash)
            .build();

        parsed_doc.parse_errors = parse_errors;

        Ok(parsed_doc)
    }

    /// Get parser capabilities.
    pub fn capabilities(&self) -> ParserCapabilities {
        ParserCapabilities {
            max_file_size: self.max_file_size,
            ..ParserCapabilities::full()
        }
    }

    /// Validate that the parser can handle this file.
    pub fn can_parse(&self, path: &Path) -> bool {
        crate::kiln::is_note_file(path)
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
        let count =
            |want: fn(&BlockKind) -> bool| content.blocks.iter().filter(|b| want(&b.kind)).count();

        ParsedNoteMetadata {
            word_count: content.word_count,
            char_count: content.char_count,
            heading_count: count(|k| matches!(k, BlockKind::Heading { .. })),
            code_block_count: count(|k| matches!(k, BlockKind::Code { .. })),
            list_count: count(|k| matches!(k, BlockKind::List { .. })),
            paragraph_count: count(|k| matches!(k, BlockKind::Paragraph)),
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

    /// The six link lists live on `ParsedNote`; `parse_content` moves them out
    /// of `NoteContent`. A reader of the content copy must see an empty list.
    #[tokio::test]
    async fn link_lists_live_on_the_note_not_in_content() {
        let parser = CrucibleParser::new();
        let content = "See [[other]] and [text](https://x.io)\n\n#tag\n\n\
            > [!note] Hi\n> body\n\nInline $x$ and a note[^1].\n\n[^1]: Foot.\n";
        let doc = parser
            .parse_content(content, &PathBuf::from("n.md"))
            .await
            .unwrap();

        assert_eq!(doc.wikilinks.len(), 1);
        assert_eq!(doc.inline_links.len(), 1);
        assert_eq!(doc.tags.len(), 1);
        assert_eq!(doc.callouts.len(), 1);
        assert_eq!(doc.latex_expressions.len(), 1);
        assert_eq!(doc.footnotes.definitions.len(), 1);

        assert!(doc.content.wikilinks.is_empty());
        assert!(doc.content.inline_links.is_empty());
        assert!(doc.content.tags.is_empty());
        assert!(doc.content.callouts.is_empty());
        assert!(doc.content.latex_expressions.is_empty());
        assert!(doc.content.footnotes.definitions.is_empty());
    }

    /// `content_hash` is BLAKE3 over the WHOLE input. It is what the daemon
    /// stores on the note row and compares the file against, so a note whose
    /// hash is empty or constant makes every pass look like the first.
    #[tokio::test]
    async fn content_hash_is_blake3_of_the_whole_input() {
        let parser = CrucibleParser::new();
        let content = "---\ntitle: T\n---\nbody\n";

        let parsed = parser
            .parse_content(content, &PathBuf::from("n.md"))
            .await
            .unwrap();

        assert_eq!(
            parsed.content_hash,
            blake3::hash(content.as_bytes()).to_hex().to_string()
        );
    }

    /// Hashing after the frontmatter split would make a frontmatter-only edit
    /// invisible, and the file would never be reindexed.
    #[tokio::test]
    async fn a_frontmatter_only_edit_changes_the_content_hash() {
        let parser = CrucibleParser::new();
        let path = PathBuf::from("n.md");

        let before = parser
            .parse_content("---\ntitle: A\n---\nbody\n", &path)
            .await
            .unwrap();
        let after = parser
            .parse_content("---\ntitle: B\n---\nbody\n", &path)
            .await
            .unwrap();

        assert_ne!(before.content_hash, after.content_hash);
    }

    /// `body_offset` converts body-relative wikilink spans to file-absolute
    /// bytes — the invariant the rename rewrite engine splices by.
    #[tokio::test]
    async fn test_body_offset_makes_spans_file_absolute() {
        let content = "---\ntitle: T\ntags: [x]\n---\nbody 🎉 [[Target|alias]] end";
        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::new();

        let doc = parser.parse_content(content, &path).await.unwrap();
        assert!(doc.body_offset > 0, "frontmatter present → nonzero offset");
        assert_eq!(doc.wikilinks.len(), 1);
        let (start, end) = doc.wikilinks[0].target_span;
        let abs = (doc.body_offset + start)..(doc.body_offset + end);
        assert_eq!(&content[abs], "Target");
    }

    /// No frontmatter → offset 0 and spans are already file-absolute.
    #[tokio::test]
    async fn test_body_offset_zero_without_frontmatter() {
        let content = "plain [[Link]]";
        let parser = CrucibleParser::new();
        let doc = parser
            .parse_content(content, &PathBuf::from("t.md"))
            .await
            .unwrap();
        assert_eq!(doc.body_offset, 0);
        let (start, end) = doc.wikilinks[0].target_span;
        assert_eq!(&content[start..end], "Link");
    }

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
        assert_eq!(format, crate::parser::types::FrontmatterFormat::Yaml);

        // TOML frontmatter
        let content = "+++\ntitle = \"Test\"\n+++\nContent";
        let (fm, content, format) = parser.parse_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(content, "Content");
        assert_eq!(format, crate::parser::types::FrontmatterFormat::Toml);

        // No frontmatter
        let content = "Just content";
        let (fm, content, format) = parser.parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(content, "Just content");
        assert_eq!(format, crate::parser::types::FrontmatterFormat::None);
    }

    #[tokio::test]
    async fn the_paragraph_count_counts_each_paragraph_once() {
        let content =
            "# Title\n\nAlpha has enough words here.\n\n- item one\n- item two\n\nBeta has enough words here.";
        let path = PathBuf::from("test.md");
        let parser = CrucibleParser::new();

        let doc = parser.parse_content(content, &path).await.unwrap();

        assert_eq!(doc.metadata.paragraph_count, 2);
        assert_eq!(doc.metadata.list_count, 1);
    }

    #[tokio::test]
    async fn the_structure_counts_come_from_the_ordered_block_list() {
        // Two headings, two paragraphs, one list, one fence. The counts must
        // match the blocks the parser emits, not a per-kind side list.
        let content = concat!(
            "# Title\n\n",
            "Alpha has enough words here.\n\n",
            "## Section\n\n",
            "- item one\n- item two\n\n",
            "```rust\nlet x = 42;\n```\n\n",
            "Beta has enough words here.\n"
        );
        let parser = CrucibleParser::new();

        let doc = parser
            .parse_content(content, &PathBuf::from("counts.md"))
            .await
            .unwrap();

        assert_eq!(doc.content.blocks.len(), 6);
        assert_eq!(doc.metadata.heading_count, 2);
        assert_eq!(doc.metadata.paragraph_count, 2);
        assert_eq!(doc.metadata.list_count, 1);
        assert_eq!(doc.metadata.code_block_count, 1);
    }
}
