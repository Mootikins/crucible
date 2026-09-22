//! The comment rows that the `/api/diff/comment*` routes answer.
//!
//! **Each row is named here, and the names are what the browser reads.** A
//! named struct puts the fields in the OpenAPI document and in the generated
//! TypeScript.
//!
//! A struct can drop a key that the daemon added, so that a new feature goes
//! missing and does not fail to compile. `comment_rows_tests.rs` prevents
//! this. Each row round-trips the *core* type that the daemon serializes,
//! `Comment`, and demands the same JSON back. So a field added in
//! `crucible-core` fails a test here.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// =========================================================================
// The rows the daemon's review types serialise as
// =========================================================================

/// A half-open range of 1-based line numbers: `end` is one past the last line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct LineRangeRow {
    /// First line, 1-based, inclusive.
    start: u32,
    /// One past the last line, 1-based, exclusive.
    end: u32,
}

/// Who wrote a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentAuthorRow {
    Human,
    Agent,
}

/// What a comment is anchored in. The web mirror of
/// `crucible_core::session::CommentAnchor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(super) enum CommentAnchorRow {
    /// The session base snapshot of a session record.
    Snapshot(String),
    /// The merge-base commit of a branch diff.
    Commit(String),
    /// One proposal id.
    Proposal(String),
}

/// The side of a diff that a comment range counts its lines on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentSideRow {
    Base,
    Current,
}

impl From<CommentAuthorRow> for crucible_core::session::CommentAuthor {
    fn from(row: CommentAuthorRow) -> Self {
        match row {
            CommentAuthorRow::Human => Self::Human,
            CommentAuthorRow::Agent => Self::Agent,
        }
    }
}

impl From<CommentSideRow> for crucible_core::session::CommentSide {
    fn from(row: CommentSideRow) -> Self {
        match row {
            CommentSideRow::Base => Self::Base,
            CommentSideRow::Current => Self::Current,
        }
    }
}

/// One review comment, anchored to a line range rather than to a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct ReviewCommentRow {
    id: String,
    /// The diffset that owns the comment.
    diffset: String,
    /// The repository top level.
    root: String,
    /// The path, relative to `root`.
    path: String,
    /// What the diffset compares with when the comment was made.
    anchor: CommentAnchorRow,
    /// The side that `line_range` counts its lines on.
    side: CommentSideRow,
    line_range: LineRangeRow,
    /// The text of the range on `side` when the comment was made.
    quoted: String,
    body: String,
    author: CommentAuthorRow,
    resolved: bool,
    /// When the comment was written, RFC 3339.
    ///
    /// A string rather than a date type: the daemon's spelling reaches the
    /// browser unchanged, and a parse and a reformat here could only lose
    /// precision the daemon sent.
    #[schema(format = DateTime)]
    created_at: String,
}

#[cfg(test)]
#[path = "comment_rows_tests.rs"]
mod tests;
