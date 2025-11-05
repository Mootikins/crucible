//! Core data types for parsed markdown documents

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

// Re-export ParseError
pub use crate::error::ParseError;

/// A fully parsed markdown document with extracted metadata
///
/// This structure represents the parsed and indexed form of a markdown file,
/// containing all structured data needed for indexing and querying.
///
/// # Memory Characteristics
///
/// Estimated size: ~3 KB per document (increased from enhanced parsing)
/// - PathBuf: ~24 bytes (SmallVec optimization)
/// - Frontmatter: ~200 bytes average
/// - Wikilinks: ~50 bytes × 10 avg = 500 bytes
/// - Tags: ~40 bytes × 5 avg = 200 bytes
/// - Callouts: ~80 bytes × 3 avg = 240 bytes (new)
/// - LaTeX: ~60 bytes × 2 avg = 120 bytes (new)
/// - Footnotes: ~70 bytes × 5 avg = 350 bytes (new)
/// - Content: ~1 KB (plain text excerpt)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    /// Original file path (absolute)
    pub path: PathBuf,

    /// Parsed frontmatter metadata
    pub frontmatter: Option<Frontmatter>,

    /// Extracted wikilinks [[note]]
    pub wikilinks: Vec<Wikilink>,

    /// Extracted tags #tag
    pub tags: Vec<Tag>,

    /// Parsed document content structure
    pub content: DocumentContent,

    /// Extracted Obsidian-style callouts > [!type]
    pub callouts: Vec<Callout>,

    /// LaTeX mathematical expressions ($...$ and $$...$$)
    pub latex_expressions: Vec<LatexExpression>,

    /// Footnote definitions and references
    pub footnotes: FootnoteMap,

    /// When this document was parsed
    pub parsed_at: DateTime<Utc>,

    /// Hash of file content (for change detection)
    pub content_hash: String,

    /// File size in bytes
    pub file_size: u64,

    /// Parsing errors encountered (non-fatal)
    pub parse_errors: Vec<ParseError>,
}

impl ParsedDocument {
    /// Create a new parsed document
    pub fn new(path: PathBuf) -> Self {
        Self::builder(path).build()
    }

    /// Create a document builder for migration compatibility
    pub fn builder(path: PathBuf) -> ParsedDocumentBuilder {
        ParsedDocumentBuilder::new(path)
    }

    /// Legacy compatibility constructor for existing tests
    pub fn legacy(path: PathBuf, frontmatter: Option<Frontmatter>, wikilinks: Vec<Wikilink>, tags: Vec<Tag>, content: DocumentContent, parsed_at: DateTime<Utc>, content_hash: String, file_size: u64) -> Self {
        Self {
            path,
            frontmatter,
            wikilinks,
            tags,
            content,
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at,
            content_hash,
            file_size,
            parse_errors: Vec::new(),
        }
    }

    /// Get the document title (from frontmatter or filename)
    pub fn title(&self) -> String {
        self.frontmatter
            .as_ref()
            .and_then(|fm| fm.get_string("title"))
            .unwrap_or_else(|| {
                self.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            })
    }

    /// Get all frontmatter tags combined with inline tags
    pub fn all_tags(&self) -> Vec<String> {
        let mut all_tags = self.tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>();

        if let Some(fm) = &self.frontmatter {
            if let Some(fm_tags) = fm.get_array("tags") {
                all_tags.extend(fm_tags);
            }
        }

        all_tags.sort();
        all_tags.dedup();
        all_tags
    }

    /// Get the first heading as a fallback title
    pub fn first_heading(&self) -> Option<&str> {
        self.content.headings.first().map(|h| h.text.as_str())
    }
}

/// Frontmatter metadata block
///
/// Supports both YAML (---) and TOML (+++) frontmatter formats.
/// Properties are lazily parsed to avoid allocation overhead when not accessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Raw frontmatter content (without delimiters)
    pub raw: String,

    /// Frontmatter format
    pub format: FrontmatterFormat,

    /// Lazily parsed properties
    #[serde(skip)]
    properties: OnceLock<HashMap<String, serde_json::Value>>,
}

