//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits, types, and implementations
//! for extracting structured data from markdown files in the kiln.
//!
//! # Module Organization
//!
//! - `traits` - Parser capabilities and requirements
//! - `types` - All parser data types (`ParsedNote`, `Wikilink`, `Tag`, etc.)
//! - `error` - Parser error types
//! - `extensions` - Syntax extension system
//! - `implementation` - Main `CrucibleParser` implementation
//! - `frontmatter_extractor` - Frontmatter parsing utilities
//! - `markdown_it` - markdown-it AST converter + syntax plugins (feature-gated)
//! - Extension modules: `wikilinks`, `latex`, `inline_links`, etc.

pub mod error;
pub mod extensions;
pub mod traits;
pub mod types;

// Parser implementation modules (absorbed from crucible-parser)
#[cfg(feature = "markdown-it-parser")]
pub mod basic_markdown_it;
pub mod enhanced_tags;
pub mod frontmatter_extractor;
pub mod implementation;
pub mod inline_links;
pub mod latex;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod wikilinks;

// AST converter + custom syntax plugins backing basic_markdown_it
#[cfg(feature = "markdown-it-parser")]
pub mod markdown_it;

// Re-export error types
pub use error::{ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult};
pub use extensions::{Extension, ExtensionRegistry};
pub use traits::ParserCapabilities;

// Re-export implementation types
pub use frontmatter_extractor::{
    extract_frontmatter, FrontmatterExtractor, FrontmatterExtractorConfig, FrontmatterResult,
    LineEndingStyle,
};
pub use implementation::CrucibleParser;

// Re-export parser types from canonical source (this module)
pub use types::{
    Block,
    // AST types
    // Hash type
    BlockHash, // Enhanced content types
    BlockKind,
    CalloutType,
    CheckboxStatus,
    Frontmatter,
    FrontmatterFormat, // Workflow types
    Gate,              // Content structure types
    InlineLink,        // Inline metadata
    InlineMetadata,
    LatexExpression,
    NoteContent, // Core note types
    ParsedNote,
    ParsedNoteBuilder,
    ParsedNoteMetadata, // Additional content types
    Tag,                // Task file type
    TaskFile,           // Task graph type
    TaskGraph,          // Task types
    TaskItem,
    ValidationEntry, // Link and tag types
    Wikilink,
    WorkflowDoc,
    WorkflowParseWarning,
    WorkflowStep,
};
