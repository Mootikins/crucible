//! Crucible Markdown Parser
//!
//! A modular, extensible markdown parser for knowledge management systems.
//! This crate provides:
//! - Obsidian-compatible syntax extensions
//! - Plugin-based architecture
//! - Dependency inversion support for testing
//! - High-performance parsing with sub-100ms target

pub mod basic_markdown;
pub mod block_extractor;
pub mod block_hasher;
pub mod blockquotes;
pub mod callouts;
pub mod enhanced_tags;
pub mod error;
pub mod extensions;
pub mod footnotes;
pub mod implementation;
pub mod inline_links;
pub mod latex;
pub mod traits;
pub mod types;

// Re-export main types for convenience
pub use block_extractor::{BlockExtractor, ExtractionConfig};
pub use block_hasher::SimpleBlockHasher;
pub use error::{ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use implementation::{BlockProcessingConfig, CrucibleParser};
pub use traits::{MarkdownParserImplementation, ParserCapabilities};
pub use types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, BlockHash, Blockquote, Callout, CodeBlock,
    NoteContent, FootnoteDefinition, FootnoteMap, FootnoteReference, Frontmatter,
    FrontmatterFormat, Heading, HorizontalRule, InlineLink, LatexExpression, ListBlock, ListItem,
    ListType, ParsedNote, ParsedNoteBuilder, Table, Tag, TaskStatus, Wikilink,
};

// Convenience factory functions
pub use basic_markdown::create_basic_markdown_extension;
pub use blockquotes::create_blockquote_extension;
pub use callouts::create_callout_extension;
pub use enhanced_tags::create_enhanced_tags_extension;
pub use footnotes::create_footnote_extension;
pub use inline_links::create_inline_link_extension;
pub use latex::create_latex_extension;
