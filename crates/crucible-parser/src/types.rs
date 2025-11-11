//! Core data types for parsed markdown notes
//!
//! # Type Ownership
//!
//! This module contains the **canonical definitions** of all parser-related types.
//! These types are re-exported by `crucible-core::parser` for convenience.
//!
//! ## Canonical Locations
//!
//! - **Parser Types**: This module (`crucible_parser::types`)
//! - **Hash Types**: This module (`BlockHash` - local to avoid circular deps)
//! - **AST Types**: This module (parser implementation detail)
//!
//! ## Import Guidelines
//!
//! Prefer importing from the canonical location:
//! ```rust,ignore
//! use crucible_parser::types::{ParsedNote, Wikilink, Tag, BlockHash};
//! ```
//!
//! Re-exports are available for convenience:
//! ```rust,ignore
//! use crucible_core::parser::{ParsedNote, Wikilink, Tag};
//! ```

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

// Re-export ParseError
pub use crate::error::ParseError;

/// A BLAKE3 hash used for block-level content addressing
///
/// Similar to FileHash but specifically used for individual content blocks
/// extracted from documents (headings, paragraphs, code blocks, etc.).
///
/// This is a local copy of the type from crucible-core to avoid circular dependencies.
/// The types are kept in sync for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    /// Create a new BlockHash from raw bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the hash as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Get the hash as a hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Create a BlockHash from a hex string
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex).map_err(|_| "Invalid hex format".to_string())?;
        if bytes.len() != 32 {
            return Err("Invalid hash length: expected 32 bytes".to_string());
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }

    /// Create a zero hash
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Check if this is a zero hash
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl Default for BlockHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}

impl From<&[u8; 32]> for BlockHash {
    fn from(bytes: &[u8; 32]) -> Self {
        Self::new(*bytes)
    }
}

impl std::fmt::Display for BlockHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl std::str::FromStr for BlockHash {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// A fully parsed markdown note with extracted metadata
///
/// This structure represents the parsed and indexed form of a markdown file,
/// containing all structured data needed for indexing and querying.
///
/// # Memory Characteristics
///
/// Estimated size: ~3 KB per note (increased from enhanced parsing)
/// - PathBuf: ~24 bytes (SmallVec optimization)
/// - Frontmatter: ~200 bytes average
/// - Wikilinks: ~50 bytes × 10 avg = 500 bytes
/// - Tags: ~40 bytes × 5 avg = 200 bytes
/// - Callouts: ~80 bytes × 3 avg = 240 bytes (new)
/// - LaTeX: ~60 bytes × 2 avg = 120 bytes (new)
/// - Footnotes: ~70 bytes × 5 avg = 350 bytes (new)
/// - Content: ~1 KB (plain text excerpt)
/// - Block hashes: ~64 bytes × 5 avg = 320 bytes (Phase 2 enhancement)
/// - Merkle root: 32 bytes (Phase 2 enhancement)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedNote {
    /// Original file path (absolute)
    pub path: PathBuf,

    /// Parsed frontmatter metadata
    pub frontmatter: Option<Frontmatter>,

    /// Extracted wikilinks [[note]]
    pub wikilinks: Vec<Wikilink>,

    /// Extracted tags #tag
    pub tags: Vec<Tag>,

    /// Extracted inline markdown links [text](url)
    pub inline_links: Vec<InlineLink>,

    /// Parsed note content structure
    pub content: NoteContent,

    /// Extracted Obsidian-style callouts > [!type]
    pub callouts: Vec<Callout>,

    /// LaTeX mathematical expressions ($...$ and $$...$$)
    pub latex_expressions: Vec<LatexExpression>,

    /// Footnote definitions and references
    pub footnotes: FootnoteMap,

    /// When this note was parsed
    pub parsed_at: DateTime<Utc>,

    /// Hash of file content (for change detection)
    pub content_hash: String,

    /// File size in bytes
    pub file_size: u64,

    /// Parsing errors encountered (non-fatal)
    pub parse_errors: Vec<ParseError>,

    /// Block-level content hashes for Phase 2 optimize-data-flow
    ///
    /// This field stores hashes of individual content blocks (headings, paragraphs,
    /// code blocks, etc.) enabling fine-grained change detection and Merkle tree diffing.
    /// Each block hash represents a discrete semantic unit within the note.
    ///
    /// Empty until Phase 2 implementation populates it during parsing.
    /// Maintained for backward compatibility - existing documents will have empty vectors.
    #[serde(default)]
    pub block_hashes: Vec<BlockHash>,

