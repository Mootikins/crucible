//! Parser data types
//!
//! This module re-exports parser types from crucible-core (canonical source).
//! See crucible-core::parser::types for the actual definitions.

// Re-export all types from canonical source
pub use crucible_core::parser::types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, BlockHash, Blockquote, Callout, CalloutType,
    CheckboxStatus, CodeBlock, FootnoteDefinition, FootnoteMap, FootnoteReference, Frontmatter,
    FrontmatterFormat, Heading, HorizontalRule, InlineLink, LatexExpression, ListBlock, ListItem,
    ListMarkerStyle, ListType, NoteContent, Paragraph, ParseError, ParsedNote, ParsedNoteBuilder,
    ParsedNoteMetadata, Table, Tag, TaskStatus, Wikilink,
};
