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
pub mod error;
pub mod extensions;
pub mod traits;
pub mod types;

pub use adapter::SurrealDBAdapter;
// Re-export error types from canonical source (this module)
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{
    ExtensionRegistry, ExtensionRegistryBuilder, ExtensionRegistryStats, SyntaxExtension,
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
