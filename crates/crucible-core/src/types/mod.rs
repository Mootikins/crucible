//! Core domain types for Crucible
//!
//! This module contains pure data structures used throughout the Crucible system.
//! Types are organized by domain concern and kept free of implementation logic.
//!
//! ## Organization
//!
//! Domain types are currently defined in their respective modules:
//! - Parser types: `parser::types` (ParsedDocument, Wikilink, Tag, etc.)
//! - Database types: `database` (Record, QueryResult, Node, Edge, Document, etc.)
//! - Document types: `document` (DocumentNode, ViewportState)
//! - Property types: `properties` (PropertyMap, PropertyValue)
//!
//! This module serves as a central re-export point for types that cross module boundaries.

// Re-export parser domain types
pub use crate::parser::types::{
    CodeBlock, DocumentContent, Frontmatter, FrontmatterFormat, Heading, ListBlock, ListItem,
    ListType, Paragraph, ParsedDocument, Tag, TaskStatus, Wikilink,
};

// Re-export database domain types
pub use crate::database::{
    // Core types
    DbError,
    DbResult,
    QueryResult,
    Record,
    RecordId,
    // Graph types
    Direction,
    Edge,
    EdgeId,
    Node,
    NodeId,
    // Document types
    Document,
    DocumentId,
    DocumentMetadata,
    // Additional types
    BatchResult,
    // Note: Other database types remain in database module for now
};

// Re-export document types
pub use crate::document::{DocumentNode, ViewportState};

// Re-export property types
pub use crate::properties::{PropertyMap, PropertyValue};

// Re-export trait types (these are associated with traits but used as data)
pub use crate::traits::storage::{Record as StorageRecord, RecordId as StorageRecordId};
pub use crate::traits::tools::{ExecutionContext, ToolDefinition, ToolExample};
