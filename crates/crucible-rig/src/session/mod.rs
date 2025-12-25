//! Session state management
//!
//! This module provides types for persisting agent conversation sessions.
//! Sessions include:
//! - Metadata (workspace, timestamps, continuation info)
//! - Messages (conversation history with tool calls)
//! - Tasks (todo list with status tracking)
//!
//! Sessions are serialized to JSON for full state preservation and
//! rendered to Markdown for human-readable summaries.

pub mod format;
pub mod logger;
pub mod types;

// Re-export key types
pub use types::{
    MessageRole, SessionEntry, SessionIndex, SessionMessage, SessionMetadata, SessionState, Task,
    TaskStatus,
};

// Re-export format functions
pub use format::{
    format_agent_response, format_frontmatter, format_session, format_task_list, format_tool_call,
    format_user_message,
};

// Re-export logger
pub use logger::{LoggerError, LoggerResult, SessionLogger};
