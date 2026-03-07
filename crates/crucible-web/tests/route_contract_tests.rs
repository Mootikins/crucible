//! Contract tests for crucible-web HTTP routes.
//!
//! Tests the HTTP API contract (status codes, response shapes, content types)
//! WITHOUT requiring a running daemon. Uses a mock Unix socket daemon to handle
//! JSON-RPC calls from the DaemonClient.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_config::CliAppConfig;
use crucible_daemon::DaemonClient;
use crucible_web::routes::{
    chat_routes, health_routes, project_routes, search_routes, session_routes,
};
use crucible_web::services::daemon::{AppState, EventBroker};
use crucible_web::{ChatEvent, WebError};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tower::ServiceExt;

// =========================================================================
// Mock Daemon Infrastructure
// =========================================================================

/// A mock daemon that listens on a Unix socket and responds to JSON-RPC calls
/// with canned responses. This allows testing HTTP routes without a real daemon.
struct MockDaemon {
    _tmp: TempDir,
}

/// Start a mock daemon on a temporary Unix socket. Returns the mock daemon
/// handle (holds TempDir alive) and a connected DaemonClient.
async fn start_mock_daemon() -> (MockDaemon, DaemonClient) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let socket_path = tmp.path().join("mock-daemon.sock");

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind mock socket");

    // Spawn mock daemon server
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        let (read, mut write) = stream.into_split();
                        let mut reader = BufReader::new(read);
                        let mut line = String::new();

                        loop {
                            line.clear();
                            match reader.read_line(&mut line).await {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    let msg: Value = match serde_json::from_str(&line) {
                                        Ok(m) => m,
                                        Err(_) => continue,
                                    };

                                    let id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let method =
                                        msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                                    let result = mock_rpc_response(method, &msg);

                                    let response = json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": result
                                    });

                                    let mut resp_str = serde_json::to_string(&response).unwrap();
                                    resp_str.push('\n');

                                    if write.write_all(resp_str.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Give the listener a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect to mock daemon");

    (MockDaemon { _tmp: tmp }, client)
}

/// Generate mock RPC responses based on method name.
fn mock_rpc_response(method: &str, _msg: &Value) -> Value {
    match method {
        "kiln.list" => json!([]),
        "list_notes" => json!([]),
        "get_note_by_name" => Value::Null,
        "note.upsert" => json!({}),
        "search_vectors" => json!([]),
        "session.create" => json!({"session_id": "test-session-001"}),
        "session.list" => json!([]),
        "session.get" => json!({
            "session_id": "test-session-001",
            "state": "active",
            "session_type": "chat"
        }),
        "session.pause" => json!({"ok": true}),
        "session.resume" => json!({"ok": true}),
        "session.end" => json!({"ok": true}),
        "session.cancel" => json!({"cancelled": true}),
        "session.subscribe" => json!(null),
        "session.configure_agent" => json!(null),
        "session.send_message" => json!({"message_id": "msg-001"}),
        "session.interaction_respond" => json!(null),
        "session.list_models" => json!({"models": ["llama3.2", "mistral"]}),
        "session.switch_model" => json!(null),
        "session.set_title" => json!(null),
        "session.resume_from_storage" => json!({"messages": [], "session_id": "test-session-001"}),
        "project.list" => json!([]),
        "project.register" => json!({
            "path": "/tmp/test-project",
            "name": "test-project",
            "kilns": [],
            "last_accessed": "2025-01-01T00:00:00Z"
        }),
        "project.unregister" => json!(null),
        "project.get" => Value::Null,
        _ => json!(null),
    }
}

/// Build an AppState using a mock daemon client.
fn build_mock_state(client: DaemonClient) -> AppState {
    AppState {
        daemon: Arc::new(client),
        events: Arc::new(EventBroker::new()),
        config: Arc::new(CliAppConfig::default()),
        http_client: reqwest::Client::new(),
    }
}

/// Build the full app router with mock state.
fn build_test_app(state: AppState) -> Router {
    Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
        .with_state(state)
        .merge(health_routes())
}

// =========================================================================
// Health Route Tests
// =========================================================================

#[tokio::test]
async fn health_check_returns_200_with_json() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
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

    assert_eq!(json["status"], "healthy");
    assert_eq!(json["service"], "crucible-web");
}

