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
//! here serialises the *core* type the daemon answers with — `ComposedHunk`,
//! `Comment`, `RootStatus`, `Integrity` — reads it into the row,
//! writes it back, and demands the same JSON. A field added in `crucible-core`
//! fails these tests rather than going silently missing.

use super::*;
use crate::test_support::request_json;
use axum::http::StatusCode;
use chrono::{TimeZone, Utc};
use crucible_core::diff::DiffsetId;
use crucible_core::session::{
    Comment, CommentAnchor, CommentAuthor, CommentSide, ComposedHunk, HunkId, Integrity, LineRange,
    PhysicalRoot, ReviewState, RootStatus, SessionId, Skip, SkipKind, SnapshotId,
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
async fn list_hunks_answers_the_declared_shape() {
    let listing: ReviewHunksResponse =
        shape("GET", &format!("{REVIEW}/hunks?scope=turn"), None).await;

    // The scope is echoed, so a client that switched scope mid-flight can drop
    // a stale answer.
    assert_eq!(listing.scope, ReviewScopeRow::Turn);
    assert_eq!(listing.hunks[0].id, "hunk-1");
    assert_eq!(listing.comments[0].body, "why this?");
}

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

/// `review.list_hunks`'s whole object survives `ReviewHunksResponse`.
///
/// Built from the daemon's own types rather than from a JSON literal, so a
/// field added to `ComposedHunk`, `Comment`, `RootStatus` or `Integrity`
/// fails here instead of vanishing between the daemon and the
/// browser — which is the property the old forward-it-verbatim handler had and
/// this is what replaces it.
#[test]
fn the_hunks_reply_writes_back_the_object_review_list_hunks_sent() {
    let hunk = ComposedHunk {
        id: HunkId::from("hunk-1".to_string()),
        root: a_root(),
        path: "src/a.rs".to_string(),
        base_range: LineRange::new(1, 2),
        current_range: LineRange::new(1, 3),
        before_content: "old\n".to_string(),
        after_content: "new\nnewer\n".to_string(),
        tool_call_ids: vec!["call-1".to_string()],
        state: ReviewState::Unreviewed,
        reapplied: true,
    };
    let comment = Comment {
        id: "comment-1".to_string(),
        root: a_root(),
        diffset: DiffsetId::for_session(&SessionId::parse("test-session-001").unwrap()),
        path: "src/a.rs".to_string(),
        anchor: CommentAnchor::Snapshot(SnapshotId::git("0".repeat(40))),
        side: CommentSide::Current,
        line_range: LineRange::new(1, 2),
        quoted: "new\n".to_string(),
        body: "why this?".to_string(),
        author: CommentAuthor::Human,
        resolved: false,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    };
    let mut integrity = Integrity::default();
    integrity.record(Skip {
        record: SkipKind::Root { root: a_root() },
        line: 12,
        reason: "a record this loader could not parse".to_string(),
    });

    let sent = json!({
        "session_id": "test-session-001",
        "scope": "session",
        "hunks": [hunk],
        "comments": [comment],
        "degraded": [RootStatus::degraded(a_root(), "the base tree was collected")],
        "integrity": integrity,
    });

    survives::<ReviewHunksResponse>(&sent);
}

/// The three kinds of journal loss survive, including the unscoped one.
///
/// `SkipKind` is a tagged union, and the tag is the whole difference between
/// "one root blocks" and "every root blocks, including ones the ledger has
/// forgotten". A row that collapsed them would report the loudest failure as
/// the quietest.
#[test]
fn every_kind_of_journal_skip_survives_the_integrity_row() {
    let mut integrity = Integrity::default();
    for (index, record) in [
        SkipKind::Session,
        SkipKind::Root { root: a_root() },
        SkipKind::Informational,
    ]
    .into_iter()
    .enumerate()
    {
        integrity.record(Skip {
            record,
            line: index as u32 + 1,
            reason: "unreadable".to_string(),
        });
    }

    survives::<ReviewIntegrityRow>(&integrity);
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

/// The wire spelling of every review state the daemon can send.
///
/// An exhaustive match, so a variant added to `crucible_core`'s enum fails to
/// compile here rather than reaching [`ReviewStateRow`] as an unreadable
/// string at run time.
fn state_spelling(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Unreviewed => "unreviewed",
        ReviewState::Accepted => "accepted",
        ReviewState::Rejected => "rejected",
    }
}

/// The wire spelling of every author. Exhaustive for the same reason.
fn author_spelling(author: CommentAuthor) -> &'static str {
    match author {
        CommentAuthor::Human => "human",
        CommentAuthor::Agent => "agent",
    }
}

/// The wire spelling of every scope. Exhaustive for the same reason.
fn scope_spelling(scope: ReviewScope) -> &'static str {
    match scope {
        ReviewScope::Session => "session",
        ReviewScope::Turn => "turn",
    }
}

#[test]
fn the_mirrored_enums_read_every_spelling_the_daemon_writes() {
    for state in [
        ReviewState::Unreviewed,
        ReviewState::Accepted,
        ReviewState::Rejected,
    ] {
        let spelling = state_spelling(state);
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            json!(spelling),
            "the daemon's spelling of {state:?} moved"
        );
        serde_json::from_value::<ReviewStateRow>(json!(spelling))
            .unwrap_or_else(|e| panic!("`ReviewStateRow` cannot read `{spelling}`: {e}"));
    }

    for author in [CommentAuthor::Human, CommentAuthor::Agent] {
        let spelling = author_spelling(author);
        assert_eq!(serde_json::to_value(author).unwrap(), json!(spelling));
        serde_json::from_value::<CommentAuthorRow>(json!(spelling))
            .unwrap_or_else(|e| panic!("`CommentAuthorRow` cannot read `{spelling}`: {e}"));
    }

    for scope in [ReviewScope::Session, ReviewScope::Turn] {
        let spelling = scope_spelling(scope);
        assert_eq!(serde_json::to_value(scope).unwrap(), json!(spelling));
        let row: ReviewScopeRow = serde_json::from_value(json!(spelling))
            .unwrap_or_else(|e| panic!("`ReviewScopeRow` cannot read `{spelling}`: {e}"));
        // And the query's mirror asks the daemon for the scope it was given,
        // rather than narrowing it to the default.
        assert_eq!(
            serde_json::to_value(ReviewScope::from(row)).unwrap(),
            json!(spelling)
        );
    }
}
