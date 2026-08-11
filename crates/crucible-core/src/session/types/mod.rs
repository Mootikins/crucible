//! Core session types.

mod agent;
mod config;
mod enums;
mod review;
mod session;
mod summary;

#[cfg(test)]
mod tests;

pub use agent::SessionAgent;
pub use config::{validate_output, ContextStrategy, OutputValidation};
pub use enums::{RecordingMode, SessionState, SessionType};
pub use review::{
    ChildLedgerRef, Comment, CommentAuthor, ComposedHunk, GateBlock, HunkId, Integrity, Interval,
    Ledger, LineRange, PhysicalRoot, ReviewState, RootBase, RootInterval, RootStatus, Skip,
    SkipKind, TreeSha, Verdict,
};
pub use session::Session;
pub use summary::SessionSummary;