impl Frontmatter {
    /// Create new frontmatter from raw string
    pub fn new(raw: String, format: FrontmatterFormat) -> Self {
        Self {
            raw,
            format,
            properties: OnceLock::new(),
        }
    }

    /// Get parsed properties (lazy initialization)
    pub fn properties(&self) -> &HashMap<String, serde_json::Value> {
        self.properties.get_or_init(|| self.parse_properties())
    }

    /// Parse properties based on format
    fn parse_properties(&self) -> HashMap<String, serde_json::Value> {
        match self.format {
            FrontmatterFormat::Yaml => serde_yaml::from_str(&self.raw).unwrap_or_default(),
            FrontmatterFormat::Toml => toml::from_str(&self.raw)
                .ok()
                .and_then(|v: toml::Value| serde_json::to_value(v).ok())
                .and_then(|v| v.as_object().cloned())
                .map(|obj| obj.into_iter().collect())
                .unwrap_or_default(),
            FrontmatterFormat::None => HashMap::new(),
        }
    }

    /// Get a string property
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.properties().get(key)?.as_str().map(|s| s.to_string())
    }

    /// Get an array property
    pub fn get_array(&self, key: &str) -> Option<Vec<String>> {
        self.properties()
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .into()
    }

    /// Get a boolean property
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.properties().get(key)?.as_bool()
    }

    /// Get a number property
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.properties().get(key)?.as_f64()
    }

    /// Check if a property exists
    pub fn has(&self, key: &str) -> bool {
        self.properties().contains_key(key)
    }
}

/// Frontmatter format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrontmatterFormat {
    /// YAML frontmatter (---)
    Yaml,
    /// TOML frontmatter (+++)
    Toml,
    /// No frontmatter
    None,
}

/// Wikilink reference [[target|alias]]
///
/// Represents a link to another note in the kiln.
/// Supports both simple [[target]] and aliased [[target|alias]] forms,
/// as well as embeds ![[target]].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wikilink {
    /// Target note name (without .md extension)
    pub target: String,

    /// Optional display alias
    pub alias: Option<String>,

    /// Character offset in source document
    pub offset: usize,

    /// Whether this is an embed (![[note]])
    pub is_embed: bool,

    /// Block reference (#^block-id)
    pub block_ref: Option<String>,

    /// Heading reference (#heading)
    pub heading_ref: Option<String>,
}

impl Wikilink {
    /// Create a simple wikilink
    pub fn new(target: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: None,
            offset,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Create a wikilink with alias
    pub fn with_alias(target: impl Into<String>, alias: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: Some(alias.into()),
            offset,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Create an embed wikilink
    pub fn embed(target: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: None,
            offset,
            is_embed: true,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Get the display text (alias or target)
    pub fn display(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.target)
    }

    /// Parse a wikilink from raw text (e.g., "Note#heading|Alias")
    pub fn parse(text: &str, offset: usize, is_embed: bool) -> Self {
        let (target_part, alias) = if let Some((t, a)) = text.split_once('|') {
            (t, Some(a.to_string()))
        } else {
            (text, None)
        };

        let (target, heading_ref, block_ref) =
            if let Some((t, ref_part)) = target_part.split_once('#') {
                if ref_part.starts_with('^') {
                    (t.to_string(), None, Some(ref_part[1..].to_string()))
                } else {
                    (t.to_string(), Some(ref_part.to_string()), None)
                }
            } else {
                (target_part.to_string(), None, None)
            };

        Self {
            target,
            alias,
            offset,
            is_embed,
            block_ref,
            heading_ref,
        }
    }
}

/// Tag reference #tag or #nested/tag
///
/// Represents a tag in the document. Supports nested tags with forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag {
    /// Full tag name (without #)
    pub name: String,

    /// Tag path components (for nested tags)
    pub path: Vec<String>,

    /// Character offset in source document
    pub offset: usize,
}

impl Tag {
    /// Create a new tag
    pub fn new(name: impl Into<String>, offset: usize) -> Self {
        let name = name.into();
        let path = name.split('/').map(|s| s.to_string()).collect();
        Self { name, path, offset }
    }

