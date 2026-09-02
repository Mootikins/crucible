//! ParsedNote and related types

use super::{
    Callout, FootnoteMap, Frontmatter, InlineLink, LatexExpression, NoteContent, Tag, Wikilink,
};
use crate::parser::error::ParseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

    /// BYTE offset of the parsed body within the original file — the length
    /// of the frontmatter block (delimiters included), 0 when there is none.
    /// All extension offsets/spans (wikilinks, tags, …) are body-relative;
    /// add this to get file-absolute byte positions (needed by the wikilink
    /// rewrite engine, which splices raw file bytes).
    #[serde(default)]
    pub body_offset: usize,

    /// Structural metadata extracted during parsing
    ///
    /// These are deterministic counts computed from the AST structure,
    /// available immediately after parsing without requiring enrichment.
    /// Follows industry standard pattern (Unified/Remark, Pandoc, Elasticsearch).
    #[serde(default)]
    pub metadata: ParsedNoteMetadata,
}

/// Structural metadata extracted during parsing
///
/// Contains only deterministic metrics computed from AST structure.
/// Computed metadata (complexity, reading time) lives in enrichment layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedNoteMetadata {
    /// Total word count across entire document
    pub word_count: usize,

    /// Total character count (excluding whitespace)
    pub char_count: usize,

    /// Number of heading elements (all levels)
    pub heading_count: usize,

    /// Number of code block elements
    pub code_block_count: usize,

    /// Number of list elements (ordered + unordered)
    pub list_count: usize,

    /// Number of paragraphs
    pub paragraph_count: usize,

    /// Number of callouts (Obsidian-style)
    pub callout_count: usize,

    /// Number of LaTeX expressions
    pub latex_count: usize,

    /// Number of footnotes
    pub footnote_count: usize,
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
    body_offset: usize,
    metadata: ParsedNoteMetadata,
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
            body_offset: 0,
            metadata: ParsedNoteMetadata::default(),
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

    /// Set structural metadata
    pub fn with_metadata(mut self, metadata: ParsedNoteMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the byte offset of the body within the original file
    pub fn with_body_offset(mut self, body_offset: usize) -> Self {
        self.body_offset = body_offset;
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
            body_offset: self.body_offset,
            metadata: self.metadata,
        }
    }
}
