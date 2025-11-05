//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits and types for extracting structured
//! data from markdown files in the kiln.

pub mod adapter;
pub mod bridge;
pub mod coordinator;
pub mod error;
pub mod examples;
pub mod extensions;
pub mod latex;
pub mod pulldown;
pub mod query_blocks;
pub mod storage_bridge;
pub mod traits;
pub mod types;

pub use adapter::SurrealDBAdapter;
pub use bridge::{create_parser, create_parser_with_config, ParserAdapter, ParserConfig};
pub use coordinator::{
    ParserStorageCoordinator, DefaultParserStorageCoordinator, ParsingOperation,
    OperationResult, BatchOperationResult, OperationType, OperationPriority,
    OperationMetadata, TransactionContext, CoordinatorConfig, CoordinatorStatistics,
    BatchStatistics, factory as coordinator_factory,
};
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
};
pub use latex::{create_latex_extension, LatexExtension};
pub use pulldown::PulldownParser;
pub use query_blocks::{create_query_block_extension, QueryBlockExtension};
pub use storage_bridge::{
    StorageAwareMarkdownParser, StorageAwareParser, StorageAwareParseResult,
    StorageAwareParserConfig, ParseStatistics, StorageOperationResult,
    factory as parser_factory,
};
pub use traits::{MarkdownParser, ParserCapabilities};
pub use types::{
    Callout, CodeBlock, DocumentContent, FootnoteDefinition, FootnoteMap, FootnoteReference,
    Frontmatter, FrontmatterFormat, Heading, LatexExpression, ListBlock, ListItem, ListType,
    Paragraph, ParsedDocument, ParsedDocumentBuilder, Tag, TaskStatus, Wikilink,
};
