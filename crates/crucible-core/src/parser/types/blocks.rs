//! Block types: tables, blockquotes, and horizontal rules

use super::{BlockHash, CalloutType};
use serde::{Deserialize, Serialize};

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
    /// An Obsidian-style callout, `> [!type] ...`.
    Callout {
        /// The type named in the marker.
        callout_type: CalloutType,
    },
    /// A display formula, `$$ ... $$` on its own lines.
    Latex,
    /// A table.
    Table,
    /// A thematic break.
    HorizontalRule,
    /// A synthetic row over the passage between two adjacent blocks.
    ///
    /// The parser never produces one. The `index:blocks` stage is its only
    /// producer, so retrieval can score the trajectory between two blocks as
    /// a row of its own.
    Transition,
}

impl BlockKind {
    /// The kind's stable name, as stored and as shown to a reader.
    ///
    /// One word per kind. A callout's own type is not folded in here: the
    /// name says what the block is, not which flavour of callout.
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockKind::Heading { .. } => "heading",
            BlockKind::Paragraph => "paragraph",
            BlockKind::Code { .. } => "code",
            BlockKind::List { .. } => "list",
            BlockKind::Blockquote => "quote",
            BlockKind::Callout { .. } => "callout",
            BlockKind::Latex => "latex",
            BlockKind::Table => "table",
            BlockKind::HorizontalRule => "rule",
            BlockKind::Transition => "transition",
        }
    }

    /// Every name [`Self::as_str`] can answer. The `index:blocks` stage
    /// admits an extra row only under one of these.
    pub const STORED_NAMES: &'static [&'static str] = &[
        "heading",
        "paragraph",
        "code",
        "list",
        "quote",
        "callout",
        "latex",
        "table",
        "rule",
        "transition",
    ];
}

impl std::fmt::Display for BlockKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