    /// Get the root tag (first component)
    pub fn root(&self) -> &str {
        self.path.first().map(|s| s.as_str()).unwrap_or(&self.name)
    }

    /// Get the leaf tag (last component)
    pub fn leaf(&self) -> &str {
        self.path.last().map(|s| s.as_str()).unwrap_or(&self.name)
    }

    /// Check if this tag is nested
    pub fn is_nested(&self) -> bool {
        self.path.len() > 1
    }

    /// Get parent tag path
    pub fn parent(&self) -> Option<String> {
        if self.path.len() > 1 {
            Some(self.path[..self.path.len() - 1].join("/"))
        } else {
            None
        }
    }
}

/// Parsed document content structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentContent {
    /// Plain text content (markdown syntax stripped)
    ///
    /// Limited to first 1000 characters for search preview.
    /// Full content remains on disk.
    pub plain_text: String,

    /// Extracted heading structure
    pub headings: Vec<Heading>,

    /// Code blocks (for potential syntax-aware indexing)
    pub code_blocks: Vec<CodeBlock>,

    /// Paragraph blocks (for content chunking and search)
    pub paragraphs: Vec<Paragraph>,

    /// List blocks (for structured content extraction)
    pub lists: Vec<ListBlock>,

    /// LaTeX mathematical expressions extracted from content
    pub latex_expressions: Vec<LatexExpression>,

    /// Obsidian-style callouts extracted from content
    pub callouts: Vec<Callout>,

    /// Footnote definitions and references
    pub footnotes: FootnoteMap,

    /// Word count (approximate)
    pub word_count: usize,

    /// Character count
    pub char_count: usize,
}

impl DocumentContent {
    /// Create empty content
    pub fn new() -> Self {
        Self {
            plain_text: String::new(),
            headings: Vec::new(),
            code_blocks: Vec::new(),
            paragraphs: Vec::new(),
            lists: Vec::new(),
            latex_expressions: Vec::new(),
            callouts: Vec::new(),
            footnotes: FootnoteMap::new(),
            word_count: 0,
            char_count: 0,
        }
    }

    /// Set plain text and update counts
    pub fn with_plain_text(mut self, text: String) -> Self {
        self.word_count = text.split_whitespace().count();
        self.char_count = text.chars().count();
        // Limit to 1000 chars for index
        if text.len() > 1000 {
            self.plain_text = text.chars().take(1000).collect();
            self.plain_text.push_str("...");
        } else {
            self.plain_text = text;
        }
        self
    }

    /// Add a heading
    pub fn add_heading(&mut self, heading: Heading) {
        self.headings.push(heading);
    }

    /// Add a code block
    pub fn add_code_block(&mut self, block: CodeBlock) {
        self.code_blocks.push(block);
    }

    /// Get document outline (headings only)
    pub fn outline(&self) -> Vec<String> {
        self.headings
            .iter()
            .map(|h| format!("{}{}", "  ".repeat((h.level - 1) as usize), h.text))
            .collect()
    }
}

/// Markdown heading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    /// Heading level (1-6)
    pub level: u8,

    /// Heading text (without #)
    pub text: String,

    /// Character offset in source
    pub offset: usize,

    /// Generated heading ID (for linking)
    pub id: Option<String>,
}

impl Heading {
    /// Create a new heading
    pub fn new(level: u8, text: impl Into<String>, offset: usize) -> Self {
        let text = text.into();
        let id = Some(Self::generate_id(&text));
        Self {
            level,
            text,
            offset,
            id,
        }
    }

