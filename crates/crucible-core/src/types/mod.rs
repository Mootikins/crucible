//! Core domain types for Crucible
//!
//! This module contains pure data structures used throughout the Crucible system.
//! Types are organized by domain concern and kept free of implementation logic.
//!
//! ## Organization
//!
//! Domain types are currently defined in their respective modules:
//! - ACP types: `acp` (SessionConfig, SessionId, ToolInvocation, etc.)
//! - Parser types: `parser::types` (ParsedNote, Wikilink, Tag, etc.)
//! - Database types: `database` (Record, QueryResult, Node, Edge, Note, etc.)
//! - Note types: `note` (NoteNode, ViewportState)
//! - Property types: `properties` (PropertyMap, AttributeValue)
//! - Hashing types: `hashing` (FileHash, BlockHash, HashAlgorithm, etc.)
//!
//! This module serves as a central re-export point for types that cross module boundaries.

pub mod acp;
pub mod grammar;
pub mod hashing;
pub mod mode;
pub mod notification;
pub mod popup;
pub mod tool_ref;
pub mod undo_tree;

// Re-export parser domain types
pub use crate::parser::types::{
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
    SearchResult,
    UnifiedSearchResult,
};

// Re-export note types
pub use crate::note::{NoteNode, ViewportState};

// Re-export property types
pub use crate::properties::{AttributeValue, PropertyMap};

// Re-export hashing types
pub use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

// Re-export change detection types
pub use crate::traits::change_detection::{ChangeSet, ChangeSummary};

// Re-export ACP types
// NOTE: ToolDescriptor and ToolExample removed - use ToolDefinition from traits::tools
pub use crate::types::acp::{
    ChunkType, FileDiff, FileMetadata, SessionConfig, SessionId, StreamChunk, StreamMetadata,
    ToolCallInfo, ToolInvocation, ToolOutput,
};

// Re-export ACP schema types from agent-client-protocol-schema
pub use crate::types::acp::schema::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionMode, SessionModeId,
    SessionModeState,
};

// Re-export mode descriptor types
pub use crate::types::mode::{default_internal_modes, ModeDescriptor};

// Re-export trait types (these are associated with traits but used as data)
pub use crate::traits::storage::{Record as StorageRecord, RecordId as StorageRecordId};
pub use crate::traits::tools::{ExecutionContext, ToolDefinition, ToolExample};

// Re-export grammar types
pub use crate::types::grammar::{Grammar, GrammarError, GrammarResult};

// Re-export tool reference types
pub use crate::types::tool_ref::{ToolRef, ToolSource};

// Re-export popup types
pub use crate::types::popup::PopupEntry;

// Re-export undo tree types
pub use crate::types::undo_tree::{
    NodeId as UndoNodeId, TreeNode, TreeNodeLabel, TreeSummary, UndoTree,
};

// Re-export notification types
pub use crate::types::notification::{Notification, NotificationKind, NotificationQueue};
