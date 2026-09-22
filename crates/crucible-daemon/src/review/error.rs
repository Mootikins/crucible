//! Review errors.
//!
//! thiserror here because these variants cross the RPC boundary: `review.*`
//! handlers match on them to pick a JSON-RPC error code, and the web panel
//! renders the message. Everything internal to the engine stays on
//! `anyhow`-shaped plumbing.

use std::path::PathBuf;

use crucible_core::session::SnapshotId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReviewError {
    /// No ledger for this session: it was never opened (no trackable root),
    /// or it was cleared at session end. Handlers answer "nothing to review".
    #[error("session {0} has no review ledger")]
    NoLedger(String),

    /// Not one of the session's roots can be snapshotted, so there is nothing
    /// to diff against. Callers skip bracketing entirely rather than falling
    /// back to walking the workspace on every tool call.
    ///
    /// Since a root outside git is tracked through
    /// [`crate::review::backend::RootBackend::Plain`], this now means an empty
    /// root list or roots that are not there at all.
    #[error("no trackable root among: {0}")]
    NoTrackableRoots(String),

    /// The session id is not a valid session id, so it names no session
    /// record diffset.
    #[error("{0}")]
    InvalidSession(String),

    #[error("unknown comment {0}")]
    UnknownComment(String),

    /// The caller asked for a comment that the daemon cannot anchor, such
    /// as a range that starts at line 0 or ends before it starts.
    #[error("{0}")]
    InvalidComment(String),

    #[error("{path} is not inside a git repository")]
    NotAGitRepo { path: PathBuf },

    /// A relative path with several tracked roots to choose from.
    ///
    /// Distinct from [`Self::NotAGitRepo`], which used to answer this: telling
    /// a caller their repository is not a repository, when the repository is
    /// fine and they simply have two of them, sends them looking in the wrong
    /// place entirely. The fix is one more argument, and the message says so.
    #[error("{path} is ambiguous across {roots} tracked roots; pass `root`")]
    AmbiguousPath { path: PathBuf, roots: usize },

    /// A path that resolves outside every tracked root.
    #[error("{path} resolves outside the session's tracked roots")]
    PathEscapesRoot { path: PathBuf },

    /// The session's `review.jsonl` exists and could not be read *as a file*.
    ///
    /// Distinct from a record the lenient parser skipped, which degrades the
    /// ledger without failing it. This variant is the case where the caller
    /// must not proceed: a journal that exists but cannot be opened must never
    /// fall through to capturing a fresh base, because a fresh base reports
    /// that the agent changed nothing and silently empties the review queue.
    #[error("review journal {path} could not be read: {reason}")]
    Journal { path: PathBuf, reason: String },

    /// Git plumbing was handed a snapshot id from another backend.
    ///
    /// A routing fault inside the daemon, never something a caller did: the
    /// arm of [`crucible_core::session::SnapshotId`] says which backend can
    /// read a snapshot, so a git command that receives a plain-store id was
    /// reached through the wrong seam. Reported rather than asserted, because
    /// the release profile aborts on a panic and takes every live session
    /// with it.
    #[error("snapshot {id} is not a git tree in {}", root.display())]
    WrongBackend { root: PathBuf, id: SnapshotId },

    #[error("git failed: {0}")]
    Git(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type ReviewResult<T> = Result<T, ReviewError>;
