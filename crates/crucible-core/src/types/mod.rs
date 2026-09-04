//! Core domain types for Crucible
//!
//! This module contains pure data structures used throughout the Crucible system.
//! Types are organized by domain concern and kept free of implementation logic.
//!
//! ## Organization
//!
//! Domain types are currently defined in their respective modules:
//! - ACP types: `acp` (FileDiff); callers import them by the `types::acp` path
//! - Parser types: `parser::types` (ParsedNote, Wikilink, Tag, etc.)
//! - Database types: `types::database` (SearchResult, DocumentId, Record, etc.)
//! - Hash type: `parser::types::BlockHash`, the one content hash
//!
//! This module serves as a central re-export point for types that cross module boundaries.

pub mod acp;
pub mod database;
pub mod mcp_status;
pub mod mode;
pub mod notification;
pub mod plugin_status;
pub mod popup;
pub mod provider_info;
pub mod tool_display;
pub mod tool_ref;
pub mod undo;
// Re-export parser domain types
pub use crate::parser::types::{
    BlockHash, Frontmatter, FrontmatterFormat, NoteContent, ParsedNote, Tag, Wikilink,
};

// Re-export database domain types (canonical definitions in types::database)
pub use self::database::{DocumentId, QueryResult, Record, RecordId, SearchResult};

// Re-export ACP schema types from agent-client-protocol-schema
pub use crate::types::acp::schema::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionMode, SessionModeId,
    SessionModeState,
};

// Re-export mode descriptor types
pub use crate::types::mode::{canonical_mode_id, default_internal_modes, ModeDescriptor};

// Re-export trait types (these are associated with traits but used as data)
pub use crate::traits::tools::{ExecutionContext, ToolDefinition, ToolExample};
pub use crate::types::database::{Record as StorageRecord, RecordId as StorageRecordId};

// Re-export tool reference types
pub use crate::types::tool_display::{ToolDisplay, ToolDisplayKind};
pub use crate::types::tool_ref::{ToolRef, ToolSource};

// Re-export popup types
pub use crate::types::popup::PopupEntry;

// Re-export undo types
pub use crate::types::undo::UndoSummary;

// Re-export notification types
pub use crate::types::notification::{
    Notification, NotificationKind, NotificationQueue, NotificationScope,
};

// Re-export provider info (used by daemon RPC and session-setup events)
pub use crate::types::provider_info::ProviderInfo;

// Re-export plugin status entry (used by session-setup events)
pub use crate::types::plugin_status::PluginStatusEntry;

// NOTE: `mcp_status::McpServerInfo` is intentionally NOT re-exported at
// `types::` top-level to avoid collision with `traits::mcp::McpServerInfo`
// (distinct: protocol-level identity vs. display-oriented status).
