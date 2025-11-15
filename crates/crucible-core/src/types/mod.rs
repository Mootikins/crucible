//! Core domain types for Crucible
//!
//! This module contains pure data structures used throughout the Crucible system.
//! Types are organized by domain concern and kept free of implementation logic.
//!
//! ## Organization
//!
//! Domain types are currently defined in their respective modules:
//! - Parser types: `parser::types` (ParsedNote, Wikilink, Tag, etc.)
//! - Database types: `database` (Record, QueryResult, Node, Edge, Note, etc.)
//! - Note types: `note` (NoteNode, ViewportState)
//! - Property types: `properties` (PropertyMap, PropertyValue)
//! - Hashing types: `hashing` (FileHash, BlockHash, HashAlgorithm, etc.)
//!
//! This module serves as a central re-export point for types that cross module boundaries.

pub mod hashing;

// Re-export parser domain types
pub use crucible_parser::types::{
    CodeBlock, Frontmatter, FrontmatterFormat, Heading, ListBlock, ListItem, ListType, NoteContent,
    Paragraph, ParsedNote, Tag, TaskStatus, Wikilink,
};

// Re-export database domain types
pub use crate::database::{
    // Additional types
    BatchResult,
    // Note: Other database types remain in database module for now
    // Core types
    DbError,
    DbResult,
    // Graph types
    Direction,
    DocumentId,
    DocumentMetadata,
    Edge,
    EdgeId,
    Node,
    NodeId,
    // Note types
    Note,
    QueryResult,
    Record,
    RecordId,
};

// Re-export note types
pub use crate::note::{NoteNode, ViewportState};

// Re-export property types
pub use crate::properties::{PropertyMap, PropertyValue};

// Re-export hashing types
pub use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

// Re-export change detection types
pub use crate::traits::change_detection::{ChangeSet, ChangeSummary};

// Re-export trait types (these are associated with traits but used as data)
pub use crate::traits::storage::{Record as StorageRecord, RecordId as StorageRecordId};
pub use crate::traits::tools::{ExecutionContext, ToolDefinition, ToolExample};