    /// Merkle root hash of all block hashes for Phase 2 optimize-data-flow
    ///
    /// This field stores the root hash of the Merkle tree constructed from all
    /// block hashes in the note. It enables efficient note-level change
    /// detection while supporting fine-grained diffing when needed.
    ///
    /// None until Phase 2 implementation computes it during parsing.
    /// Maintained for backward compatibility - existing documents will have None.
    #[serde(default)]
    pub merkle_root: Option<BlockHash>,
}

impl ParsedNote {
    /// Create a new parsed note
    pub fn new(path: PathBuf) -> Self {
        Self::builder(path).build()
    }

    /// Create a note builder for migration compatibility
    pub fn builder(path: PathBuf) -> ParsedNoteBuilder {
        ParsedNoteBuilder::new(path)
    }

    /// Legacy compatibility constructor for existing tests
    pub fn legacy(
        path: PathBuf,
        frontmatter: Option<Frontmatter>,
        wikilinks: Vec<Wikilink>,
        tags: Vec<Tag>,
        content: NoteContent,
        parsed_at: DateTime<Utc>,
        content_hash: String,
        file_size: u64,
    ) -> Self {
        Self {
            path,
            frontmatter,
            wikilinks,
            tags,
            inline_links: Vec::new(),
            content,
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at,
            content_hash,
            file_size,
            parse_errors: Vec::new(),
            block_hashes: Vec::new(), // Phase 2: empty by default for backward compatibility
            merkle_root: None,        // Phase 2: None by default for backward compatibility
        }
    }
}

impl Default for ParsedNote {
    fn default() -> Self {
        Self::new(PathBuf::from(""))
    }
}

impl ParsedNote {
    /// Get the note title (from frontmatter or filename)
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

    /// Check if this note has block hashes (Phase 2 support)
    pub fn has_block_hashes(&self) -> bool {
        !self.block_hashes.is_empty()
    }

    /// Get the number of block hashes
    pub fn block_hash_count(&self) -> usize {
        self.block_hashes.len()
    }

    /// Check if this note has a Merkle root (Phase 2 support)
    pub fn has_merkle_root(&self) -> bool {
        self.merkle_root.is_some()
    }

    /// Get the Merkle root hash if available
    pub fn get_merkle_root(&self) -> Option<BlockHash> {
        self.merkle_root
    }

    /// Set block hashes (for Phase 2 parser implementation)
    pub fn with_block_hashes(mut self, block_hashes: Vec<BlockHash>) -> Self {
        self.block_hashes = block_hashes;
        self
    }

    /// Set Merkle root (for Phase 2 parser implementation)
    pub fn with_merkle_root(mut self, merkle_root: Option<BlockHash>) -> Self {
        self.merkle_root = merkle_root;
        self
    }

    /// Add a single block hash (for incremental building)
    pub fn add_block_hash(&mut self, block_hash: BlockHash) {
        self.block_hashes.push(block_hash);
    }

    /// Clear all block hashes and Merkle root
    pub fn clear_hash_data(&mut self) {
        self.block_hashes.clear();
        self.merkle_root = None;
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

    /// Get a date property
    ///
    /// Supports multiple date formats:
    /// - ISO 8601: "2024-11-08"
    /// - RFC 3339: "2024-11-08T10:30:00Z"
    /// - Integer (YYYYMMDD): 20241108
    pub fn get_date(&self, key: &str) -> Option<NaiveDate> {
        let value = self.properties().get(key)?;

        // Try as string first (most common format)
        if let Some(date_str) = value.as_str() {
            // Try ISO 8601 format (YYYY-MM-DD)
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                return Some(date);
            }
            // Try RFC 3339 format (with time)
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
                return Some(dt.date_naive());
            }
        }

        // Try as integer (YYYYMMDD format)
        if let Some(num) = value.as_i64() {
            let year = (num / 10000) as i32;
            let month = ((num % 10000) / 100) as u32;
            let day = (num % 100) as u32;
            return NaiveDate::from_ymd_opt(year, month, day);
        }

        None
    }

    /// Get an object (nested hash map) property
    ///
    /// Returns the object as a serde_json::Map for further processing.
    /// Note: Flat frontmatter structure is preferred (following Obsidian conventions),
    /// but objects are supported for compatibility with existing content.
    pub fn get_object(&self, key: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        self.properties().get(key)?.as_object().cloned()
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

    /// Character offset in source note
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
/// Represents a tag in the note. Supports nested tags with forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag {
    /// Full tag name (without #)
    pub name: String,

    /// Tag path components (for nested tags)
    pub path: Vec<String>,

    /// Character offset in source note
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

/// Parsed note content structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteContent {
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

    /// Inline markdown links [text](url)
    pub inline_links: Vec<InlineLink>,

    /// LaTeX mathematical expressions extracted from content
    pub latex_expressions: Vec<LatexExpression>,

    /// Obsidian-style callouts extracted from content
    pub callouts: Vec<Callout>,

    /// Regular blockquotes (not callouts)
    pub blockquotes: Vec<Blockquote>,

    /// Footnote definitions and references
    pub footnotes: FootnoteMap,

    /// Markdown tables
    pub tables: Vec<Table>,

    /// Horizontal rules (--- or ***)
    pub horizontal_rules: Vec<HorizontalRule>,

    /// Word count (approximate)
    pub word_count: usize,

    /// Character count
    pub char_count: usize,
}

