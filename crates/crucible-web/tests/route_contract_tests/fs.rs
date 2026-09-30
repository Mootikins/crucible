//! Contract tests for `fs.list_dir`/`fs.move`/`fs.mkdir`/`fs.trash`.
//!
//! `GET/POST /api/fs/*` are gone ([[Simplification Plan#Step 19]] item 3,
//! the "migration"): each only forwarded one RPC row. The browser reaches
//! them through `POST /api/rpc/{method}` now, so these tests drive that one
//! route instead.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

async fn call_rpc(app: axum::Router, method: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/rpc/{method}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn fs_list_dir_returns_200_with_a_listing_envelope() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_state(client));

    let (status, body) = call_rpc(
        app,
        "fs.list_dir",
        json!({ "root": "/tmp/proj", "rel_path": "" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    // `{ entries, truncated }` rather than a bare array: the daemon caps a
    // directory at 1000 entries, and the flag is the only way for a capped
    // listing to say so.
    assert!(body["entries"].is_array(), "entries: {body}");
    assert_eq!(body["truncated"], json!(false), "truncated: {body}");
}

#[tokio::test]
async fn fs_move_returns_200_with_moved_true() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_state(client));

    let (status, body) = call_rpc(
        app,
        "fs.move",
        json!({
            "root": "/tmp/proj",
            "kind": "project",
            "from_rel": "a.md",
            "to_rel": "notes/a.md"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["moved"], true);
}

// `fs_move_rejects_missing_fields` (a missing `kind`/`from_rel`/`to_rel`
// answering 422 before the daemon ever saw it) is gone with the route it
// proved: `POST /api/rpc/{method}` takes the body as `serde_json::Value`
// (step 19 item 6's own design — one route for 169 methods, not 169 typed
// bodies), so axum's own extractor no longer rejects a malformed row.
// Deserializing the row's declared `Req` type, and refusing a bad one, is
// the real daemon's job now — the same `INVALID_PARAMS` → 422 mapping every
// other route already used (`WebResultExt::daemon_err`), proved against the
// real dispatcher in `crucible-daemon`'s own RPC tests, not against this
// mock, which never deserializes past `RpcMethod`.

#[tokio::test]
async fn fs_mkdir_returns_200_created() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_state(client));

    let (status, body) = call_rpc(
        app,
        "fs.mkdir",
        json!({ "root": "/tmp/proj", "kind": "project", "rel_path": "new/dir" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["created"], true);
}

#[tokio::test]
async fn fs_trash_returns_200_with_trash_path() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_state(client));

    let (status, body) = call_rpc(
        app,
        "fs.trash",
        json!({ "root": "/tmp/k", "kind": "kiln", "rel_path": "a.md" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["trashed"], true);
    assert!(body["trash_path"]
        .as_str()
        .unwrap()
        .starts_with(".crucible/trash/"));
}
