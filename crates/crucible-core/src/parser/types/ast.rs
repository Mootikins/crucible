//! AST block types for semantic markdown structure

use super::ListType;
use serde::{Deserialize, Serialize};

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
