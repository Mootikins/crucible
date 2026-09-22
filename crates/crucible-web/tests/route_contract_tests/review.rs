//! Review route contract tests.
//!
//! These two routes forward a request untouched and answer a **named
//! struct**. Task A6 named the replies (`routes/session/review.rs`), so the
//! daemon's result no longer travels as `serde_json::Value`: the route reads
//! the daemon's object into its reply type and writes that type back. A key
//! the daemon grows therefore reaches the browser only after someone adds a
//! field for it, which is the opposite of what this file used to promise.
//!
//! What holds the reply to the daemon is
//! `src/routes/session/review_shape_tests.rs`. It builds the core types the
//! daemon serialises — `Comment` — pushes it through its row and demands the
//! same JSON back,
//! so a new field in `crucible-core` fails a test instead of going missing.
//! These tests pin the HTTP surface around it: the path, the statuses, and
//! what reaches the daemon. `web/src/lib/__tests__/review-api.test.ts`
//! holds the other side of the same wire.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{
    build_mock_state, build_test_app, start_mock_daemon, start_mock_daemon_with_errors, MockErrors,
};
use crucible_web::test_support::MockDaemon;

/// Drive one request through a mock-daemon-backed app, keeping the mock alive
/// so the caller can assert on what the daemon actually received.
async fn call(method: &str, uri: &str, body: Option<Value>) -> (MockDaemon, StatusCode, Value) {
    let (mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (mock, status, json)
}

#[tokio::test]
async fn a_session_id_with_a_slash_survives_the_path() {
    // The frontend percent-encodes it; axum decodes it back. If the round trip
    // dropped the encoding the daemon would be asked about a session named `a`.
    let (mock, status, _json) = call(
        "POST",
        "/api/session/a%2Fb/review/comment/c1/resolve",
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mock.received_params("review.resolve_comment").unwrap()["session_id"],
        "a/b"
    );
}

/// `line_end`, `root` and `author` must arrive ABSENT, not null: the daemon
/// applies its own defaults through `optional_param!`, and an explicit null
/// defeats every one of them (`line_end` in particular would stop defaulting
/// to `line_start + 1`).
#[tokio::test]
async fn a_comment_omits_the_optional_fields_the_caller_omitted() {
    let (mock, status, _json) = call(
        "POST",
        "/api/session/s1/review/comment",
        Some(json!({ "path": "src/a.rs", "line_start": 3, "body": "why" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let params = mock.received_params("review.comment").unwrap();
    let object = params.as_object().expect("params object");
    assert!(!object.contains_key("line_end"), "sent: {params}");
    assert!(!object.contains_key("root"), "sent: {params}");
    assert!(!object.contains_key("author"), "sent: {params}");
    assert_eq!(params["line_start"], 3);
    assert_eq!(params["body"], "why");
}

#[tokio::test]
async fn a_comment_forwards_the_optional_fields_the_caller_sent() {
    let (mock, status, json) = call(
        "POST",
        "/api/session/s1/review/comment",
        Some(json!({
            "path": "src/a.rs",
            "line_start": 3,
            "line_end": 9,
            "body": "why",
            "root": "/tmp/test-project",
            "author": "agent",
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let params = mock.received_params("review.comment").unwrap();
    assert_eq!(params["line_end"], 9);
    assert_eq!(params["root"], "/tmp/test-project");
    assert_eq!(params["author"], "agent");
    assert_eq!(json["comment"]["body"], "why");
}

/// The session under review is the one in the PATH. A body naming a different
/// session must not redirect the write — otherwise a comment could be planted
/// on any session the caller can name.
#[tokio::test]
async fn a_session_id_in_the_body_cannot_override_the_path() {
    let (mock, status, _json) = call(
        "POST",
        "/api/session/s1/review/comment",
        Some(json!({
            "session_id": "victim",
            "path": "src/a.rs",
            "line_start": 3,
            "body": "why",
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mock.received_params("review.comment").unwrap()["session_id"],
        "s1"
    );
}

#[tokio::test]
async fn resolve_comment_takes_the_comment_id_from_the_path() {
    let (mock, status, json) = call(
        "POST",
        "/api/session/s1/review/comment/c%201/resolve",
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let params = mock.received_params("review.resolve_comment").unwrap();
    assert_eq!(params["session_id"], "s1");
    assert_eq!(params["comment_id"], "c 1");
    assert_eq!(json["resolved"], true);
}

/// Everything the daemon refuses as INVALID_PARAMS — such as a comment on a
/// path under no tracked root — is the caller's problem, not a gateway
/// failure, and the client's response is to read again rather than to report
/// the daemon as broken.
#[tokio::test]
async fn a_daemon_refusal_is_a_422_carrying_its_message() {
    let errors: MockErrors = [(
        "review.comment".to_string(),
        (
            -32602i64,
            "src/a.rs resolves outside the session's tracked roots".to_string(),
        ),
    )]
    .into_iter()
    .collect();
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/s1/review/comment")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "path": "src/a.rs", "line_start": 1, "body": "why" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("outside the session's tracked roots"),
        "the daemon's message must reach the client: {json}"
    );
}

/// A journal the daemon cannot read answers INTERNAL_ERROR, and that must not
/// be laundered into a 4xx: "your request was bad" would send the client
/// looking for a field to fix when the review data itself is unreadable.
#[tokio::test]
async fn an_internal_daemon_failure_stays_a_502() {
    let errors: MockErrors = [(
        "review.comment".to_string(),
        (-32603i64, "review journal unreadable".to_string()),
    )]
    .into_iter()
    .collect();
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/s1/review/comment")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "path": "src/a.rs", "line_start": 1, "body": "why" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}
