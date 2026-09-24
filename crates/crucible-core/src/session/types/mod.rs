//! Core session types.

mod agent;
mod config;
mod enums;
mod id;
mod review;
mod session;
mod summary;

#[cfg(test)]
mod tests;

pub use agent::SessionAgent;
pub use config::ContextStrategy;
pub use enums::{RecordingMode, SessionState, SessionType};
pub use id::{InvalidSessionId, SessionId};
pub use review::{
    ChildLedgerRef, Comment, CommentAnchor, CommentAuthor, CommentSide, ComposedHunk, HunkId,
    Integrity, Interval, Ledger, LineRange, PhysicalRoot, RootBase, RootInterval, RootStatus, Skip,
    SkipKind, SnapshotId,
};
pub use session::{IsolationRecord, IsolationRequirement, Session};
pub use summary::SessionSummary;