impl NoteContent {
    /// Create empty content
    pub fn new() -> Self {
        Self {
            plain_text: String::new(),
            headings: Vec::new(),
            code_blocks: Vec::new(),
            paragraphs: Vec::new(),
            lists: Vec::new(),
            inline_links: Vec::new(),
            latex_expressions: Vec::new(),
            callouts: Vec::new(),
            blockquotes: Vec::new(),
            footnotes: FootnoteMap::new(),
            tables: Vec::new(),
            horizontal_rules: Vec::new(),
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

    /// Get note outline (headings only)
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

/// A markdown table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Raw table content (with pipes and formatting)
    pub raw_content: String,
    /// Table headers
    pub headers: Vec<String>,
    /// Number of columns
    pub columns: usize,
    /// Number of data rows (excluding header)
    pub rows: usize,
    /// Character offset in source
    pub offset: usize,
}

impl Table {
    /// Create a new table
    pub fn new(raw_content: String, headers: Vec<String>, columns: usize, rows: usize, offset: usize) -> Self {
        Self {
            raw_content,
            headers,
            columns,
            rows,
            offset,
        }
    }
}

/// Blockquote content (not a callout)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blockquote {
    /// Blockquote content
    pub content: String,
    /// Nesting level (0 for single >, 1 for >>, etc.)
    pub nested_level: u8,
    /// Character offset in source
    pub offset: usize,
}

impl Blockquote {
    /// Create a new blockquote
    pub fn new(content: String, offset: usize) -> Self {
        Self {
            content,
            nested_level: 0,
            offset,
        }
    }

    /// Create a new blockquote with nesting level
    pub fn with_nesting(content: String, nested_level: u8, offset: usize) -> Self {
        Self {
            content,
            nested_level,
            offset,
        }
    }
}

/// A horizontal rule / thematic break
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HorizontalRule {
    /// Raw content (e.g., "---" or "***")
    pub raw_content: String,

    /// Style indicator (dash, asterisk, underscore)
    pub style: String,

    /// Character offset in source note
    pub offset: usize,
}

impl HorizontalRule {
    /// Create a new horizontal rule
    pub fn new(raw_content: String, style: String, offset: usize) -> Self {
        Self {
            raw_content,
            style,
            offset,
        }
    }

    /// Detect style from raw content
    pub fn detect_style(content: &str) -> String {
        if content.contains('-') {
            "dash".to_string()
        } else if content.contains('*') {
            "asterisk".to_string()
        } else if content.contains('_') {
            "underscore".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Get the length of the horizontal rule
    pub fn length(&self) -> usize {
        self.raw_content.len()
    }
}

/// AST block type enumeration
///
/// Represents the different types of semantic blocks that can be extracted
/// from a markdown note. Each block type corresponds to a natural
/// semantic boundary that aligns with user mental model and HTML rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ASTBlockType {
    /// Heading block (# ## ### etc.)
    Heading,
    /// Paragraph block (plain text content)
    Paragraph,
    /// Code block (```language ... ````)
    Code,
    /// List block (ordered or unordered lists)
    List,
    /// Callout block (> [!type] ...)
    Callout,
    /// LaTeX mathematical expression ($...$ or $$...$$)
    Latex,
    /// Block quote (> content)
    Blockquote,
    /// Table block
    Table,
    /// Horizontal rule (--- or ***)
    HorizontalRule,
    /// Thematic break or divider
    ThematicBreak,
}

impl ASTBlockType {
    /// Get the string representation of this block type
    ///
    /// Returns a zero-cost &'static str
    pub fn as_str(&self) -> &'static str {
        match self {
            ASTBlockType::Heading => "heading",
            ASTBlockType::Paragraph => "paragraph",
            ASTBlockType::Code => "code",
            ASTBlockType::List => "list",
            ASTBlockType::Callout => "callout",
            ASTBlockType::Latex => "latex",
            ASTBlockType::Blockquote => "blockquote",
            ASTBlockType::Table => "table",
            ASTBlockType::HorizontalRule => "horizontal_rule",
            ASTBlockType::ThematicBreak => "thematic_break",
        }
    }
}

impl std::fmt::Display for ASTBlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// AST Block representing a semantic unit in a markdown note
///
/// AST blocks are natural semantic boundaries that correspond to complete
/// structural elements in markdown. Each block represents one coherent
/// unit that would typically render as a single HTML element or related
/// group of elements.
///
/// # Memory Characteristics
///
/// Estimated size: ~200-500 bytes per block depending on content length
/// - content: variable (main contributor)
/// - block_hash: 32 bytes (BLAKE3 hash)
/// - metadata: ~50 bytes
///
/// # Block Boundaries and Semantics
///
/// Blocks align with:
/// - User mental model (editing a paragraph = one block changed)
/// - HTML rendering (one block = one or more related HTML elements)
/// - AST node boundaries without artificial chunking
/// - Natural content units (complete paragraphs, full code blocks, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASTBlock {
    /// Type of this block
    pub block_type: ASTBlockType,