    /// Generate a heading ID from text (slugify)
    fn generate_id(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// Code block with optional language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Programming language (if specified)
    pub language: Option<String>,

    /// Code content
    pub content: String,

    /// Character offset in source
    pub offset: usize,

    /// Line count
    pub line_count: usize,
}

impl CodeBlock {
    /// Create a new code block
    pub fn new(language: Option<String>, content: String, offset: usize) -> Self {
        let line_count = content.lines().count();
        Self {
            language,
            content,
            offset,
            line_count,
        }
    }

    /// Check if this is a specific language
    pub fn is_language(&self, lang: &str) -> bool {
        self.language.as_deref() == Some(lang)
    }
}

/// Paragraph block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paragraph {
    /// Paragraph text content
    pub content: String,

    /// Character offset in source
    pub offset: usize,

    /// Word count in this paragraph
    pub word_count: usize,
}

impl Paragraph {
    /// Create a new paragraph
    pub fn new(content: String, offset: usize) -> Self {
        let word_count = content.split_whitespace().count();
        Self {
            content,
            offset,
            word_count,
        }
    }
}

/// List block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBlock {
    /// List type (ordered or unordered)
    pub list_type: ListType,

    /// List items
    pub items: Vec<ListItem>,

    /// Character offset in source
    pub offset: usize,

    /// Total item count
    pub item_count: usize,
}

impl ListBlock {
    /// Create a new list block
    pub fn new(list_type: ListType, offset: usize) -> Self {
        Self {
            list_type,
            items: Vec::new(),
            offset,
            item_count: 0,
        }
    }

    /// Add an item to the list
    pub fn add_item(&mut self, item: ListItem) {
        self.item_count += 1;
        self.items.push(item);
    }
}

/// List type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListType {
    /// Unordered list (-, *, +)
    Unordered,
    /// Ordered list (1., 2., etc.)
    Ordered,
}

/// List item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    /// Item text content
    pub content: String,

    /// Item level (for nested lists)
    pub level: usize,

    /// Task status (for task lists)
    pub task_status: Option<TaskStatus>,
}

impl ListItem {
    /// Create a new list item
    pub fn new(content: String, level: usize) -> Self {
        Self {
            content,
            level,
            task_status: None,
        }
    }

    /// Create a task list item
    pub fn new_task(content: String, level: usize, completed: bool) -> Self {
        Self {
            content,
            level,
            task_status: Some(if completed {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            }),
        }
    }
}

/// Task status for task list items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is completed ([x])
    Completed,
    /// Task is pending ([ ])
    Pending,
}

/// Obsidian-style callout > [!type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Callout {
    /// Callout type (note, tip, warning, danger, etc.)
    pub callout_type: String,

    /// Callout title (optional)
    pub title: Option<String>,

    /// Callout content
    pub content: String,

    /// Character offset in source document
    pub offset: usize,

    /// Whether this is a known callout type
    pub is_standard_type: bool,
}

impl Callout {
    /// Create a new callout
    pub fn new(callout_type: impl Into<String>, content: String, offset: usize) -> Self {
        let callout_type = callout_type.into();
        let is_standard_type = matches!(callout_type.as_str(), "note" | "tip" | "warning" | "danger" | "info" | "abstract" | "summary" | "tldr" | "todo" | "question" | "success" | "failure" | "example" | "quote");

        Self {
            callout_type,
            title: None,
            content,
            offset,
            is_standard_type,
        }
    }

    /// Create a callout with title
    pub fn with_title(callout_type: impl Into<String>, title: impl Into<String>, content: String, offset: usize) -> Self {
        let mut callout = Self::new(callout_type, content, offset);
        callout.title = Some(title.into());
        callout
    }

    /// Get the display type with fallback
    pub fn display_type(&self) -> &str {
        if self.is_standard_type {
            &self.callout_type
        } else {
            "note" // fallback to generic note type
        }
    }
}

/// LaTeX mathematical expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatexExpression {
    /// LaTeX expression content
    pub expression: String,

    /// Whether this is inline ($) or block ($$) math
    pub is_block: bool,

    /// Character offset in source document
    pub offset: usize,

    /// Length of the expression in source
    pub length: usize,
}

