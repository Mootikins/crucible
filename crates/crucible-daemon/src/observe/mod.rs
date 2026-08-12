//! Session logging and observability for Crucible
//!
//! This crate provides session persistence as append-only JSONL files
//! with optional SQLite indexing for fast queries.
//!
//! # Architecture
//!
//! Sessions are stored in `.crucible/sessions/<id>/`:
//! - `session.jsonl` - Append-only event stream (primary format)
//! - `session.md` - Human-readable export (generated on demand)
//! - `workspace/` - Scratch directory for session artifacts
//!
//! # Event Types
//!
//! `session.jsonl` holds **two** line shapes, and this module reads both. See
//! [`SessionLogLine`] for why, and [`parse_session_log`] for the one parser
//! that handles it.
//!
//! The wire shape — `{"type":"event","event":"<name>","data":{…}}` — is what
//! `persist_event` (`server/core.rs`) appends, and is the overwhelming
//! majority of every real file. Its `event` names are the ones `should_persist`
//! (`server/core.rs`) admits:
//! - `user_message`, `thinking`, `message_complete` - the conversation
//! - `segment_complete` - a prefix of the same turn's `message_complete`
//! - `tool_call`, `tool_result` - tool invocations and their outputs
//! - `model_switched` - supplies the model attribution for later turns
//! - `precognition_complete` - what context was injected
//! - `ended` - lifecycle bookkeeping
//!
//! The view shape is a serialized [`LogEvent`], written by `inject_context_impl`
//! (`server/session/messaging.rs`) and both fork handlers, and tagged on `type`:
//! - `init` - Session initialization with metadata
//! - `system` - System prompts and context injections
//! - `user` - User messages
//! - `assistant` - Model responses (final, not streaming)
//! - `thinking` - Model reasoning/thinking blocks
//! - `tool_call` - Tool invocations with args
//! - `permission` - Allow/deny decisions for tool calls
//! - `tool_result` - Tool outputs (may be truncated)
//! - `summary` - Context compaction summaries
//! - `error` - Errors during session
//! - `bash_*`, `subagent_*` - background task and subagent bookkeeping
//!
//! # Example
//!
//! This module is the **read** side. Writing is `persist_event`'s job, off the
//! daemon's broadcast channel; nothing outside the daemon appends to a session
//! log.
//!
//! ```no_run
//! use crucible_daemon::{load_events, LogEvent};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let events = load_events(".crucible/sessions/chat-20260811-1200-abcd").await?;
//!
//! for event in &events {
//!     if let LogEvent::User { content, .. } = event {
//!         println!("user said: {content}");
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod events;
pub mod id;
pub mod indexer;
pub mod markdown;
pub mod rebuild;
pub mod serde_md;
pub mod session;

// Re-exports for convenience
pub use events::{
    parse_session_log, wire_to_log_event, LogEvent, PermissionDecision, SessionLogLine, TokenUsage,
};
pub use id::{SessionId, SessionIdError, SessionType};
pub use indexer::{extract_session_content, SessionContent};
pub use markdown::{render_to_markdown, RenderOptions};
pub use session::{list_sessions, load_events, SessionError};