    /// The actual content of this block
    pub content: String,

    /// Character offset where this block starts in the source note
    pub start_offset: usize,

    /// Character offset where this block ends in the source note
    pub end_offset: usize,

    /// Cryptographic hash of the block content (BLAKE3)
    ///
    /// This hash uniquely identifies the content of the block and is used
    /// for change detection, deduplication, and content-addressed storage.
    pub block_hash: String,

    /// Block-level metadata that varies by type
    ///
    /// For headings: level (1-6)
    /// For code blocks: language identifier
    /// For callouts: callout type
    /// For lists: ordered/unordered flag
    pub metadata: ASTBlockMetadata,

    /// Parent block ID for hierarchy tracking
    ///
    /// For blocks under a heading: ID of the parent heading
    /// For headings: ID of the parent heading (if nested)
    /// For top-level blocks: None
    ///
    /// This enables:
    /// - Merkle tree construction (rehash only changed subtree)
    /// - Note structure queries ("all blocks under X")
    /// - Context breadcrumbs for AI/search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_block_id: Option<String>,

    /// Depth in the heading hierarchy
    ///
    /// - 0: Top-level blocks (no parent heading)
    /// - 1: Under H1
    /// - 2: Under H2 (under H1)
    /// - etc.
    ///
    /// Calculated based on heading nesting level, not just heading level.
    /// Example: H3 under H1 (skipped H2) has depth=1, not depth=3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

impl ASTBlock {
    /// Create a new AST block
    pub fn new(
        block_type: ASTBlockType,
        content: String,
        start_offset: usize,
        end_offset: usize,
        metadata: ASTBlockMetadata,
    ) -> Self {
        let block_hash = Self::compute_hash(&content);
        Self {
            block_type,
            content,
            start_offset,
            end_offset,
            block_hash,
            metadata,
            parent_block_id: None,
            depth: None,
        }
    }

    /// Create a new AST block with explicit hash (for loading from storage)
    pub fn with_hash(
        block_type: ASTBlockType,
        content: String,
        start_offset: usize,
        end_offset: usize,
        block_hash: String,
        metadata: ASTBlockMetadata,
    ) -> Self {
        Self {
            block_type,
            content,
            start_offset,
            end_offset,
            block_hash,
            metadata,
            parent_block_id: None,
            depth: None,
        }
    }

    /// Compute BLAKE3 hash of block content
    fn compute_hash(content: &str) -> String {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        hex::encode(result.as_bytes())
    }

    /// Get the length of this block in characters
    pub fn length(&self) -> usize {
        self.end_offset - self.start_offset
    }

    /// Get the length of the content (excluding markdown syntax)
    pub fn content_length(&self) -> usize {
        self.content.len()
    }

    /// Check if this block contains any content
    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Get a string representation of this block type
    pub fn type_name(&self) -> &'static str {
        match self.block_type {
            ASTBlockType::Heading => "heading",
            ASTBlockType::Paragraph => "paragraph",
            ASTBlockType::Code => "code",
            ASTBlockType::List => "list",
            ASTBlockType::Callout => "callout",
            ASTBlockType::Latex => "latex",
            ASTBlockType::Blockquote => "blockquote",
            ASTBlockType::Table => "table",
            ASTBlockType::HorizontalRule => "horizontal_rule",
            ASTBlockType::ThematicBreak => "thematic_break",
        }
    }

    /// Check if this block is a heading level
    pub fn is_heading_level(&self, level: u8) -> bool {
        matches!(self.block_type, ASTBlockType::Heading)
            && matches!(&self.metadata, ASTBlockMetadata::Heading { level: l, .. } if *l == level)
    }

