//! Session types for Crucible.
//!
//! A session is a continuous sequence of agent actions in a workspace.
//! Sessions are the fundamental unit of agent interaction in Crucible.
//!
//! # Key Concepts
//!
//! - **Session**: A sequence of agent actions, stored under the daemon's
//!   sessions root (not inside a kiln)
//! - **Workspace**: Where the agent operates (file I/O happens here)
//! - **Kilns**: The knowledge stores the session can query — a flat set, with
//!   no privileged member
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::session::{Session, SessionType, SessionState};
//! use std::path::PathBuf;
//!
//! let session = Session::new(
//!     SessionType::Chat,
//!     vec![PathBuf::from("/home/user/notes")],
//! )
//! .with_workspace(PathBuf::from("/home/user/project"))
//! .with_kiln(PathBuf::from("/home/user/reference"));
//! ```

mod types;

pub use types::{
    validate_output, ChildLedgerRef, Comment, CommentAuthor, ComposedHunk, ContextStrategy,
    GateBlock, HunkId, Integrity, Interval, InvalidSessionId, Ledger, LineRange, OutputValidation,
    PhysicalRoot, RecordingMode, ReviewState, RootBase, RootInterval, RootStatus, Session,
    SessionAgent, SessionId, SessionState, SessionSummary, SessionType, Skip, SkipKind, TreeSha,
    Verdict,
};
