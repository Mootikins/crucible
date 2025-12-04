//! Note content structure and basic block types

use super::{
    Blockquote, Callout, FootnoteMap, HorizontalRule, InlineLink, LatexExpression, ListBlock,
    Table, Tag, Wikilink,
};
use serde::{Deserialize, Serialize};

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

    /// Wikilinks [[note]] extracted from content
    pub wikilinks: Vec<Wikilink>,

    /// Tags #tag extracted from content
    pub tags: Vec<Tag>,

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
            wikilinks: Vec::new(),
            tags: Vec::new(),
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
