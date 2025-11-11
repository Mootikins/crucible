//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits and types for extracting structured
//! data from markdown files in the kiln.

pub mod adapter;
pub mod bridge;
pub mod coordinator;
pub mod eav_document;
pub mod error;
pub mod examples;
pub mod extensions;
pub mod frontmatter_mapper;
pub mod latex;
pub mod pulldown;
pub mod query_blocks;
pub mod storage_bridge;
pub mod traits;

pub use adapter::SurrealDBAdapter;
pub use bridge::{create_parser, create_parser_with_config, ParserAdapter, ParserConfig};
pub use eav_document::{EAVDocument, EAVDocumentBuilder, ValidationError};
pub use frontmatter_mapper::FrontmatterPropertyMapper;
pub use coordinator::{
    factory as coordinator_factory, BatchOperationResult, BatchStatistics, CoordinatorConfig,
    CoordinatorStatistics, DefaultParserStorageCoordinator, OperationMetadata, OperationPriority,
    OperationResult, OperationType, ParserStorageCoordinator, ParsingOperation, TransactionContext,
};
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use latex::{create_latex_extension, LatexExtension};
pub use pulldown::PulldownParser;
pub use query_blocks::{create_query_block_extension, QueryBlockExtension};
pub use storage_bridge::{
    factory as parser_factory, ParseStatistics, StorageAwareMarkdownParser,
    StorageAwareParseResult, StorageAwareParser, StorageAwareParserConfig, StorageOperationResult,
};
pub use traits::{MarkdownParser, ParserCapabilities};

// Re-export parser types for convenience
// Canonical definitions are in crucible-parser crate
pub use crucible_parser::types::{
    // Core note types
    ParsedNote,
    ParsedNoteBuilder,
    NoteContent,
    Frontmatter,
    FrontmatterFormat,

    // Link and tag types
    Wikilink,
    InlineLink,
    Tag,

    // Content structure types
    Heading,
    CodeBlock,
    Paragraph,
    ListBlock,
    ListItem,
    ListType,
    TaskStatus,

    // Enhanced content types
    Callout,
    LatexExpression,

    // Footnote types
    FootnoteMap,
    FootnoteDefinition,
    FootnoteReference,

    // AST types (new in parser, not previously in core)
    ASTBlock,
    ASTBlockMetadata,
    ASTBlockType,

    // Additional content types (new in parser)
    Table,
    Blockquote,
    HorizontalRule,

    // Hash type (parser's local copy to avoid circular dependency)
    BlockHash,
};
