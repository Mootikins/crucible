//! Diff comment route contract tests: what a daemon refusal becomes.
//!
//! The shape of each reply is pinned in `src/routes/diff.rs`, which returns
//! the core `crucible_core::protocol::requests::DiffCommentReply` and its
//! siblings unchanged. These tests pin the status that a daemon error
//! gives, because a client acts on the status.

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon_with_errors, MockErrors};

/// Post one comment to an app whose daemon answers `diff.comment` with
/// `(code, message)`.
async fn comment_refused(code: i64, message: &str) -> (StatusCode, Value) {
    let errors: MockErrors = [(RpcMethod::DiffComment, (code, message.to_string()))]
        .into_iter()
        .collect();
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_state(client));
    let body = json!({
        "source": { "kind": "session_record", "session": "s1" },
        "root": "/tmp/test-project",
        "path": "src/a.rs",
        "side": "current",
        "line_start": 1,
        "body": "why",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/diff/comment")
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A refusal as INVALID_PARAMS, such as a path under no root of the
/// session, is the caller's error. The client reads its message and does
/// not report the daemon as broken.
#[tokio::test]
async fn a_daemon_refusal_is_a_422_carrying_its_message() {
    let (status, json) = comment_refused(-32602, "path escapes the root: \"../a\"").await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("path escapes the root"),
        "the daemon's message must reach the client: {json}"
    );
}

/// A store that the daemon cannot write answers INTERNAL_ERROR. The route
/// must not make it a 4xx, because the request was correct.
#[tokio::test]
async fn an_internal_daemon_failure_stays_a_502() {
    let (status, _json) = comment_refused(-32603, "comment store unreadable").await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
}