impl LatexExpression {
    /// Create a new LaTeX expression
    pub fn new(expression: String, is_block: bool, offset: usize, length: usize) -> Self {
        Self {
            expression,
            is_block,
            offset,
            length,
        }
    }

    /// Get the expression type as a string
    pub fn expression_type(&self) -> &'static str {
        if self.is_block {
            "block"
        } else {
            "inline"
        }
    }
}

/// Footnote definitions and references
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FootnoteMap {
    /// Footnote definitions [^1]: content
    pub definitions: HashMap<String, FootnoteDefinition>,

    /// Footnote references in text order
    pub references: Vec<FootnoteReference>,
}

impl FootnoteMap {
    /// Create a new empty footnote map
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a footnote definition
    pub fn add_definition(&mut self, identifier: String, definition: FootnoteDefinition) {
        self.definitions.insert(identifier, definition);
    }

    /// Add a footnote reference
    pub fn add_reference(&mut self, reference: FootnoteReference) {
        self.references.push(reference);
    }

    /// Get a footnote definition by identifier
    pub fn get_definition(&self, identifier: &str) -> Option<&FootnoteDefinition> {
        self.definitions.get(identifier)
    }

    /// Get all orphaned references (no definition found)
    pub fn orphaned_references(&self) -> Vec<&FootnoteReference> {
        self.references
            .iter()
            .filter(|ref_| !self.definitions.contains_key(&ref_.identifier))
            .collect()
    }

    /// Get all unused definitions (no references found)
    pub fn unused_definitions(&self) -> Vec<&String> {
        self.definitions
            .keys()
            .filter(|key| !self.references.iter().any(|ref_| &ref_.identifier == *key))
            .collect()
    }
}

/// Footnote definition [^1]: content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteDefinition {
    /// Footnote identifier (without [^] and :)
    pub identifier: String,

    /// Footnote content
    pub content: String,

    /// Character offset in source document
    pub offset: usize,

    /// Line number in source
    pub line_number: usize,
}

impl FootnoteDefinition {
    /// Create a new footnote definition
    pub fn new(identifier: String, content: String, offset: usize, line_number: usize) -> Self {
        Self {
            identifier,
            content,
            offset,
            line_number,
        }
    }
}

/// Footnote reference [^1]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteReference {
    /// Footnote identifier (without [^])
    pub identifier: String,

    /// Character offset in source document
    pub offset: usize,

    /// Reference order number (for sequential numbering)
    pub order_number: Option<usize>,
}

impl FootnoteReference {
    /// Create a new footnote reference
    pub fn new(identifier: String, offset: usize) -> Self {
        Self {
            identifier,
            offset,
            order_number: None,
        }
    }

    /// Create a footnote reference with order number
    pub fn with_order(identifier: String, offset: usize, order_number: usize) -> Self {
        Self {
            identifier,
            offset,
            order_number: Some(order_number),
        }
    }
}

/// Builder for ParsedDocument to support migration and test compatibility
pub struct ParsedDocumentBuilder {
    path: PathBuf,
    frontmatter: Option<Frontmatter>,
    wikilinks: Vec<Wikilink>,
    tags: Vec<Tag>,
    content: DocumentContent,
    callouts: Vec<Callout>,
    latex_expressions: Vec<LatexExpression>,
    footnotes: FootnoteMap,
    parsed_at: Option<DateTime<Utc>>,
    content_hash: String,
    file_size: u64,
    parse_errors: Vec<ParseError>,
}

