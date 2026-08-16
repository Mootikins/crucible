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
                        "kilns": ["/tmp/test-kiln"],
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
                        "kilns": ["/tmp/test-kiln"],
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

    // Only required field is kilns — provider and model use defaults
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kilns": ["/tmp/test-kiln"]}).to_string()))
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
    assert_eq!(json["kilns"][0], "/tmp/test-kiln");
    assert_eq!(json["workspace"], "/tmp/test-kiln");
    assert_eq!(json["kilns"][1], "/tmp/extra-kiln");
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
    assert_eq!(
        json["kilns"].as_array().unwrap().len(),
        1,
        "disconnect drops the detached kiln: {json}"
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

// =========================================================================
// Plugin Status Route Contract Tests (with mock daemon)
// =========================================================================

/// Drive one GET through a fresh mock-daemon-backed app and decode JSON.
async fn get_json(uri: &str) -> (StatusCode, Value) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
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
async fn status_route_forwards_every_plugin_slot_verbatim() {
    let (status, json) = get_json("/api/session/test-session-001/status").await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    let slots = json["status"].as_array().expect("status array");
    assert_eq!(slots.len(), 2, "both slots survive: {json}");

    // The four keys ARE the contract: a rename on either side would surface
    // downstream as blank chips rather than as a failure here.
    assert_eq!(slots[0]["key"], "oci");
    assert_eq!(slots[0]["plugin"], "oci");
    assert_eq!(slots[0]["text"], "sandboxed: alpine:latest");
    assert_eq!(slots[0]["level"], "info");

    // A slot from a plugin this crate has never heard of rides through with
    // the same shape — the route interprets no key.
    assert_eq!(slots[1]["key"], "weather");
    assert_eq!(slots[1]["plugin"], "weather");
    assert_eq!(slots[1]["text"], "storm warning");
    assert_eq!(slots[1]["level"], "warn");
}

#[tokio::test]
async fn a_session_with_no_plugin_slots_returns_an_empty_status_array() {
    // 200 + empty, never 404: most sessions publish nothing, and a chip strip
    // that treated "quiet" as an error would light up on every one of them.
    let (status, json) = get_json("/api/session/quiet-session/status").await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["status"], json!([]));
}

// =========================================================================
// Isolation Passthrough Contract Tests (with mock daemon)
// =========================================================================

/// POST a create body and return the params `session.create` saw on the wire.
async fn create_session_wire_params(body: Value) -> Value {
    let (mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    mock.received_params("session.create")
        .expect("session.create was called")
}

#[tokio::test]
async fn create_session_forwards_the_isolation_value_untouched() {
    // A profile name is the isolating plugin's vocabulary; the web neither
    // validates it nor rewrites it.
    let params = create_session_wire_params(json!({
        "kilns": ["/tmp/test-kiln"],
        "isolation": "throwaway"
    }))
    .await;
    assert_eq!(params["isolation"], json!("throwaway"));

    // `false` is a real instruction ("no container even if the project has
    // one"), not a falsy value to drop.
    let params = create_session_wire_params(json!({
        "kilns": ["/tmp/test-kiln"],
        "isolation": false
    }))
    .await;
    assert_eq!(params["isolation"], json!(false));

    // An object the web has no type for reaches the plugin that defined it.
    let params = create_session_wire_params(json!({
        "kilns": ["/tmp/test-kiln"],
        "isolation": {"image": "docker.io/library/alpine:latest"}
    }))
    .await;
    assert_eq!(
        params["isolation"],
        json!({"image": "docker.io/library/alpine:latest"})
    );
}

#[tokio::test]
async fn create_session_without_isolation_omits_the_field_from_the_wire() {
    // Absent ("resolve normally") and `false` ("no container") are different
    // instructions to the plugin; a `null` on the wire would collapse them.
    let params = create_session_wire_params(json!({"kilns": ["/tmp/test-kiln"]})).await;
    assert!(
        params.get("isolation").is_none(),
        "isolation must be absent, not null: {params}"
    );
}
