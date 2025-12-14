//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits and types for extracting structured
//! data from markdown files in the kiln.
//!
//! # Dependency Inversion Principle
//!
//! This module defines the **canonical** parser abstractions and types:
//! - `traits::MarkdownParser` - Core parser trait
//! - `types::*` - All parser data types (ParsedNote, Wikilink, Tag, etc.)
//! - `error::*` - Parser error types
//!
//! The `crucible-parser` crate depends on these types and provides implementations.

pub mod adapter;
// TODO: bridge module disabled - it depends on crucible_parser implementation
// Should be moved to a higher-level crate that depends on both core and parser
// pub mod bridge;
pub mod coordinator;
pub mod eav_document;
pub mod error;
pub mod extensions;
pub mod frontmatter_mapper;
pub mod storage_bridge;
pub mod traits;
pub mod types;

pub use adapter::SurrealDBAdapter;
// pub use bridge::{create_parser, create_parser_with_config, ParserAdapter, ParserConfig};
pub use coordinator::{
    factory as coordinator_factory, BatchOperationResult, BatchStatistics, CoordinatorConfig,
    CoordinatorStatistics, DefaultParserStorageCoordinator, OperationMetadata, OperationPriority,
    OperationResult, OperationType, ParserStorageCoordinator, ParsingOperation, TransactionContext,
};
pub use eav_document::{EAVDocument, EAVDocumentBuilder, ValidationError};
// Re-export error types from canonical source (this module)
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use frontmatter_mapper::FrontmatterPropertyMapper;
pub use storage_bridge::{
    factory as parser_factory, ParseStatistics, StorageAwareMarkdownParser,
    StorageAwareParseResult, StorageAwareParser, StorageAwareParserConfig, StorageOperationResult,
};
pub use traits::{MarkdownParser, ParserCapabilities, ParserCapabilitiesExt, ParserRequirements};

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
    // Task types
    TaskItem,
    TaskStatus,
    // Task file type
    TaskFile,
    // Task graph type
    TaskGraph,
    // Link and tag types
    Wikilink,
};
