//! Note content structure and basic block types

use super::{Block, InlineLink, LatexExpression, Tag, Wikilink};
use serde::{Deserialize, Serialize};

/// Parsed note content structure
///
/// The extensions write the extracted lists (`wikilinks`, `tags`,
/// `inline_links`, `latex_expressions`) here while
/// they run. `parse_content` then moves them to `ParsedNote`. Read them from
/// the note; on a parsed note these copies are empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteContent {
    /// Plain text content (markdown syntax stripped)
    ///
    /// Limited to first 1000 characters for search preview.
    /// Full content remains on disk.
    pub plain_text: String,

    /// Every top-level block, in document order, with source-map spans.
    ///
    /// The typed collections below are views of the same document split by
    /// kind. They carry no order between kinds; this one does.
    #[serde(default)]
    pub blocks: Vec<Block>,

    /// Inline markdown links [text](url)
    pub inline_links: Vec<InlineLink>,

    /// Wikilinks [[note]] extracted from content
    pub wikilinks: Vec<Wikilink>,

    /// Tags #tag extracted from content
    pub tags: Vec<Tag>,

    /// LaTeX mathematical expressions extracted from content
    pub latex_expressions: Vec<LatexExpression>,

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
            blocks: Vec::new(),
            inline_links: Vec::new(),
            wikilinks: Vec::new(),
            tags: Vec::new(),
            latex_expressions: Vec::new(),
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
}
