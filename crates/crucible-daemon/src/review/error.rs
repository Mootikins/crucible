//! Review errors.
//!
//! thiserror here because these variants cross the RPC boundary: `review.*`
//! handlers match on them to pick a JSON-RPC error code, and the web panel
//! renders the message. Everything internal to the engine stays on
//! `anyhow`-shaped plumbing.

use std::path::PathBuf;

use crucible_core::session::HunkId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReviewError {
    /// No ledger for this session: it was never opened (no git-backed root),
    /// or it was cleared at session end. Handlers answer "nothing to review".
    #[error("session {0} has no review ledger")]
    NoLedger(String),

    /// None of the session's roots is inside a git repository, so there is
    /// nothing to diff against. Callers skip bracketing entirely rather than
    /// falling back to walking the workspace on every tool call.
    #[error("no git-backed root among: {0}")]
    NoTrackableRoots(String),

    /// Fail-closed answer for an identity the ledger does not recognise. A
    /// stale client acting on a hunk that no longer exists must be told so,
    /// never have its decision applied to whatever is at those lines now.
    #[error("unknown hunk {0}")]
    UnknownHunk(HunkId),

    /// The file changed under us between listing and reverting. The client
    /// re-lists and retries; we never write a hunk we cannot recognise.
    #[error("{path} changed since the hunk was computed")]
    Stale { path: String },

    /// Reverting an unattributed hunk would destroy the user's own edit while
    /// reporting that an agent edit was undone.
    #[error("hunk {0} is external and cannot be reverted")]
    ExternalHunk(HunkId),

    #[error("unknown comment {0}")]
    UnknownComment(String),

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

    #[error("git failed: {0}")]
    Git(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type ReviewResult<T> = Result<T, ReviewError>;
