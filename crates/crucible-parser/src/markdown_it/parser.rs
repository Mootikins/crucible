//! MarkdownIt-based parser implementation

use async_trait::async_trait;
use markdown_it::MarkdownIt;
use std::path::Path;

use crate::error::{ParserError, ParserResult};
use crate::traits::{MarkdownParserImplementation, ParserCapabilities};
use crate::types::{FrontmatterFormat, Frontmatter, ParsedNote};
use super::converter::AstConverter;
use super::plugins;

/// Parser implementation using markdown-it-rust
pub struct MarkdownItParser {
    md: MarkdownIt,
    capabilities: ParserCapabilities,
    max_file_size: Option<usize>,
}

impl MarkdownItParser {
    /// Create a new MarkdownItParser with default plugins
    pub fn new() -> Self {
        let mut md = MarkdownIt::new();

        // Add CommonMark support
        markdown_it::plugins::cmark::add(&mut md);

        // Add custom plugins for Obsidian-style syntax
        plugins::add_wikilink_plugin(&mut md);
        plugins::add_tag_plugin(&mut md);
        plugins::add_callout_plugin(&mut md);
        plugins::add_latex_plugin(&mut md);

        Self {
            md,
            capabilities: ParserCapabilities {
                name: "MarkdownItParser",
                version: env!("CARGO_PKG_VERSION"),
                yaml_frontmatter: false, // Not yet implemented in PoC
                toml_frontmatter: false,
                wikilinks: true,
                tags: true,      // ✅ Now implemented
                headings: true,
                code_blocks: true,
                full_content: true,
                max_file_size: Some(10 * 1024 * 1024),
                extensions: vec!["md", "markdown"],
            },
            max_file_size: Some(10 * 1024 * 1024),
        }
    }

    /// Create with custom max file size
    pub fn with_max_file_size(mut self, max_size: usize) -> Self {
        self.max_file_size = Some(max_size);
        self
    }

    /// Extract frontmatter from content (basic implementation)
    fn extract_frontmatter(content: &str) -> Option<Frontmatter> {
        // Simple frontmatter extraction for PoC
        // Full implementation would use markdown-it-front-matter plugin
        if let Some(rest) = content.strip_prefix("---\n") {
            if let Some(end_idx) = rest.find("\n---\n") {
                let yaml = &rest[..end_idx];
                return Some(Frontmatter::new(
                    yaml.to_string(),
                    FrontmatterFormat::Yaml,
                ));
            }
        }
        None
    }

    /// Hash content using BLAKE3
    fn hash_content(content: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(content.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

impl Default for MarkdownItParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarkdownParserImplementation for MarkdownItParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        // Read file content
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ParserError::Io(e))?;

        // Check file size limit
        if let Some(max_size) = self.max_file_size {
            if content.len() > max_size {
                return Err(ParserError::FileTooLarge {
                    size: content.len(),
                    max: max_size,
                });
            }
        }

        self.parse_content(&content, path).await
    }

    async fn parse_content(
        &self,
        content: &str,
        source_path: &Path,
    ) -> ParserResult<ParsedNote> {
        // Extract frontmatter (simple version for PoC)
        let frontmatter = Self::extract_frontmatter(content);

        // Get body content (skip frontmatter if present)
        let body = if let Some(rest) = content.strip_prefix("---\n") {
            if let Some(end_idx) = rest.find("\n---\n") {
                &rest[end_idx + 5..]
            } else {
                content
            }
        } else {
            content
        };

        // Parse with markdown-it
        let ast = self.md.parse(body);

        // Convert AST to NoteContent
        let note_content = AstConverter::convert(&ast)?;

        // Build ParsedNote
        let parsed_note = ParsedNote::builder(source_path.to_path_buf())
            .with_content(note_content)
            .with_frontmatter(frontmatter)
            .with_content_hash(Self::hash_content(content))
            .with_file_size(content.len() as u64)
            .build();

        Ok(parsed_note)
    }

    fn capabilities(&self) -> ParserCapabilities {
        self.capabilities.clone()
    }

    fn can_parse(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| matches!(ext, "md" | "markdown"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_parse_simple_content() {
        let parser = MarkdownItParser::new();
        let content = "# Hello World\n\nThis is a test.";
        let path = PathBuf::from("test.md");

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let note = result.unwrap();
        assert_eq!(note.content.headings.len(), 1);
        assert_eq!(note.content.headings[0].text, "Hello World");
    }

    #[tokio::test]
    async fn test_parse_wikilinks() {
        let parser = MarkdownItParser::new();
        let content = "Link to [[Other Note]] and [[Page|Alias]].";
        let path = PathBuf::from("test.md");

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let note = result.unwrap();
        assert_eq!(note.wikilinks.len(), 2);
        assert_eq!(note.wikilinks[0].target, "Other Note");
        assert_eq!(note.wikilinks[1].target, "Page");
        assert_eq!(note.wikilinks[1].alias, Some("Alias".to_string()));
    }

    #[tokio::test]
    async fn test_parse_frontmatter() {
        let parser = MarkdownItParser::new();
        let content = "---\ntitle: Test\n---\n\n# Content";
        let path = PathBuf::from("test.md");

        let result = parser.parse_content(content, &path).await;
        assert!(result.is_ok());

        let note = result.unwrap();
        assert!(note.frontmatter.is_some());
    }

    #[tokio::test]
    async fn test_file_size_limit() {
        let parser = MarkdownItParser::new().with_max_file_size(100);
        let content = "a".repeat(200); // 200 bytes
        let path = PathBuf::from("test.md");

        let result = parser.parse_content(&content, &path).await;
        assert!(result.is_err());
    }
}