impl ParsedDocumentBuilder {
    /// Create a new builder with just the path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            frontmatter: None,
            wikilinks: Vec::new(),
            tags: Vec::new(),
            content: DocumentContent::default(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: None,
            content_hash: String::new(),
            file_size: 0,
            parse_errors: Vec::new(),
        }
    }

    /// Set frontmatter
    pub fn with_frontmatter(mut self, frontmatter: Option<Frontmatter>) -> Self {
        self.frontmatter = frontmatter;
        self
    }

    /// Set wikilinks
    pub fn with_wikilinks(mut self, wikilinks: Vec<Wikilink>) -> Self {
        self.wikilinks = wikilinks;
        self
    }

    /// Set tags
    pub fn with_tags(mut self, tags: Vec<Tag>) -> Self {
        self.tags = tags;
        self
    }

    /// Set content
    pub fn with_content(mut self, content: DocumentContent) -> Self {
        self.content = content;
        self
    }

    /// Set parsed timestamp
    pub fn with_parsed_at(mut self, parsed_at: DateTime<Utc>) -> Self {
        self.parsed_at = Some(parsed_at);
        self
    }

    /// Set content hash
    pub fn with_content_hash(mut self, content_hash: String) -> Self {
        self.content_hash = content_hash;
        self
    }

    /// Set file size
    pub fn with_file_size(mut self, file_size: u64) -> Self {
        self.file_size = file_size;
        self
    }

    /// Build the ParsedDocument
    pub fn build(self) -> ParsedDocument {
        ParsedDocument {
            path: self.path,
            frontmatter: self.frontmatter,
            wikilinks: self.wikilinks,
            tags: self.tags,
            content: self.content,
            callouts: self.callouts,
            latex_expressions: self.latex_expressions,
            footnotes: self.footnotes,
            parsed_at: self.parsed_at.unwrap_or_else(Utc::now),
            content_hash: self.content_hash,
            file_size: self.file_size,
            parse_errors: self.parse_errors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wikilink_parse() {
        let link = Wikilink::parse("Note A", 10, false);
        assert_eq!(link.target, "Note A");
        assert_eq!(link.alias, None);
        assert!(!link.is_embed);

        let link = Wikilink::parse("Note B|My Alias", 20, false);
        assert_eq!(link.target, "Note B");
        assert_eq!(link.alias, Some("My Alias".to_string()));

        let link = Wikilink::parse("Note#heading", 30, false);
        assert_eq!(link.target, "Note");
        assert_eq!(link.heading_ref, Some("heading".to_string()));

        let link = Wikilink::parse("Note#^block", 40, false);
        assert_eq!(link.target, "Note");
        assert_eq!(link.block_ref, Some("block".to_string()));
    }

    #[test]
    fn test_tag_nested() {
        let tag = Tag::new("project/ai/llm", 10);
        assert_eq!(tag.path.len(), 3);
        assert_eq!(tag.root(), "project");
        assert_eq!(tag.leaf(), "llm");
        assert!(tag.is_nested());
        assert_eq!(tag.parent(), Some("project/ai".to_string()));
    }

    #[test]
    fn test_frontmatter_yaml() {
        let yaml = "title: Test Note\ntags: [ai, rust]";
        let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

        assert_eq!(fm.get_string("title"), Some("Test Note".to_string()));
        assert_eq!(
            fm.get_array("tags"),
            Some(vec!["ai".to_string(), "rust".to_string()])
        );
    }

    #[test]
    fn test_heading_id_generation() {
        let heading = Heading::new(1, "Hello World!", 0);
        assert_eq!(heading.id, Some("hello-world".to_string()));

        let heading = Heading::new(2, "API Reference (v2)", 10);
        assert_eq!(heading.id, Some("api-reference-v2".to_string()));
    }

    #[test]
    fn test_document_content_word_count() {
        let content = DocumentContent::new().with_plain_text("Hello world test".to_string());
        assert_eq!(content.word_count, 3);
        assert_eq!(content.char_count, 16);
    }

    #[test]
    fn test_parsed_document_all_tags() {
        let mut doc = ParsedDocument::new(PathBuf::from("test.md"));
        doc.tags = vec![Tag::new("rust", 0), Tag::new("ai", 10)];

        let yaml = "tags: [project, parsing]";
        doc.frontmatter = Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml));

        let all_tags = doc.all_tags();
        assert_eq!(all_tags.len(), 4);
        assert!(all_tags.contains(&"rust".to_string()));
        assert!(all_tags.contains(&"project".to_string()));
    }
}
