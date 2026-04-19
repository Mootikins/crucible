//! ChatEvent Contract Tests and Chat Route Contract Tests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use crucible_web::ChatEvent;
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

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
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        cache_read_tokens: None,
        cache_creation_tokens: None,
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
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        cache_read_tokens: None,
        cache_creation_tokens: None,
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
