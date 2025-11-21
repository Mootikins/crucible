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
pub use coordinator::{
    factory as coordinator_factory, BatchOperationResult, BatchStatistics, CoordinatorConfig,
    CoordinatorStatistics, DefaultParserStorageCoordinator, OperationMetadata, OperationPriority,
    OperationResult, OperationType, ParserStorageCoordinator, ParsingOperation, TransactionContext,
};
pub use eav_document::{EAVDocument, EAVDocumentBuilder, ValidationError};
// Re-export ParserError and ParserResult from crucible-parser (canonical source)
pub use crucible_parser::error::{ParserError, ParserResult};
// Core-specific error types
pub use error::{ErrorSeverity, ParseError, ParseErrorType};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use frontmatter_mapper::FrontmatterPropertyMapper;
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
    // AST types (new in parser, not previously in core)
    ASTBlock,
    ASTBlockMetadata,
    ASTBlockType,

    // Hash type (parser's local copy to avoid circular dependency)
    BlockHash,
    Blockquote,
    // Enhanced content types
    Callout,
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
    LatexExpression,

    ListBlock,
    ListItem,
    ListType,
    NoteContent,
    Paragraph,
    // Core note types
    ParsedNote,
    ParsedNoteBuilder,
    // Additional content types (new in parser)
    Table,
    Tag,

    TaskStatus,

    // Link and tag types
    Wikilink,
};
