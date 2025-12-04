//! Crucible Markdown Parser
//!
//! A modular, extensible markdown parser for knowledge management systems.
//! This crate provides implementations of the parser traits defined in crucible-core.
//!
//! # Dependency Inversion
//!
//! This crate depends on `crucible-core` for:
//! - Parser trait definitions (MarkdownParser, ParserCapabilities)
//! - Parser type definitions (ParsedNote, Wikilink, Tag, etc.)
//! - Parser error types (ParserError, ParseError, etc.)
//!
//! This crate provides:
//! - Concrete parser implementations (MarkdownItParser)
//! - Extension system for syntax features
//! - Block extraction and processing utilities

// markdown-it specific modules
#[cfg(feature = "markdown-it-parser")]
pub mod basic_markdown_it;

pub mod block_extractor;
pub mod block_hasher;
pub mod blockquotes;

// Test utilities - always available for use in other crates' tests
pub mod test_utils;
pub mod callouts;
pub mod enhanced_tags;
pub mod error;
pub mod extensions;
pub mod footnotes;
pub mod frontmatter_extractor;
pub mod implementation;
pub mod inline_links;
pub mod latex;
pub mod traits;
pub mod types;
pub mod wikilinks;

// markdown-it based parser (default)
#[cfg(feature = "markdown-it-parser")]
pub mod markdown_it;

// Re-export core parser types and traits (canonical definitions in crucible-core)
pub use crucible_core::parser::{
    // Core types
    ASTBlock,
    ASTBlockMetadata,
    ASTBlockType,
    BlockHash,
    Blockquote,
    Callout,
    CodeBlock,
    // Error types
    ErrorSeverity,
    FootnoteDefinition,
    FootnoteMap,
    FootnoteReference,
    Frontmatter,
    FrontmatterFormat,
    Heading,
    HorizontalRule,
    InlineLink,
    LatexExpression,
    ListBlock,
    ListItem,
    ListMarkerStyle,
    ListType,
    // Trait definitions
    MarkdownParser,
    NoteContent,
    Paragraph,
    ParseError,
    ParseErrorType,
    ParsedNote,
    ParsedNoteBuilder,
    ParsedNoteMetadata,
    ParserCapabilities,
    ParserCapabilitiesExt,
    ParserError,
    ParserRequirements,
    ParserResult,
    Table,
    Tag,
    TaskStatus,
    Wikilink,
};

// Re-export implementation types
pub use block_extractor::{BlockExtractor, ExtractionConfig};
pub use block_hasher::SimpleBlockHasher;
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use frontmatter_extractor::{
    extract_frontmatter, ExtractionStats, FrontmatterExtractor, FrontmatterExtractorConfig,
    FrontmatterResult, LineEndingStyle,
};
pub use implementation::{BlockProcessingConfig, CrucibleParser};

// Re-export markdown-it parser when feature is enabled (default)
#[cfg(feature = "markdown-it-parser")]
pub use markdown_it::MarkdownItParser;

// Convenience factory functions
#[cfg(feature = "markdown-it-parser")]
pub use basic_markdown_it::create_basic_markdown_it_extension;
pub use blockquotes::create_blockquote_extension;
pub use callouts::create_callout_extension;
pub use enhanced_tags::create_enhanced_tags_extension;
pub use footnotes::create_footnote_extension;
pub use inline_links::create_inline_link_extension;
pub use latex::create_latex_extension;
pub use wikilinks::create_wikilink_extension;
