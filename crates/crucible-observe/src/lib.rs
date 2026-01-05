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
//! The JSONL log captures:
//! - `system` - System prompts and context injections
//! - `user` - User messages
//! - `assistant` - Model responses (final, not streaming)
//! - `tool_call` - Tool invocations with args
//! - `tool_result` - Tool outputs (may be truncated)
//! - `error` - Errors during session
//!
//! # Example
//!
//! ```no_run
//! use crucible_observe::{SessionWriter, LogEvent, SessionType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a new session
//! let mut writer = SessionWriter::create(".crucible/sessions", SessionType::Chat).await?;
//!
//! // Log events
//! writer.append(LogEvent::system("You are helpful")).await?;
//! writer.append(LogEvent::user("Hello!")).await?;
//! writer.append(LogEvent::assistant("Hi there!")).await?;
//!
//! // Session ID can be used to resume later
//! let id = writer.id().clone();
//! # Ok(())
//! # }
//! ```

pub mod events;
pub mod id;
pub mod markdown;
pub mod serde_md;
pub mod session;
pub mod storage;

// Re-exports for convenience
pub use events::{LogEvent, TokenUsage};
pub use id::{SessionId, SessionIdError, SessionType};
pub use markdown::{render_to_markdown, RenderOptions};
pub use session::{list_sessions, load_events, SessionError, SessionMetadata, SessionWriter};

#[cfg(feature = "sqlite")]
pub use storage::SessionIndex;
