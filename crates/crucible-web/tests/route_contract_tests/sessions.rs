//! Session Route Contract Tests (with mock daemon)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

#[tokio::test]
async fn list_sessions_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

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

#[tokio::test]
async fn get_session_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn pause_session_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/pause")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn end_session_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/end")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn cancel_session_returns_200_with_cancelled_field() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("cancelled").is_some(),
        "Response must contain 'cancelled' field"
    );
}

#[tokio::test]
async fn list_models_returns_200_with_models_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["models"].is_array(),
        "Response must have 'models' array"
    );
}

#[tokio::test]
async fn switch_model_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/model")
                .header("content-type", "application/json")
                .body(Body::from(json!({"model_id": "mistral"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_mode_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/mode")
                .header("content-type", "application/json")
                .body(Body::from(json!({"mode": "plan"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_session_title_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/session/test-session-001/title")
                .header("content-type", "application/json")
                .body(Body::from(json!({"title": "My Chat Session"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// =========================================================================
// Session Creation Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn create_session_returns_200_with_session_id() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "provider": "ollama",
                        "model": "llama3.2"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("session_id").is_some(),
        "Response must contain session_id"
    );
    assert_eq!(json["session_id"], "test-session-001");
}

#[tokio::test]
async fn create_session_with_private_ip_endpoint_returns_422() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "provider": "openai",
                        "model": "gpt-4o",
                        "endpoint": "http://10.0.0.1/v1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_session_with_defaults_uses_ollama() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Only required field is kiln — provider and model use defaults
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kiln": "/tmp/test-kiln"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["session_id"], "test-session-001");
}

#[tokio::test]
async fn export_session_returns_markdown_content_type() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/markdown"),
        "Expected text/markdown content-type, got: {}",
        content_type
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.is_empty(), "Exported markdown should not be empty");
}

#[tokio::test]
async fn get_session_returns_session_data() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["state"], "active");
    assert_eq!(json["session_type"], "chat");
}

// =========================================================================
// Session Delete/Archive/Unarchive Route Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn delete_session_returns_200_with_deleted_field() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/session/test-session-001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deleted"], true, "Response must contain deleted: true");
}

#[tokio::test]
async fn archive_session_returns_200_with_archived_true() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/archive")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["archived"], true,
        "Response must contain archived: true"
    );
}

#[tokio::test]
async fn unarchive_session_returns_200_with_archived_false() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/unarchive")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["archived"], false,
        "Response must contain archived: false"
    );
}

#[tokio::test]
async fn list_sessions_with_include_archived_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/list?include_archived=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "include_archived query param should be accepted"
    );
}

// =========================================================================
// Session Scope (kilns/workspace) Route Contract Tests (with mock daemon)
// =========================================================================

/// Drive one request through a fresh mock-daemon-backed app and decode JSON.
async fn send_json(method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
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
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn connect_kiln_returns_scope_shape() {
    let (status, json) = send_json(
        "POST",
        "/api/session/test-session-001/kilns/connect",
        json!({"kiln": "/tmp/extra-kiln"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["kiln"], "/tmp/test-kiln");
    assert_eq!(json["workspace"], "/tmp/test-kiln");
    assert_eq!(json["connected_kilns"][0], "/tmp/extra-kiln");
}

#[tokio::test]
async fn disconnect_kiln_returns_scope_shape() {
    let (status, json) = send_json(
        "POST",
        "/api/session/test-session-001/kilns/disconnect",
        json!({"kiln": "/tmp/extra-kiln"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
    assert!(
        json["connected_kilns"].as_array().unwrap().is_empty(),
        "disconnect empties connected_kilns: {json}"
    );
}

#[tokio::test]
async fn set_workspace_attaches_project_dir() {
    let (status, json) = send_json(
        "PUT",
        "/api/session/test-session-001/workspace",
        json!({"workspace": "/repos/crucible"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["workspace"], "/repos/crucible");
}

#[tokio::test]
async fn set_workspace_null_detaches_to_kiln() {
    let (status, json) = send_json(
        "PUT",
        "/api/session/test-session-001/workspace",
        json!({"workspace": null}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    // Detach falls back to the kiln path (the mock echoes its default).
    assert_eq!(json["workspace"], "/tmp/test-kiln");
}