    /// Check if this block is a code block with specific language
    pub fn is_code_language(&self, language: &str) -> bool {
        matches!(self.block_type, ASTBlockType::Code)
            && matches!(&self.metadata, ASTBlockMetadata::Code { language: lang, .. }
                if lang.as_ref().map_or(false, |l| l == language))
    }

    /// Check if this block is a specific callout type
    pub fn is_callout_type(&self, callout_type: &str) -> bool {
        matches!(self.block_type, ASTBlockType::Callout)
            && matches!(&self.metadata, ASTBlockMetadata::Callout { callout_type: ct, .. } if ct == callout_type)
    }

    /// Builder method: Set the parent block ID for hierarchy tracking
    #[must_use = "builder methods consume self and return a new value"]
    pub fn with_parent(mut self, parent_block_id: impl Into<String>) -> Self {
        self.parent_block_id = Some(parent_block_id.into());
        self
    }

    /// Builder method: Set the depth in the heading hierarchy
    #[must_use = "builder methods consume self and return a new value"]
    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = Some(depth);
        self
    }

    /// Builder method: Set both parent and depth for hierarchy
    #[must_use = "builder methods consume self and return a new value"]
    pub fn with_hierarchy(mut self, parent_block_id: impl Into<String>, depth: u32) -> Self {
        self.parent_block_id = Some(parent_block_id.into());
        self.depth = Some(depth);
        self
    }

    /// Get the heading level if this is a heading block
    pub fn heading_level(&self) -> Option<u8> {
        match &self.metadata {
            ASTBlockMetadata::Heading { level, .. } => Some(*level),
            _ => None,
        }
    }

    /// Check if this block is a heading
    pub fn is_heading(&self) -> bool {
        matches!(self.block_type, ASTBlockType::Heading)
    }
}

/// Block-specific metadata that varies by block type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ASTBlockMetadata {
    /// Heading metadata
    Heading {
        /// Heading level (1-6)
        level: u8,
        /// Generated heading ID for linking
        id: Option<String>,
    },

    /// Code block metadata
    Code {
        /// Programming language identifier (if specified)
        language: Option<String>,
        /// Number of lines in the code block
        line_count: usize,
    },

    /// List metadata
    List {
        /// List type (ordered or unordered)
        list_type: ListType,
        /// Number of items in the list
        item_count: usize,
    },

    /// Callout metadata
    Callout {
        /// Callout type (note, tip, warning, etc.)
        callout_type: String,
        /// Callout title (optional)
        title: Option<String>,
        /// Whether this is a standard callout type
        is_standard_type: bool,
    },

    /// LaTeX metadata
    Latex {
        /// Whether this is block ($$) or inline ($) math
        is_block: bool,
    },

    /// Table metadata
    Table {
        /// Number of rows (excluding header)
        rows: usize,
        /// Number of columns
        columns: usize,
        /// Table headers
        headers: Vec<String>,
    },

    /// Generic metadata for block types that don't need specific fields
    Generic,
}

impl ASTBlockMetadata {
    /// Create heading metadata
    pub fn heading(level: u8, id: Option<String>) -> Self {
        Self::Heading { level, id }
    }

    /// Create code block metadata
    pub fn code(language: Option<String>, line_count: usize) -> Self {
        Self::Code {
            language,
            line_count,
        }
    }

    /// Create list metadata
    pub fn list(list_type: ListType, item_count: usize) -> Self {
        Self::List {
            list_type,
            item_count,
        }
    }

    /// Create callout metadata
    pub fn callout(callout_type: String, title: Option<String>, is_standard_type: bool) -> Self {
        Self::Callout {
            callout_type,
            title,
            is_standard_type,
        }
    }

    /// Create LaTeX metadata
    pub fn latex(is_block: bool) -> Self {
        Self::Latex { is_block }
    }

    /// Create table metadata
    pub fn table(rows: usize, columns: usize, headers: Vec<String>) -> Self {
        Self::Table {
            rows,
            columns,
            headers,
        }
    }

    /// Create generic metadata
    pub fn generic() -> Self {
        Self::Generic
    }
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

    /// Character offset in source note
    pub offset: usize,

    /// Whether this is a known callout type
    pub is_standard_type: bool,
}

impl Callout {
    /// Create a new callout
    pub fn new(callout_type: impl Into<String>, content: String, offset: usize) -> Self {
        let callout_type = callout_type.into();
        let is_standard_type = matches!(
            callout_type.as_str(),
            "note"
                | "tip"
                | "warning"
                | "danger"
                | "info"
                | "abstract"
                | "summary"
                | "tldr"
                | "todo"
                | "question"
                | "success"
                | "failure"
                | "example"
                | "quote"
        );

        Self {
            callout_type,
            title: None,
            content,
            offset,
            is_standard_type,
        }
    }

