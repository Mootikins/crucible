//! Execute Command Contract Tests (with mock daemon)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

#[tokio::test]
async fn command_help_returns_help_text() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/help"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    assert!(result.contains("/help"), "Help text should mention /help");
    assert!(
        result.contains("/search"),
        "Help text should mention /search"
    );
    assert!(
        result.contains("/models"),
        "Help text should mention /models"
    );
    assert!(result.contains("/clear"), "Help text should mention /clear");
    assert!(
        result.contains("/export"),
        "Help text should mention /export"
    );
    assert!(result.contains("/model"), "Help text should mention /model");
}

#[tokio::test]
async fn command_search_no_args_returns_error() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/search"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "error");
    assert!(
        json["result"].as_str().unwrap().contains("Usage"),
        "Error should contain usage hint"
    );
}

#[tokio::test]
async fn command_search_with_query_returns_results() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/search test query"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    assert!(
        result.contains("test query"),
        "Result should reference the search query"
    );
    assert!(
        result.contains("Test Session"),
        "Result should contain mock session title"
    );
}

#[tokio::test]
async fn command_models_returns_model_list() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/models"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    assert!(result.contains("llama3.2"), "Should list llama3.2 model");
    assert!(result.contains("mistral"), "Should list mistral model");
}

#[tokio::test]
async fn command_model_no_args_returns_error() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/model"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "error");
    assert!(
        json["result"].as_str().unwrap().contains("Usage"),
        "Error should contain usage hint"
    );
}

#[tokio::test]
async fn command_model_with_name_switches_model() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/model mistral"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    assert!(
        json["result"].as_str().unwrap().contains("mistral"),
        "Result should confirm model switch to mistral"
    );
}

#[tokio::test]
async fn command_clear_returns_cleared() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/clear"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    // Pins the honest wording: /clear only clears the browser view; the
    // daemon-side history is untouched (TUI end+recreate parity deferred).
    assert!(
        json["result"]
            .as_str()
            .unwrap()
            .contains("server-side history preserved"),
        "Result must state view-only clear semantics"
    );
}

#[tokio::test]
async fn command_export_returns_hint() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/export"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    assert!(
        json["result"].as_str().unwrap().contains("export"),
        "Result should mention export"
    );
}

#[tokio::test]
async fn command_unknown_returns_error_type() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/foobar"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "error");
    let result = json["result"].as_str().unwrap();
    assert!(
        result.contains("Unknown command"),
        "Error should say unknown command"
    );
    assert!(
        result.contains("foobar"),
        "Error should echo the unknown command name"
    );
}

#[tokio::test]
async fn command_without_slash_prefix_works() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Send "help" without leading "/" — should still work
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"help"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    assert!(
        json["result"].as_str().unwrap().contains("/help"),
        "Help text should work without leading slash"
    );
}

#[tokio::test]
async fn command_with_whitespace_padding_works() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Send command with extra whitespace
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"  /clear  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "success");
    assert!(
        json["result"].as_str().unwrap().contains("cleared"),
        "Command should work with whitespace padding"
    );
}