#[tokio::test]
async fn health_check_response_is_json_content_type() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("application/json"),
        "Expected JSON content-type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn ready_check_returns_200_with_ready_status() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
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

    // BUG: ready_check always returns "ready" without checking actual readiness
    assert_eq!(json["status"], "ready");
}

#[tokio::test]
async fn health_nonexistent_route_returns_404() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// WebError Response Contract Tests
// =========================================================================

#[test]
fn web_error_config_returns_500() {
    let err = WebError::Config("bad config".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn web_error_chat_returns_400() {
    let err = WebError::Chat("invalid message".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn web_error_daemon_returns_502() {
    let err = WebError::Daemon("daemon unreachable".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[test]
fn web_error_validation_returns_422() {
    let err = WebError::Validation("invalid input".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn web_error_not_found_returns_404() {
    let err = WebError::NotFound("missing resource".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn web_error_internal_returns_500() {
    let err = WebError::Internal("unexpected failure".to_string());
    let response = axum::response::IntoResponse::into_response(err);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn web_error_body_contains_error_code_and_message() {
    let err = WebError::Chat("test error message".to_string());
    let response = axum::response::IntoResponse::into_response(err);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Contract: error responses have { "error": { "code": N, "message": "..." } }
    assert!(json.get("error").is_some(), "Response must have 'error' key");
    assert_eq!(json["error"]["code"], 400);
    assert_eq!(json["error"]["message"], "test error message");
}

#[tokio::test]
async fn web_error_io_returns_500_with_message() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err = WebError::Io(io_err);
    let response = axum::response::IntoResponse::into_response(err);

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], 500);
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("file missing"));
}

// =========================================================================
// ChatEvent Contract Tests
// =========================================================================

#[test]
fn chat_event_token_event_name() {
    let event = ChatEvent::Token {
        content: "hello".to_string(),
    };
    assert_eq!(event.event_name(), "token");
}

#[test]
fn chat_event_tool_call_event_name() {
    let event = ChatEvent::ToolCall {
        id: "1".to_string(),
        title: "search".to_string(),
        arguments: None,
    };
    assert_eq!(event.event_name(), "tool_call");
}

#[test]
fn chat_event_tool_result_event_name() {
    let event = ChatEvent::ToolResult {
        id: "1".to_string(),
        result: Some("found it".to_string()),
    };
    assert_eq!(event.event_name(), "tool_result");
}

#[test]
fn chat_event_thinking_event_name() {
    let event = ChatEvent::Thinking {
        content: "hmm".to_string(),
    };
    assert_eq!(event.event_name(), "thinking");
}

#[test]
fn chat_event_message_complete_event_name() {
    let event = ChatEvent::MessageComplete {
        id: "1".to_string(),
        content: "done".to_string(),
        tool_calls: vec![],
    };
    assert_eq!(event.event_name(), "message_complete");
}

#[test]
fn chat_event_error_event_name() {
    let event = ChatEvent::Error {
        code: "500".to_string(),
        message: "oops".to_string(),
    };
    assert_eq!(event.event_name(), "error");
}

#[test]
fn chat_event_token_serializes_with_type_tag() {
    let event = ChatEvent::Token {
        content: "hello world".to_string(),
    };
    let json: Value = serde_json::to_value(&event).unwrap();

    // Contract: ChatEvent uses { "type": "token", ... } tagged format
    assert_eq!(json["type"], "token");
    assert_eq!(json["content"], "hello world");
}

#[test]
fn chat_event_error_serializes_with_type_tag() {
    let event = ChatEvent::Error {
        code: "rate_limit".to_string(),
        message: "Too many requests".to_string(),
    };
    let json: Value = serde_json::to_value(&event).unwrap();

    assert_eq!(json["type"], "error");
    assert_eq!(json["code"], "rate_limit");
    assert_eq!(json["message"], "Too many requests");
}

#[test]
fn chat_event_message_complete_omits_empty_tool_calls() {
    let event = ChatEvent::MessageComplete {
        id: "msg-1".to_string(),
        content: "response text".to_string(),
        tool_calls: vec![],
    };
    let json: Value = serde_json::to_value(&event).unwrap();

    // Contract: empty tool_calls should be omitted (skip_serializing_if)
    assert!(
        json.get("tool_calls").is_none(),
        "Empty tool_calls should be omitted from serialization"
    );
}

#[test]
fn chat_event_from_daemon_text_delta() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "text_delta".to_string(),
        data: json!({"content": "chunk"}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "token");

    let json: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["content"], "chunk");
}

#[test]
fn chat_event_from_daemon_thinking_delta() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "thinking_delta".to_string(),
        data: json!({"content": "reasoning..."}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "thinking");
}

#[test]
fn chat_event_from_daemon_tool_call() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "tool_call_start".to_string(),
        data: json!({"id": "tc-1", "name": "search", "arguments": {"query": "test"}}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "tool_call");

    let json: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["id"], "tc-1");
    assert_eq!(json["title"], "search");
}

#[test]
fn chat_event_from_daemon_error() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "error".to_string(),
        data: json!({"code": "provider_error", "message": "API down"}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "error");

    let json: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["code"], "provider_error");
    assert_eq!(json["message"], "API down");
}

