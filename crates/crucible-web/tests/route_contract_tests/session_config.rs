//! Session Config Endpoint Contract Tests (with mock daemon)
//!
//! Every route these tests used to drive only forwarded one RPC row and is
//! gone ([[Simplification Plan#Step 19]] item 9): the browser calls
//! `rpc(method, params)` through `POST /api/rpc/{method}` now.

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

/// `POST /api/rpc/{method}` with `body`, through a fresh mock-daemon app.
async fn call_rpc(method: &str, body: Value) -> (StatusCode, Value) {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/rpc/{method}"))
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
async fn set_precognition_returns_200() {
    let (status, _json) = call_rpc(
        "session.knob.set",
        json!({"session_id": "test-session-001", "knob": "precognition", "value": true}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn get_precognition_returns_200_with_value_field() {
    let (status, json) = call_rpc(
        "session.knob.get",
        json!({"session_id": "test-session-001", "knob": "precognition"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        json.get("value").is_some(),
        "Response must contain a value field"
    );
}

#[tokio::test]
async fn mode_can_be_read_back_through_the_generic_knob_get() {
    let (status, json) = call_rpc(
        "session.knob.get",
        json!({"session_id": "test-session-001", "knob": "mode"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json, json!({"knob": "mode", "value": "plan"}));
}

#[tokio::test]
async fn plugin_approval_routes_forward_plugin_and_value() {
    let (status, _json) = call_rpc(
        "session.set_plugin_approval",
        json!({"session_id": "s1", "plugin": "alpha", "approval": "ask"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = call_rpc(
        "session.get_plugin_approval",
        json!({"session_id": "s1", "plugin": "alpha"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json, json!({"plugin": "alpha", "approval": "ask"}));

    let (status, json) =
        call_rpc("session.list_plugin_approvals", json!({"session_id": "s1"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["approvals"], json!({"alpha": "ask", "beta": "stop"}));
}

/// The kind and the value are read as one: an option is a select carrying
/// choices or a toggle carrying a bool.
#[tokio::test]
async fn list_agent_options_answers_the_declared_shape() {
    let (status, json) = call_rpc("session.list_agent_options", json!({"session_id": "s1"})).await;
    assert_eq!(status, StatusCode::OK, "body: {json}");

    assert_eq!(json["session_id"], "s1");
    let options = json["options"].as_array().expect("options array");
    assert_eq!(options[0]["id"], "reasoning");
    assert_eq!(options[0]["kind"], "select");
    assert_eq!(options[0]["current"], "medium");
    assert_eq!(options[0]["choices"][1]["value"], "high");
    assert_eq!(options[1]["kind"], "toggle");
    assert_eq!(options[1]["current"], false);
}

#[tokio::test]
async fn set_agent_option_answers_the_declared_shape() {
    let (mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc/session.set_agent_option")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"session_id": "s1", "option_id": "reasoning", "value": "high"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let params = mock
        .received_params(RpcMethod::SessionSetAgentOption)
        .expect("the call reaches session.set_agent_option");
    assert_eq!(params.get("option_id"), Some(&json!("reasoning")));
    assert_eq!(params.get("value"), Some(&json!("high")));
}
