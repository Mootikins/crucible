//! Block types: tables, blockquotes, and horizontal rules

use super::BlockHash;
use serde::{Deserialize, Serialize};

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
    pub fn new(
        raw_content: String,
        headers: Vec<String>,
        columns: usize,
        rows: usize,
        offset: usize,
    ) -> Self {
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

    /// Get the length of the horizontal rule
    pub fn length(&self) -> usize {
        self.raw_content.len()
    }
}

/// What kind of block this is, with the little that distinguishes each kind.
///
/// Richer per-kind detail (list items, table headers) stays on the typed
/// collections beside `NoteContent::blocks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    /// A heading, with its level (1-6).
    Heading {
        /// Heading level, 1 for `#` through 6 for `######`.
        level: u8,
    },
    /// A paragraph of prose.
    Paragraph,
    /// A fenced or indented code block.
    Code {
        /// The fence's info string, when it names one.
        language: Option<String>,
    },
    /// A bullet or numbered list.
    List {
        /// True for a numbered list.
        ordered: bool,
    },
    /// A block quote.
    Blockquote,
    /// A display formula, `$$ ... $$` on its own lines.
    Latex,
    /// A table.
    Table,
    /// A thematic break.
    HorizontalRule,
}

/// One top-level block of a note, in document order.
///
/// The parser emits these in one pass over the markdown-it tree, so their
/// order is the document's order and their spans come from the source map
/// rather than from arithmetic over stripped text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// What kind of block this is.
    pub kind: BlockKind,

    /// The block's text, with markdown syntax stripped.
    pub text: String,

    /// Byte offset where the block starts, relative to the note body.
    pub start_offset: usize,

    /// Byte offset one past the block's last byte, relative to the body.
    pub end_offset: usize,

    /// BLAKE3 of the block's source bytes, `body[start_offset..end_offset]`.
    ///
    /// This is a **reuse** key, not an identity. Two identical blocks hash
    /// alike on purpose, so one embedding serves both, in this note and in
    /// every other. Identity is the span: no two top-level blocks of a note
    /// begin at the same byte.
    pub content_hash: BlockHash,
}

impl Block {
    /// Create a block, hashing the source it spans.
    pub fn new(
        kind: BlockKind,
        text: String,
        start_offset: usize,
        end_offset: usize,
        source: &str,
    ) -> Self {
        let bytes = source
            .as_bytes()
            .get(start_offset..end_offset)
            .unwrap_or_default();
        Self {
            kind,
            text,
            start_offset,
            end_offset,
            content_hash: BlockHash::new(*blake3::hash(bytes).as_bytes()),
        }
    }

    /// Count the words in the block's text.
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}
