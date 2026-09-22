//! Every review route answers the struct it declares, and every struct writes
//! back what the daemon sent.
//!
//! One test per handler, because a test that reads `status == 200` proves
//! nothing about the reply: these routes answered `serde_json::Value`
//! until task A6 named their shapes, and a renamed field would have passed
//! every such test. Each one decodes the body into the handler's own reply
//! struct, which fails on a missing or retyped field, and reads one field
//! back.
//!
//! The round-trip tests at the end are the stronger claim, and they are why
//! naming these replies is safe at all. The module used to forward the
//! daemon's objects verbatim so that a key the daemon added could not be
//! dropped on the way to the browser. A named struct can drop one, so each row
//! here serialises the *core* type the daemon answers with — `Comment` —
//! reads it into the row,
//! writes it back, and demands the same JSON. A field added in `crucible-core`
//! fails these tests rather than going silently missing.

use super::*;
use crate::test_support::request_json;
use axum::http::StatusCode;
use chrono::{TimeZone, Utc};
use crucible_core::diff::DiffsetId;
use crucible_core::session::{
    Comment, CommentAnchor, CommentAuthor, CommentSide, LineRange, PhysicalRoot, SessionId,
};
use serde_json::{json, Value};

/// Drive one request and decode the body into the reply struct `T`.
async fn shape<T: serde::de::DeserializeOwned>(method: &str, uri: &str, body: Option<Value>) -> T {
    let (status, json) = request_json(method, uri, body).await;
    assert_eq!(status, StatusCode::OK, "{method} {uri}: {json}");
    serde_json::from_value(json.clone()).unwrap_or_else(|e| {
        panic!("{method} {uri} answered a body the struct cannot read: {e}\n{json}")
    })
}

const REVIEW: &str = "/api/session/test-session-001/review";

// =========================================================================
// One test per handler
// =========================================================================

#[tokio::test]
async fn comment_answers_the_declared_shape() {
    let written: ReviewCommentResponse = shape(
        "POST",
        &format!("{REVIEW}/comment"),
        Some(json!({"path": "src/a.rs", "line_start": 1, "body": "needs a test"})),
    )
    .await;
    assert_eq!(written.comment.body, "needs a test");
    assert_eq!(written.comment.author, CommentAuthorRow::Human);
}

#[tokio::test]
async fn resolve_comment_answers_the_declared_shape() {
    let resolved: ReviewResolveCommentResponse = shape(
        "POST",
        &format!("{REVIEW}/comment/comment-1/resolve"),
        Some(json!({})),
    )
    .await;
    assert_eq!(resolved.comment_id, "comment-1");
    assert!(resolved.resolved);
}

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
fn the_comment_reply_writes_back_the_object_review_comment_sent() {
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
    let sent = json!({ "session_id": "test-session-001", "comment": comment });

    survives::<ReviewCommentResponse>(&sent);
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
