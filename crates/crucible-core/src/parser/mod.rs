//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits, types, and implementations
//! for extracting structured data from markdown files in the kiln.
//!
//! # Module Organization
//!
//! - `traits` - Core parser trait (`MarkdownParser`)
//! - `types` - All parser data types (`ParsedNote`, `Wikilink`, `Tag`, etc.)
//! - `error` - Parser error types
//! - `extensions` - Syntax extension system
//! - `implementation` - Main `CrucibleParser` implementation
//! - `block_extractor` - AST block extraction
//! - `block_hasher` - Block-level hashing
//! - `frontmatter_extractor` - Frontmatter parsing utilities
//! - `markdown_it` - markdown-it based parser (feature-gated)
//! - Extension modules: `wikilinks`, `callouts`, `blockquotes`, etc.

pub mod error;
pub mod extensions;
pub mod traits;
pub mod types;

// Parser implementation modules (absorbed from crucible-parser)
#[cfg(feature = "markdown-it-parser")]
pub mod basic_markdown_it;
pub mod block_extractor;
pub mod block_hasher;
pub mod blockquotes;
pub mod callouts;
pub mod enhanced_tags;
pub mod footnotes;
pub mod frontmatter_extractor;
pub mod implementation;
pub mod inline_links;
pub mod latex;
pub mod test_utils;
pub mod wikilinks;

// markdown-it based parser (default)
#[cfg(feature = "markdown-it-parser")]
pub mod markdown_it;

// Re-export error types
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionCapabilities, ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats,
    SyntaxExtension,
};
pub use traits::{MarkdownParser, ParserCapabilities, ParserRequirements};

// Re-export implementation types
pub use block_extractor::{BlockExtractor, ExtractionConfig};
pub use block_hasher::SimpleBlockHasher;
pub use frontmatter_extractor::{
    extract_frontmatter, ExtractionStats, FrontmatterExtractor, FrontmatterExtractorConfig,
    FrontmatterResult, LineEndingStyle,
};
pub use implementation::{BlockProcessingConfig, CrucibleParser};

// Re-export markdown-it parser when feature is enabled (default)
#[cfg(feature = "markdown-it-parser")]
pub use basic_markdown_it::create_basic_markdown_it_extension;
#[cfg(feature = "markdown-it-parser")]
pub use markdown_it::MarkdownItParser;

// Convenience factory functions
pub use blockquotes::create_blockquote_extension;
pub use callouts::create_callout_extension;
pub use enhanced_tags::create_enhanced_tags_extension;
pub use footnotes::create_footnote_extension;
pub use inline_links::create_inline_link_extension;
pub use latex::create_latex_extension;
pub use wikilinks::create_wikilink_extension;

// Re-export parser types from canonical source (this module)
pub use types::{
    // AST types
    ASTBlock,
    ASTBlockMetadata,
    ASTBlockType,
    // Hash type
    BlockHash,
    Blockquote,
    // Enhanced content types
    Callout,
    CalloutType,
    CheckboxStatus,
    CodeBlock,
    FootnoteDefinition,
    // Footnote types
    FootnoteMap,
    FootnoteReference,
    Frontmatter,
    FrontmatterFormat,
    // Content structure types
    Heading,
    HorizontalRule,
    InlineLink,
    // Inline metadata
    InlineMetadata,
    LatexExpression,
    ListBlock,
    ListItem,
    ListMarkerStyle,
    ListType,
    NoteContent,
    Paragraph,
    // Core note types
    ParsedNote,
    ParsedNoteBuilder,
    ParsedNoteMetadata,
    // Additional content types
    Table,
    Tag,
    // Task file type
    TaskFile,
    // Task graph type
    TaskGraph,
    // Task types
    TaskItem,
    TaskStatus,
    // Link and tag types
    Wikilink,
};