    /// Create a callout with title
    pub fn with_title(
        callout_type: impl Into<String>,
        title: impl Into<String>,
        content: String,
        offset: usize,
    ) -> Self {
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

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }

    /// Get the total length of the callout
    pub fn length(&self) -> usize {
        // Calculate total length including callout header and content
        let header_len = if let Some(title) = &self.title {
            format!("> [!{}] {}\n", self.callout_type, title).len()
        } else {
            format!("> [!{}]\n", self.callout_type).len()
        };
        header_len + self.content.len()
    }
}

/// LaTeX mathematical expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatexExpression {
    /// LaTeX expression content
    pub expression: String,

    /// Whether this is inline ($) or block ($$) math
    pub is_block: bool,

    /// Character offset in source note
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

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }
}

/// Inline markdown link [text](url)
///
/// Represents a standard markdown link (not a wikilink).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineLink {
    /// Link text (displayed to user)
    pub text: String,

    /// URL or relative path
    pub url: String,

    /// Optional link title attribute
    pub title: Option<String>,

    /// Character offset in source note
    pub offset: usize,
}

impl InlineLink {
    /// Create a new inline link
    pub fn new(text: String, url: String, offset: usize) -> Self {
        Self {
            text,
            url,
            title: None,
            offset,
        }
    }

    /// Create an inline link with title
    pub fn with_title(text: String, url: String, title: String, offset: usize) -> Self {
        Self {
            text,
            url,
            title: Some(title),
            offset,
        }
    }

    /// Check if this is an external link (starts with http:// or https://)
    pub fn is_external(&self) -> bool {
        self.url.starts_with("http://") || self.url.starts_with("https://")
    }

