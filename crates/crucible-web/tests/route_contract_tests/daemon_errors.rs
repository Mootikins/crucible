//! Daemon-error → HTTP contract tests.
//!
//! errors.rs verifies the WebError→status mapping in isolation; these tests
//! close the end-to-end gap: a daemon that answers with a JSON-RPC *error
//! envelope* (not a result) must surface through the route as 502 Bad
//! Gateway with the `{"error": {code, message}}` body — not a hang, panic,
//! or misparsed success.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon_with_errors, MockErrors};

fn errors_for(methods: &[&str]) -> MockErrors {
    methods
        .iter()
        .map(|m| (m.to_string(), (-32000i64, format!("{m} exploded"))))
        .collect()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body readable");
    serde_json::from_slice(&bytes).expect("error body is JSON")
}

#[tokio::test]
async fn session_get_daemon_error_maps_to_502_with_error_body() {
    let (_mock, client) = start_mock_daemon_with_errors(errors_for(&["session.get"])).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let json = body_json(response).await;
    assert!(json["error"]["code"].is_number(), "error body: {json}");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("session.get exploded"),
        "the daemon's message must reach the client: {json}"
    );
}

#[tokio::test]
async fn chat_send_daemon_error_maps_to_502() {
    let (_mock, client) =
        start_mock_daemon_with_errors(errors_for(&["session.send_message"])).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"session_id":"test-session-001","content":"hi"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn set_mode_daemon_error_maps_to_502() {
    let (_mock, client) = start_mock_daemon_with_errors(errors_for(&["session.set_mode"])).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"yolo"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn session_list_daemon_error_maps_to_502() {
    let (_mock, client) = start_mock_daemon_with_errors(errors_for(&["session.list"])).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/list")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

/// The daemon's refusal reaches the browser as the sentence the daemon
/// wrote, at 422, and not as `RPC error: {"code":-32602,…}`: a session
/// created with no project lists its own folder, and when the daemon refuses
/// a root the user must read why.
#[tokio::test]
async fn fs_list_refusal_maps_to_422_with_the_daemons_sentence() {
    let mut errors = MockErrors::new();
    errors.insert(
        "fs.list_dir".to_string(),
        (
            -32602,
            "root is not a registered project or a session's own workspace folder".to_string(),
        ),
    );
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/fs/list?root=/home/u/.crucible/workspaces/ses-1&rel_path=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(response).await;
    assert_eq!(
        json["error"]["message"],
        "root is not a registered project or a session's own workspace folder",
        "the body is the daemon's sentence, not its envelope: {json}"
    );
}

/// A create the daemon cannot honour — here a workspace target no plugin
/// resolves — is a 422 whose body names the reason, for every agent type.
#[tokio::test]
async fn session_create_refusal_maps_to_422_with_the_reason() {
    let mut errors = MockErrors::new();
    errors.insert(
        "session.create".to_string(),
        (
            -32602,
            "workspace target 'worktree:feat/x' could not be resolved: no plugin provides workspace targets named 'worktree'".to_string(),
        ),
    );
    let (_mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"agent_type":"acp","agent_name":"claude","workspace_target":"worktree:feat/x","isolation":{"plugin":"oci","target":"dev"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(response).await;
    let message = json["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("workspace target 'worktree:feat/x' could not be resolved"),
        "the reason, unwrapped: {json}"
    );
}

/// A method NOT in the error script still succeeds — the scripting is
/// per-method, so one failing RPC doesn't poison unrelated routes.
#[tokio::test]
async fn unscripted_methods_still_succeed_alongside_errors() {
    let (_mock, client) = start_mock_daemon_with_errors(errors_for(&["session.get"])).await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/list")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
