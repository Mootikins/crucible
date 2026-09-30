//! Diffset contract tests (with mock daemon).
//!
//! `GET /api/diff`, `/api/diff/file`, `/api/diff/comments` and the three
//! `POST /api/diff/comment*` routes each only forwarded one RPC row
//! (`diff.get`/`diff.file`/`diff.comments`/`diff.comment`/
//! `diff.resolve_comment`/`diff.delete_comment`), so they are gone
//! ([[Simplification Plan#Step 19]]). The browser reaches them through
//! `POST /api/rpc/{method}` now, sending the tagged `DiffsetSource` directly
//! in the JSON body rather than a flat query string built and validated by
//! the web route — the union already refuses an ambiguous or malformed
//! source at `serde` deserialization, mapped to a 422 the same generic way
//! `a_daemon_invalid_params_error_is_422` in `routes/rpc.rs` proves, so this
//! file does not repeat the web-local "give exactly one of root, session and
//! proposal" pre-check `routes/diff.rs` used to answer itself.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

/// The query reaches the daemon as a branch source, and the reply reaches
/// the browser as the daemon wrote it.
#[tokio::test]
async fn diff_get_and_file_answer_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state.clone());

    let (status, answered) = call_rpc(
        app,
        "diff.get",
        json!({ "source": { "kind": "branch", "root": "/tmp/test-project", "base": "", "head": null } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        answered["source"],
        json!({ "kind": "branch", "root": "/tmp/test-project", "base": "", "head": null })
    );

    let app = build_test_app(state);
    let (status, text) = call_rpc(
        app,
        "diff.file",
        json!({
            "source": { "kind": "branch", "root": "/tmp/test-project", "base": "main", "head": "topic" },
            "path": "new.md",
            "from": "old.md",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.get("base_text").is_some(), "{text}");
}

/// A `session_record` source reaches the daemon as such, and the file
/// request carries the root of the file (a session record can span more
/// than one root).
#[tokio::test]
async fn a_session_record_source_names_the_session() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state.clone());
    let source = json!({ "kind": "session_record", "session": "chat-1" });

    let (status, answered) = call_rpc(app, "diff.get", json!({ "source": source })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answered["source"], source);

    let app = build_test_app(state);
    let (status, text) = call_rpc(
        app,
        "diff.file",
        json!({ "source": source, "path": "a.md", "root": "/tmp/test-project" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.get("base_text").is_some(), "{text}");
}

/// The comment body reaches the daemon as the row's own params: the source,
/// the root, the side, the range and the author.
#[tokio::test]
async fn diff_comment_answers_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let source = json!({ "kind": "session_record", "session": "chat-1" });
    let (status, answered) = call_rpc(
        app,
        "diff.comment",
        json!({
            "source": source,
            "root": "/tmp/test-project",
            "path": "a.md",
            "side": "base",
            "line_start": 2,
            "line_end": 4,
            "body": "why?",
            "author": "agent",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(answered.get("comment").is_some(), "{answered}");
}

#[tokio::test]
async fn diff_resolve_and_delete_comment_answer_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state.clone());
    let source =
        json!({ "kind": "branch", "root": "/tmp/test-project", "base": "main", "head": null });

    let (status, answered) = call_rpc(
        app,
        "diff.resolve_comment",
        json!({ "source": source, "comment_id": "comment-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answered["comment_id"], "comment-1");
    assert_eq!(answered["resolved"], true);

    let app = build_test_app(state);
    let (status, answered) = call_rpc(
        app,
        "diff.delete_comment",
        json!({ "source": source, "comment_id": "comment-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answered["comment_id"], "comment-1");
    assert_eq!(answered["deleted"], true);
}

/// The `outdated` flag of each comment reaches the browser.
#[tokio::test]
async fn diff_comments_answers_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, answered) = call_rpc(
        app,
        "diff.comments",
        json!({ "source": { "kind": "session_record", "session": "chat-1" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let comments = answered["comments"].as_array().expect("an array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["outdated"], true);
}

/// A `proposal` source reaches the daemon as such, and the file request
/// carries the root of the file.
#[tokio::test]
async fn a_proposal_source_names_the_proposal() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state.clone());
    let source = json!({ "kind": "proposal", "id": "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f" });

    let (status, answered) = call_rpc(app, "diff.get", json!({ "source": source })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answered["source"], source);

    let app = build_test_app(state.clone());
    let (status, text) = call_rpc(
        app,
        "diff.file",
        json!({ "source": source, "path": "a.md", "root": "/tmp/test-project" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.get("base_text").is_some(), "{text}");

    let app = build_test_app(state);
    let (status, answered) = call_rpc(app, "diff.comments", json!({ "source": source })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(answered.get("comments").is_some(), "{answered}");
}