    /// Check if this is a relative link (internal to vault)
    pub fn is_relative(&self) -> bool {
        !self.is_external()
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

    /// Character offset in source note
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

    /// Character offset in source note
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

/// Builder for ParsedNote to support migration and test compatibility
pub struct ParsedNoteBuilder {
    path: PathBuf,
    frontmatter: Option<Frontmatter>,
    wikilinks: Vec<Wikilink>,
    tags: Vec<Tag>,
    inline_links: Vec<InlineLink>,
    content: NoteContent,
    callouts: Vec<Callout>,
    latex_expressions: Vec<LatexExpression>,
    footnotes: FootnoteMap,
    parsed_at: Option<DateTime<Utc>>,
    content_hash: String,
    file_size: u64,
    parse_errors: Vec<ParseError>,
    block_hashes: Vec<BlockHash>,
    merkle_root: Option<BlockHash>,
}

impl ParsedNoteBuilder {
    /// Create a new builder with just the path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            frontmatter: None,
            wikilinks: Vec::new(),
            tags: Vec::new(),
            inline_links: Vec::new(),
            content: NoteContent::default(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: None,
            content_hash: String::new(),
            file_size: 0,
            parse_errors: Vec::new(),
            block_hashes: Vec::new(),
            merkle_root: None,
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

    /// Set inline links
    pub fn with_inline_links(mut self, inline_links: Vec<InlineLink>) -> Self {
        self.inline_links = inline_links;
        self
    }

    /// Set content
    pub fn with_content(mut self, content: NoteContent) -> Self {
        self.content = content;
        self
    }

    /// Set callouts
    pub fn with_callouts(mut self, callouts: Vec<Callout>) -> Self {
        self.callouts = callouts;
        self
    }

    /// Set LaTeX expressions
    pub fn with_latex_expressions(mut self, latex_expressions: Vec<LatexExpression>) -> Self {
        self.latex_expressions = latex_expressions;
        self
    }

    /// Set footnotes
    pub fn with_footnotes(mut self, footnotes: FootnoteMap) -> Self {
        self.footnotes = footnotes;
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

    /// Set block hashes (Phase 2)
    pub fn with_block_hashes(mut self, block_hashes: Vec<BlockHash>) -> Self {
        self.block_hashes = block_hashes;
        self
    }

    /// Set Merkle root (Phase 2)
    pub fn with_merkle_root(mut self, merkle_root: Option<BlockHash>) -> Self {
        self.merkle_root = merkle_root;
        self
    }

    /// Build the ParsedNote
    pub fn build(self) -> ParsedNote {
        ParsedNote {
            path: self.path,
            frontmatter: self.frontmatter,
            wikilinks: self.wikilinks,
            tags: self.tags,
            inline_links: self.inline_links,
            content: self.content,
            callouts: self.callouts,
            latex_expressions: self.latex_expressions,
            footnotes: self.footnotes,
            parsed_at: self.parsed_at.unwrap_or_else(Utc::now),
            content_hash: self.content_hash,
            file_size: self.file_size,
            parse_errors: self.parse_errors,
            block_hashes: self.block_hashes,
            merkle_root: self.merkle_root,
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
        let content = NoteContent::new().with_plain_text("Hello world test".to_string());
        assert_eq!(content.word_count, 3);
        assert_eq!(content.char_count, 16);
    }

    #[test]
    fn test_parsed_note_all_tags() {
        let mut doc = ParsedNote::new(PathBuf::from("test.md"));
        doc.tags = vec![Tag::new("rust", 0), Tag::new("ai", 10)];

        let yaml = "tags: [project, parsing]";
        doc.frontmatter = Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml));

        let all_tags = doc.all_tags();
        assert_eq!(all_tags.len(), 4);
        assert!(all_tags.contains(&"rust".to_string()));
        assert!(all_tags.contains(&"project".to_string()));
    }

    #[test]
    fn test_ast_block_creation() {
        let metadata = ASTBlockMetadata::heading(1, Some("test-heading".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Test Heading".to_string(),
            0,
            12,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Heading);
        assert_eq!(block.content, "Test Heading");
        assert_eq!(block.start_offset, 0);
        assert_eq!(block.end_offset, 12);
        assert_eq!(block.length(), 12);
        assert_eq!(block.content_length(), 12);
        assert!(!block.is_empty());
        assert_eq!(block.type_name(), "heading");
        assert!(block.is_heading_level(1));
        assert!(!block.is_heading_level(2));
    }

    #[test]
    fn test_ast_block_code_creation() {
        let metadata = ASTBlockMetadata::code(Some("rust".to_string()), 3);
        let block = ASTBlock::new(
            ASTBlockType::Code,
            "let x = 42;".to_string(),
            20,
            31,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Code);
        assert_eq!(block.content, "let x = 42;");
        assert!(block.is_code_language("rust"));
        assert!(!block.is_code_language("python"));
        assert_eq!(block.type_name(), "code");
    }

    #[test]
    fn test_ast_block_callout_creation() {
        let metadata =
            ASTBlockMetadata::callout("note".to_string(), Some("Important Note".to_string()), true);
        let block = ASTBlock::new(
            ASTBlockType::Callout,
            "This is an important note".to_string(),
            50,
            78,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Callout);
        assert!(block.is_callout_type("note"));
        assert!(!block.is_callout_type("warning"));
        assert_eq!(block.type_name(), "callout");
    }

    #[test]
    fn test_ast_block_hash_computation() {
        let content = "Test content";
        let metadata = ASTBlockMetadata::generic();
        let block1 = ASTBlock::new(
            ASTBlockType::Paragraph,
            content.to_string(),
            0,
            content.len(),
            metadata.clone(),
        );

        let block2 = ASTBlock::new(
            ASTBlockType::Paragraph,
            content.to_string(),
            10,
            10 + content.len(),
            metadata,
        );

        // Same content should produce same hash
        assert_eq!(block1.block_hash, block2.block_hash);
        assert_eq!(block1.block_hash.len(), 64); // BLAKE3 produces 32-byte hash = 64 hex chars
    }

    #[test]
    fn test_ast_block_with_explicit_hash() {
        let metadata = ASTBlockMetadata::generic();
        let block = ASTBlock::with_hash(
            ASTBlockType::Paragraph,
            "Test content".to_string(),
            0,
            12,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262".to_string(),
            metadata,
        );

        assert_eq!(
            block.block_hash,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn test_ast_block_type_display() {
        let metadata = ASTBlockMetadata::generic();

        let paragraph = ASTBlock::new(
            ASTBlockType::Paragraph,
            "Test".to_string(),
            0,
            4,
            metadata.clone(),
        );
        assert_eq!(paragraph.type_name(), "paragraph");

        let heading = ASTBlock::new(
            ASTBlockType::Heading,
            "Test".to_string(),
            0,
            4,
            metadata.clone(),
        );
        assert_eq!(heading.type_name(), "heading");

        let code = ASTBlock::new(ASTBlockType::Code, "Test".to_string(), 0, 4, metadata);
        assert_eq!(code.type_name(), "code");
    }

    #[test]
    fn test_ast_block_metadata_creation() {
        // Test all metadata creation methods
        let heading_meta = ASTBlockMetadata::heading(2, Some("test".to_string()));
        if let ASTBlockMetadata::Heading { level, id } = heading_meta {
            assert_eq!(level, 2);
            assert_eq!(id, Some("test".to_string()));
        } else {
            panic!("Expected heading metadata");
        }

        let code_meta = ASTBlockMetadata::code(Some("rust".to_string()), 10);
        if let ASTBlockMetadata::Code {
            language,
            line_count,
        } = code_meta
        {
            assert_eq!(language, Some("rust".to_string()));
            assert_eq!(line_count, 10);
        } else {
            panic!("Expected code metadata");
        }

        let list_meta = ASTBlockMetadata::list(ListType::Ordered, 5);
        if let ASTBlockMetadata::List {
            list_type,
            item_count,
        } = list_meta
        {
            assert_eq!(list_type, ListType::Ordered);
            assert_eq!(item_count, 5);
        } else {
            panic!("Expected list metadata");
        }

        let callout_meta =
            ASTBlockMetadata::callout("warning".to_string(), Some("Watch out".to_string()), true);
        if let ASTBlockMetadata::Callout {
            callout_type,
            title,
            is_standard_type,
        } = callout_meta
        {
            assert_eq!(callout_type, "warning");
            assert_eq!(title, Some("Watch out".to_string()));
            assert!(is_standard_type);
        } else {
            panic!("Expected callout metadata");
        }

        let latex_meta = ASTBlockMetadata::latex(true);
        if let ASTBlockMetadata::Latex { is_block } = latex_meta {
            assert!(is_block);
        } else {
            panic!("Expected latex metadata");
        }

        let generic_meta = ASTBlockMetadata::generic();
        matches!(generic_meta, ASTBlockMetadata::Generic);
    }

    #[test]
    fn test_parsed_note_with_block_hashes() {
        let mut doc = ParsedNote::new(PathBuf::from("test.md"));

        // Create some test block hashes
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let hash2 = BlockHash::new([
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
            0x3d, 0x3e, 0x3f, 0x40,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        // Test adding block hashes
        doc.add_block_hash(hash1);
        doc.add_block_hash(hash2);

        assert_eq!(doc.block_hash_count(), 2);
        assert!(doc.has_block_hashes());
        assert_eq!(doc.block_hashes[0], hash1);
        assert_eq!(doc.block_hashes[1], hash2);

        // Test setting Merkle root
        doc = doc.with_merkle_root(Some(merkle_root));
        assert!(doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), Some(merkle_root));

        // Test clearing hash data
        doc.clear_hash_data();
        assert!(!doc.has_block_hashes());
        assert!(!doc.has_merkle_root());
        assert_eq!(doc.block_hash_count(), 0);
    }

    #[test]
    fn test_parsed_note_builder_with_hashes() {
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        let doc = ParsedNote::builder(PathBuf::from("test.md"))
            .with_block_hashes(vec![hash1])
            .with_merkle_root(Some(merkle_root))
            .build();

        assert!(doc.has_block_hashes());
        assert_eq!(doc.block_hash_count(), 1);
        assert_eq!(doc.block_hashes[0], hash1);
        assert!(doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), Some(merkle_root));
    }

    #[test]
    fn test_parsed_note_backward_compatibility() {
        // Test that legacy constructor still works with empty hash fields
        let path = PathBuf::from("test.md");
        let frontmatter = None;
        let wikilinks = vec![];
        let tags = vec![];
        let content = NoteContent::new();
        let parsed_at = Utc::now();
        let content_hash = "test_hash".to_string();
        let file_size = 1024;

        let doc = ParsedNote::legacy(
            path.clone(),
            frontmatter,
            wikilinks,
            tags,
            content,
            parsed_at,
            content_hash.clone(),
            file_size,
        );

        // Legacy documents should have empty hash fields
        assert!(!doc.has_block_hashes());
        assert_eq!(doc.block_hash_count(), 0);
        assert!(!doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), None);

        // But other fields should still work
        assert_eq!(doc.path, path);
        assert_eq!(doc.content_hash, content_hash);
        assert_eq!(doc.file_size, file_size);
    }

    #[test]
    fn test_parsed_note_serialization_with_hashes() {
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        let original_doc = ParsedNote::builder(PathBuf::from("test.md"))
            .with_block_hashes(vec![hash1])
            .with_merkle_root(Some(merkle_root))
            .build();

        // Test JSON serialization
        let json = serde_json::to_string(&original_doc).expect("Failed to serialize");
        let deserialized_doc: ParsedNote =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Verify the fields are preserved
        assert!(deserialized_doc.has_block_hashes());
        assert_eq!(deserialized_doc.block_hash_count(), 1);
        assert_eq!(deserialized_doc.block_hashes[0], hash1);
        assert!(deserialized_doc.has_merkle_root());
        assert_eq!(deserialized_doc.get_merkle_root(), Some(merkle_root));
        assert_eq!(deserialized_doc.path, original_doc.path);
    }
}