#[test]
fn chat_event_from_daemon_turn_complete() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "turn_complete".to_string(),
        data: json!({"message_id": "msg-99", "full_response": "Final answer"}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "message_complete");

    let json: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["id"], "msg-99");
    assert_eq!(json["content"], "Final answer");
}

#[test]
fn chat_event_from_daemon_unknown_maps_to_session_event() {
    let daemon_event = crucible_daemon::SessionEvent {
        session_id: "s1".to_string(),
        event_type: "custom_plugin_event".to_string(),
        data: json!({"key": "value"}),
    };
    let event = ChatEvent::from_daemon_event(&daemon_event);
    assert_eq!(event.event_name(), "session_event");
}

// =========================================================================
// Chat Route Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn chat_send_empty_message_returns_400() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id": "s1", "content": "  "}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_send_valid_message_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id": "test-session-001", "content": "Hello"}).to_string(),
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
        json.get("message_id").is_some(),
        "Response must contain message_id"
    );
}

#[tokio::test]
async fn chat_send_missing_fields_returns_422() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Missing content field
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from(json!({"session_id": "s1"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn chat_send_invalid_json_returns_error() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat/send")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400 for JSON parse errors
    assert!(
        response.status().is_client_error(),
        "Invalid JSON should return client error, got: {}",
        response.status()
    );
}

// =========================================================================
// Search/Kiln Route Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn list_kilns_returns_200_with_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/kilns")
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
        json["kilns"].is_array(),
        "Response must have 'kilns' array"
    );
}

#[tokio::test]
async fn list_notes_requires_kiln_query_param() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Missing required 'kiln' query parameter
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400/422 for missing required query parameters
    assert!(
        response.status().is_client_error(),
        "Missing kiln param should return client error, got: {}",
        response.status()
    );
}

#[tokio::test]
async fn list_notes_with_kiln_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/notes?kiln=/tmp/test-kiln")
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
        json["notes"].is_array(),
        "Response must have 'notes' array"
    );
}

#[tokio::test]
async fn search_vectors_returns_200_with_results() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search/vectors")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "vector": [0.1, 0.2, 0.3],
                        "limit": 5
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
        json["results"].is_array(),
        "Response must have 'results' array"
    );
}

// =========================================================================
// Project Route Contract Tests (with mock daemon)
// =========================================================================

#[tokio::test]
async fn list_projects_returns_200_with_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/project/list")
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
    assert!(json.is_array(), "Response must be an array of projects");
}

#[tokio::test]
async fn register_project_returns_200_with_project() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"path": "/tmp/test-project"}).to_string(),
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
    assert!(json.get("name").is_some(), "Project must have a name");
    assert!(json.get("path").is_some(), "Project must have a path");
}

#[tokio::test]
async fn unregister_project_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project/unregister")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"path": "/tmp/test-project"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_project_missing_returns_404() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/project/get?path=/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// Session Route Contract Tests (with mock daemon)
// =========================================================================

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
                .body(Body::from(
                    json!({"model_id": "mistral"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"title": "My Chat Session"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// =========================================================================
// Router Wiring Contract Tests
// =========================================================================

#[tokio::test]
async fn get_on_post_only_route_returns_method_not_allowed() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // /api/chat/send is POST-only
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/chat/send")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn unknown_api_route_returns_404() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// Providers Route Contract Test
// =========================================================================

#[tokio::test]
async fn list_providers_returns_200_with_providers_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/providers")
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
        json["providers"].is_array(),
        "Response must have 'providers' array"
    );
}
