//! Session logging and observability for Crucible
//!
//! This module reads session persistence: append-only JSONL files.
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
//! `session.jsonl` holds wire lines, and this module also reads the two older
//! shapes that a stored session can hold. See [`SessionLogLine`] for why, and
//! [`parse_session_log`] for the one parser that handles it.
//!
//! The wire shape — `{"type":"event","event":"<name>","data":{…}}` — is what
//! `persist_event` (`server/core.rs`) appends for the events that
//! `is_persisted` admits:
//! - `user_message`, `thinking`, `message_complete` - the conversation
//! - `segment_complete` - a prefix of the same turn's `message_complete`
//! - `tool_call`, `tool_result`, `tool_call_update` - tool invocations
//! - `turn_finished` - where a turn ended, and how
//! - `model_switched` - supplies the model attribution for later turns
//! - `precognition_complete` - what context was injected
//!
//! The agent manager writes two more wire events itself, at once and in
//! order, so that a tree rebuilt from the file cannot miss them:
//! - `context_cleared` - the clear marker
//! - `context_injected` - accepted context, anchored after the turn whose
//!   input was already assembled. The parser places it before the next user
//!   turn, even if the broadcast writer persisted that anchor later.
//!
//! Older daemons wrote a serialized [`LogEvent`] for the clear, for forks and
//! for plain system text, and a `context_injection` line for accepted
//! context. A stored session keeps those lines, so the parser reads them.
//!
//! # Example
//!
//! This module is the **read** side. The daemon writes broadcast events via
//! `persist_event`, and the clear, accepted context and fork records
//! directly; nothing outside the daemon appends to a session log.
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
pub mod markdown;
pub mod rebuild;
pub mod session;

// Re-exports for convenience
pub use events::{parse_session_log, wire_to_log_event, LogEvent, SessionLogLine, TokenUsage};
pub use id::{SessionId, SessionIdError, SessionType};
pub use markdown::{render_to_markdown, RenderOptions};
pub use session::{events_after, load_events, SessionError};
