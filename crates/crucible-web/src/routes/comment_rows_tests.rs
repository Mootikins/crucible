//! Every comment row writes back what the daemon sent.
//!
//! A named struct can drop a key that the daemon added. So each row here
//! serializes the *core* type that the daemon answers with, `Comment`, reads
//! it into the row, writes it back, and demands the same JSON. A field added
//! in `crucible-core` fails these tests and does not go missing.

use super::*;
use chrono::{TimeZone, Utc};
use crucible_core::diff::DiffsetId;
use crucible_core::session::{
    Comment, CommentAnchor, CommentAuthor, CommentSide, LineRange, PhysicalRoot, SessionId,
};
use serde_json::json;

// =========================================================================
// The rows write back what the daemon's own types sent
// =========================================================================

/// Serialise `sent`, read it into `T`, write it back, and demand the same JSON.
fn survives<T: serde::Serialize + serde::de::DeserializeOwned>(sent: &impl serde::Serialize) {
    let wire = serde_json::to_value(sent).expect("the daemon's type writes JSON");
    let row: T = serde_json::from_value(wire.clone())
        .unwrap_or_else(|e| panic!("the row cannot read what the daemon sent: {e}\n{wire}"));
    assert_eq!(
        serde_json::to_value(row).expect("the row writes JSON"),
        wire,
        "the row changed the object on the way through"
    );
}

fn a_root() -> PhysicalRoot {
    PhysicalRoot::from_top_level("/tmp/test-project")
}

/// A comment the daemon minted survives, including the timestamp's spelling.
///
/// The row carries `created_at` as a string so the daemon's own RFC 3339
/// spelling reaches the browser; a parse and a reformat here could only lose
/// what the daemon wrote.
#[test]
fn the_comment_row_writes_back_the_comment_the_daemon_sent() {
    let comment = Comment {
        id: "comment-2".to_string(),
        root: a_root(),
        diffset: DiffsetId::for_session(&SessionId::parse("test-session-001").unwrap()),
        path: "src/a.rs".to_string(),
        anchor: CommentAnchor::Commit("1".repeat(40)),
        side: CommentSide::Base,
        line_range: LineRange::new(4, 9),
        quoted: "old\n".to_string(),
        body: "needs a test".to_string(),
        author: CommentAuthor::Agent,
        resolved: true,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 12, 30, 15).unwrap(),
    };
    survives::<ReviewCommentRow>(&comment);
}

// =========================================================================
// The mirrored vocabularies stay closed
// =========================================================================

/// The wire spelling of every author.
///
/// An exhaustive match, so a variant added to `crucible_core`'s enum fails to
/// compile here rather than reaching [`CommentAuthorRow`] as an unreadable
/// string at run time.
fn author_spelling(author: CommentAuthor) -> &'static str {
    match author {
        CommentAuthor::Human => "human",
        CommentAuthor::Agent => "agent",
    }
}

#[test]
fn the_mirrored_enums_read_every_spelling_the_daemon_writes() {
    for author in [CommentAuthor::Human, CommentAuthor::Agent] {
        let spelling = author_spelling(author);
        assert_eq!(serde_json::to_value(author).unwrap(), json!(spelling));
        serde_json::from_value::<CommentAuthorRow>(json!(spelling))
            .unwrap_or_else(|e| panic!("`CommentAuthorRow` cannot read `{spelling}`: {e}"));
    }
}
