//! Session Route Contract Tests (with mock daemon)
//!
//! Most session routes only forwarded one RPC row and are gone
//! ([[Simplification Plan#Step 19]] item 9): the browser calls
//! `rpc(method, params)` through `POST /api/rpc/{method}` now. `create_session`
//! and `export_session` keep their own routes (web-only validation and a
//! two-call composition), so those tests still drive them directly.

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon, start_real_daemon_with_kilns};

/// Drive one request through a fresh mock-daemon-backed app and decode JSON.
async fn send_json(method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
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

/// `POST /api/rpc/{method}` with `body`.
async fn call_rpc(method: &str, body: Value) -> (StatusCode, Value) {
    send_json("POST", &format!("/api/rpc/{method}"), body).await
}

// =========================================================================
// session.list / session.get / session.search Route Contract Tests
// =========================================================================

#[tokio::test]
async fn list_sessions_returns_200() {
    let (status, _json) = call_rpc("session.list", json!({})).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn list_sessions_with_include_archived_returns_200() {
    let (status, _json) = call_rpc("session.list", json!({"include_archived": true})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "include_archived should be accepted"
    );
}

#[tokio::test]
async fn get_session_returns_session_data() {
    let (status, json) = call_rpc("session.get", json!({"session_id": "test-session-001"})).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["state"], "active");
    // `type`, not `session_type`: that is the name the daemon writes
    // (`server/session/list.rs:302`) and the name the browser reads. The mock
    // answered `session_type` until this route named its reply, and this
    // assertion held the mock's spelling rather than the wire's.
    assert_eq!(json["type"], "chat");
}

// =========================================================================
// Session lifecycle Route Contract Tests
// =========================================================================

#[tokio::test]
async fn pause_session_returns_200() {
    let (status, _json) =
        call_rpc("session.pause", json!({"session_id": "test-session-001"})).await;
    assert_eq!(status, StatusCode::OK);
}

// `end_session`, `archive_session` and `delete_session` keep their own REST
// routes ([[Simplification Plan#Step 19]] item 9): each also releases this
// web process's own SSE broker entry for the session
// (`ReconnectingDaemon::close_event_streams`), which a plain
// `POST /api/rpc/{method}` forward has no way to reach.

#[tokio::test]
async fn end_session_returns_200() {
    let (status, _json) = send_json("POST", "/api/session/test-session-001/end", json!({})).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn cancel_session_returns_200_with_cancelled_field() {
    let (status, json) =
        call_rpc("session.cancel", json!({"session_id": "test-session-001"})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        json.get("cancelled").is_some(),
        "Response must contain 'cancelled' field"
    );
}

#[tokio::test]
async fn delete_session_returns_200_with_deleted_field() {
    let (status, json) = send_json("DELETE", "/api/session/test-session-001", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["deleted"], true, "Response must contain deleted: true");
}

#[tokio::test]
async fn archive_session_returns_200_with_archived_true() {
    let (status, json) =
        send_json("POST", "/api/session/test-session-001/archive", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["archived"], true,
        "Response must contain archived: true"
    );
}

#[tokio::test]
async fn unarchive_session_returns_200_with_archived_false() {
    let (status, json) = call_rpc(
        "session.unarchive",
        json!({"session_id": "test-session-001"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["archived"], false,
        "Response must contain archived: false"
    );
}

#[tokio::test]
async fn list_models_returns_200_with_models_array() {
    let (status, json) = call_rpc(
        "session.list_models",
        json!({"session_id": "test-session-001"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        json["models"].is_array(),
        "Response must have 'models' array"
    );
}

// =========================================================================
// session.knob.set / session.set_title Route Contract Tests
// =========================================================================

#[tokio::test]
async fn switch_model_returns_200() {
    let (status, _json) = call_rpc(
        "session.knob.set",
        json!({"session_id": "test-session-001", "knob": "model", "value": "mistral"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn set_mode_returns_200() {
    let (status, _json) = call_rpc(
        "session.knob.set",
        json!({"session_id": "test-session-001", "knob": "mode", "value": "plan"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn set_session_title_returns_200() {
    let (status, _json) = call_rpc(
        "session.set_title",
        json!({"session_id": "test-session-001", "title": "My Chat Session"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// =========================================================================
// Session Creation Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn create_session_returns_200_with_session_id() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kilns": ["test-kiln"],
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

/// The endpoint check lives in the daemon, so this test runs a real one: a
/// mock daemon would only prove what the mock was told to answer. The web
/// forwards the endpoint; the daemon refuses it with `-32602`; the route turns
/// that into a 422 that names the reason.
#[tokio::test]
async fn create_session_with_private_ip_endpoint_returns_422() {
    let (_daemon, client) = start_real_daemon_with_kilns(&[]).await;
    let app = build_test_app(build_state(client));

    for endpoint in [
        "http://10.0.0.1/v1",
        "http://169.254.169.254/latest/meta-data/",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "provider": "openai",
                            "model": "gpt-4o",
                            "endpoint": endpoint,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{endpoint}"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("internal address"), "{endpoint}: {body}");
    }
}

#[tokio::test]
async fn create_session_with_defaults_uses_ollama() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    // Only required field is kilns — provider and model use defaults
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kilns": ["test-kiln"]}).to_string()))
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
    let state = build_state(client);
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

// =========================================================================
// Session Scope (kilns/workspace) Route Contract Tests
// =========================================================================

#[tokio::test]
async fn connect_kiln_returns_scope_shape() {
    let (status, json) = call_rpc(
        "session.connect_kiln",
        json!({"session_id": "test-session-001", "kiln": "extra-kiln"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["kilns"][0], "test-kiln");
    assert_eq!(json["workspace"], "/tmp/test-kiln");
    assert_eq!(json["kilns"][1], "extra-kiln");
}

#[tokio::test]
async fn disconnect_kiln_returns_scope_shape() {
    let (status, json) = call_rpc(
        "session.disconnect_kiln",
        json!({"session_id": "test-session-001", "kiln": "extra-kiln"}),
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
    let (status, json) = call_rpc(
        "session.set_workspace",
        json!({"session_id": "test-session-001", "workspace": "/repos/crucible"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["session_id"], "test-session-001");
    assert_eq!(json["workspace"], "/repos/crucible");
}

#[tokio::test]
async fn set_workspace_null_detaches_to_kiln() {
    let (status, json) = call_rpc(
        "session.set_workspace",
        json!({"session_id": "test-session-001", "workspace": null}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    // Detach falls back to the kiln path (the mock echoes its default).
    assert_eq!(json["workspace"], "/tmp/test-kiln");
}

// =========================================================================
// session.status Route Contract Tests
// =========================================================================

#[tokio::test]
async fn status_route_forwards_every_plugin_slot_verbatim() {
    let (status, json) =
        call_rpc("session.status", json!({"session_id": "test-session-001"})).await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    let slots = json["status"].as_array().expect("status array");
    assert_eq!(slots.len(), 3, "every slot survives: {json}");

    // The engine's plugin-turn item rides first, with its kind, so the
    // browser can place it without a rule of its own.
    assert_eq!(slots[0]["id"], "plugin_turns:goal");
    assert_eq!(slots[0]["kind"], "plugin_turns");
    assert_eq!(slots[0]["pinned"], true);

    // These keys ARE the contract: a rename on either side would surface
    // downstream as blank chips rather than as a failure here.
    assert_eq!(slots[1]["id"], "oci");
    assert_eq!(slots[1]["plugin"], "oci");
    assert_eq!(slots[1]["text"], "sandboxed: alpine:latest");
    assert_eq!(slots[1]["color_group"], "hue-4");

    // A slot from a plugin this crate has never heard of rides through with
    // the same shape — the route interprets no key.
    assert_eq!(slots[2]["id"], "weather");
    assert_eq!(slots[2]["plugin"], "weather");
    assert_eq!(slots[2]["text"], "storm warning");
    assert_eq!(slots[2]["color_group"], "warn");
}

#[tokio::test]
async fn a_session_with_no_plugin_slots_returns_an_empty_status_array() {
    // 200 + empty, never 404: most sessions publish nothing, and a chip strip
    // that treated "quiet" as an error would light up on every one of them.
    let (status, json) = call_rpc("session.status", json!({"session_id": "quiet-session"})).await;

    assert_eq!(status, StatusCode::OK, "body: {json}");
    assert_eq!(json["status"], json!([]));
}

// =========================================================================
// Isolation Passthrough Contract Tests (with mock daemon)
// =========================================================================

/// POST a create body and return the params `session.create` saw on the wire.
async fn create_session_wire_params(body: Value) -> Value {
    let (mock, client) = start_mock_daemon().await;
    let state = build_state(client);
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

    mock.received_params(RpcMethod::SessionCreate)
        .expect("session.create was called")
}

#[tokio::test]
async fn create_session_forwards_the_isolation_value_untouched() {
    // A profile name is the isolating plugin's vocabulary; the web neither
    // validates it nor rewrites it.
    let params = create_session_wire_params(json!({
        "kilns": ["test-kiln"],
        "isolation": "throwaway"
    }))
    .await;
    assert_eq!(params["isolation"], json!("throwaway"));

    // `false` is a real instruction ("no container even if the project has
    // one"), not a falsy value to drop.
    let params = create_session_wire_params(json!({
        "kilns": ["test-kiln"],
        "isolation": false
    }))
    .await;
    assert_eq!(params["isolation"], json!(false));

    // An object the web has no type for reaches the plugin that defined it.
    let params = create_session_wire_params(json!({
        "kilns": ["test-kiln"],
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
    let params = create_session_wire_params(json!({"kilns": ["test-kiln"]})).await;
    assert!(
        params.get("isolation").is_none(),
        "isolation must be absent, not null: {params}"
    );
}
