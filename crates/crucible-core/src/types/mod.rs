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
//! - Database types: `types::database` (SearchResult, Record, etc.)
//! - Hash type: `parser::types::BlockHash`, the one content hash
//!
//! This module serves as a central re-export point for types that cross module boundaries.

pub mod acp;
pub mod command;
pub mod command_effect;
pub mod database;
pub mod knob;
pub mod mcp_status;
pub mod mode;
pub mod notification;
pub mod plugin_reply;
pub mod plugin_status;
pub mod popup;
pub mod provider_info;
pub mod skill;
pub mod status_item;
pub mod surface;
pub mod tool_call;
pub mod tool_match;
pub mod tool_ref;
pub mod undo;
// Re-export parser domain types
pub use crate::parser::types::{
    BlockHash, Frontmatter, FrontmatterFormat, NoteContent, ParsedNote, Tag, Wikilink,
};

// Re-export database domain types (canonical definitions in types::database)
pub use self::database::SearchResult;

// Re-export ACP schema types from agent-client-protocol-schema
pub use crate::types::acp::schema::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionMode, SessionModeId,
    SessionModeState,
};

// Re-export mode descriptor types
pub use crate::types::knob::{
    AcpKnob, AgentConfigOption, AgentOptionChoice, AgentOptionKind, KnobDescriptor, SessionKnob,
    SessionKnobSupport,
};
pub use crate::types::mode::{
    canonical_mode_id, default_internal_modes, ModeDescriptor, WriteMode,
};

// Re-export trait types (these are associated with traits but used as data)
pub use crate::traits::tools::{ExecutionContext, ToolDefinition, ToolExample};

// Re-export tool reference types
pub use crate::types::command::{
    split_slash_command, BuiltinCommand, CommandKind, SendOutcome, SessionCommand,
};
pub use crate::types::tool_call::{BuiltinKind, CanonicalToolCall, RenderField, ToolRender};
pub use crate::types::tool_match::{classify_acp, AgentKeys, KeyPattern, RawToolCall};
pub use crate::types::tool_ref::{ToolRef, ToolSource};

// Re-export popup types
pub use crate::types::popup::PopupEntry;

// Re-export undo types
pub use crate::types::undo::UndoSummary;

// Re-export notification types
pub use crate::types::notification::{Notification, NotificationKind, NotificationScope};

// Re-export provider info (used by daemon RPC and session-setup events)
pub use crate::types::provider_info::ProviderInfo;

// Re-export plugin status entry (used by session-setup events)
pub use crate::types::plugin_status::PluginStatusEntry;

// Re-export the command effect (declared by a plugin command, read by the
// daemon's `plugin.commands` reply and the web's plugin panel).
pub use crate::types::command_effect::CommandEffect;

// Re-export the `plugin.*` RPC reply types (see `types::plugin_reply` for
// why they live here rather than as web row types or daemon-local structs).
pub use crate::types::plugin_reply::{
    PluginAck, PluginCommand, PluginCommandsReply, PluginDiscoveryError, PluginInfo,
    PluginInstallOutcome, PluginInstallReply, PluginListReply, PluginOptionCallReply,
    PluginOptionValue, PluginOptionsReply, PluginPublications, PluginPublicationsReply,
    PluginReloadReply, PluginRemoveReply, PluginRunCommandReply,
};
pub use crate::types::status_item::{
    IndeterminateProgress, StatusDisplayItem, StatusItemKind, StatusProgress,
    PLUGIN_APPROVAL_ACTION, PLUGIN_TURNS_ID_PREFIX,
};

// Re-export the plugin surface types (declared in `crucible-lua`, served by
// the daemon's `surface.*` RPCs, drawn by the TUI and the web client).
pub use crate::types::surface::{Mark, Shape, Surface, SurfaceRow};

// Re-export skill discovery types (`skills.*` RPCs).
pub use crate::types::skill::{SkillDetail, SkillSummary, SkillsReply};

// NOTE: `mcp_status::McpServerInfo` is intentionally NOT re-exported at
// `types::` top-level to avoid collision with `traits::mcp::McpServerInfo`
// (distinct: protocol-level identity vs. display-oriented status).
