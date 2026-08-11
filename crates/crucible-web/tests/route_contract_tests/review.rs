//! Review route contract tests.
//!
//! These six routes are a pure passthrough in both directions, so the type
//! system proves nothing about them: the daemon's result objects are
//! `serde_json::Value` all the way to the browser precisely so a key the
//! daemon grows (`degraded`, `gate`) reaches a frontend that reads it without
//! a rebuild here. That freedom has a price — the shape is only pinned by
//! these assertions, and by `web/src/lib/__tests__/review-api.test.ts` on the
//! other side of the same wire.

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
async fn list_hunks_forwards_the_daemon_result_verbatim() {
    let (mock, status, json) = call("GET", "/api/session/s1/review/hunks", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.received_methods(), vec!["review.list_hunks"]);
    assert_eq!(json["session_id"], "s1");
    assert_eq!(json["hunks"][0]["id"], "hunk-1");
    assert_eq!(json["comments"][0]["id"], "comment-1");
}

/// The forward-compatibility claim, made concrete: `degraded` was added to the
/// daemon's result after this crate's review code was written and reaches the
/// browser anyway, because nothing between them names the fields.
#[tokio::test]
async fn list_hunks_passes_through_keys_this_crate_never_names() {
    let (_mock, _status, json) = call("GET", "/api/session/s1/review/hunks", None).await;

    assert!(
        json.get("degraded").is_some(),
        "a key the route does not model must still reach the client: {json}"
    );
    // `gate` specifically, and specifically its null-ness: the store treats an
    // absent key as "leave the chip alone" and a null one as "clear it", so a
    // layer that dropped a null on the floor would silently strand a stale
    // "waiting on review" chip.
    assert!(
        json.get("gate").is_some_and(Value::is_null),
        "an explicit null must survive the forward, not be elided: {json}"
    );
    assert!(
        json["hunks"][0].get("reapplied").is_some(),
        "hunk fields are forwarded whole, not remapped: {json}"
    );
    // The unscoped losses `degraded` cannot carry, because they are exactly
    // the ones with no root left to name.
    assert!(
        json.pointer("/integrity/skips").is_some(),
        "a journal loss with no root to attach to must still reach the client: {json}"
    );
}

#[tokio::test]
async fn a_session_id_with_a_slash_survives_the_path() {
    // The frontend percent-encodes it; axum decodes it back. If the round trip
    // dropped the encoding the daemon would be asked about a session named `a`.
    let (mock, status, _json) = call("GET", "/api/session/a%2Fb/review/hunks", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mock.received_params("review.list_hunks").unwrap()["session_id"],
        "a/b"
    );
}

/// The release for a degraded root. It is the only way out of a fail-closed
/// block — a degraded root contributes no hunks, so no amount of reviewing
/// clears it — and for a while it existed only as a daemon method with no
/// client anywhere, reachable solely by hand-written JSON-RPC.
#[tokio::test]
async fn rebase_reaches_the_daemon_and_returns_its_root_statuses() {
    let (mock, status, json) = call("POST", "/api/session/s1/review/rebase", Some(json!({}))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mock.received_params("review.rebase").unwrap()["session_id"],
        "s1"
    );
    assert_eq!(json["roots"][0]["root"], "/tmp/test-project");
}

#[tokio::test]
async fn set_state_forwards_hunk_id_and_state() {
    let (mock, status, json) = call(
        "POST",
        "/api/session/s1/review/state",
        Some(json!({ "hunk_id": "h1", "state": "accepted" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let params = mock.received_params("review.set_state").unwrap();
    assert_eq!(params["session_id"], "s1");
    assert_eq!(params["hunk_id"], "h1");
    assert_eq!(params["state"], "accepted");
    assert_eq!(json["state"], "accepted");
}

/// The state vocabulary belongs to the daemon. A copy of it in this crate
/// could only ever refuse a state the daemon had newly learned, so an unknown
/// value must travel and be refused there.
#[tokio::test]
async fn an_unknown_state_is_the_daemons_refusal_not_this_crates() {
    let (mock, _status, _json) = call(
        "POST",
        "/api/session/s1/review/state",
        Some(json!({ "hunk_id": "h1", "state": "Accepted" })),
    )
    .await;

    assert_eq!(
        mock.received_params("review.set_state").unwrap()["state"],
        "Accepted"
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

/// Everything the daemon refuses as INVALID_PARAMS — unknown hunk, stale hunk,
/// external hunk, and now a comment on a path under no git root — is the
/// caller's problem, not a gateway failure, and the client's response is to
/// re-list rather than to report the daemon as broken.
#[tokio::test]
async fn a_daemon_refusal_is_a_422_carrying_its_message() {
    let errors: MockErrors = [(
        "review.set_state".to_string(),
        (-32602i64, "Hunk no longer exists: h1".to_string()),
    )]
    .into_iter()
    .collect();
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/s1/review/state")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "hunk_id": "h1", "state": "rejected" }).to_string(),
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
            .contains("Hunk no longer exists"),
        "the daemon's message must reach the client: {json}"
    );
}

/// A journal the daemon cannot read answers INTERNAL_ERROR, and that must not
/// be laundered into a 4xx: "your request was bad" would send the client
/// looking for a hunk id to fix when the review data itself is unreadable.
#[tokio::test]
async fn an_internal_daemon_failure_stays_a_502() {
    let errors: MockErrors = [(
        "review.list_hunks".to_string(),
        (-32603i64, "review journal unreadable".to_string()),
    )]
    .into_iter()
    .collect();
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/s1/review/hunks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}
