//! Round-trip tests for every session config knob, both directions.
//!
//! **Route existence is not the contract; the field name is.** Gate A2e proves
//! a route exists at `/api/session/{}/config/<tail>`; gate A1 proves the
//! daemon's client and server agree on the JSON field name. Neither proves that
//! the *web's* request struct reads the browser's field or that its response
//! struct answers under a key the frontend can find. That gap is the
//! silent-failure mode CLAUDE.md names: a request struct named after the knob
//! rather than the wire field compiles, passes review, and drops the value.
//!
//! `session.set_execution_timeout` is why: its wire field is `timeout_secs`. A
//! `SetExecutionTimeoutRequest { execution_timeout }` would serialize
//! `{"timeout_secs": null}` to the daemon and 200 back to the browser.
//!
//! So each knob is asserted twice:
//!   * **PUT** — the value the browser sent arrives in the RPC `params` under
//!     the daemon's field name (read off the wire via `received_params`).
//!   * **GET** — the value the daemon answered arrives in the HTTP body under
//!     the web's response key, with a per-knob-distinct value so a route wired
//!     to the wrong knob cannot pass by coincidence.

use crucible_core::protocol::rpc::RpcMethod;

use axum::http::StatusCode;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::routes::session_routes;
use crate::test_support::{build_state, start_mock_daemon, MockDaemon};

use super::basic::{AgentOptionKindRow, AgentOptionsResponse};

/// `(method, uri, body)` → `(status, response JSON, the mock daemon)`.
async fn call(method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value, MockDaemon) {
    let (mock, client) = start_mock_daemon().await;
    // The group carries an OpenAPI document now, and only the axum half of it
    // answers a request.
    let app = axum::Router::from(session_routes()).with_state(build_state(client));

    let builder = axum::http::Request::builder().method(method).uri(uri);
    let request = match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .body(axum::body::Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(axum::body::Body::empty()).unwrap(),
    };

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, mock)
}

/// Decode a reply into the struct its handler declares.
///
/// A status alone proves nothing about a shape: these routes answered
/// `serde_json::Value` until task A6 named their replies, and a renamed field
/// would have passed a 200 check.
fn shaped<T: serde::de::DeserializeOwned>(uri: &str, body: &Value) -> T {
    serde_json::from_value(body.clone())
        .unwrap_or_else(|e| panic!("{uri} answered a body the struct cannot read: {e}\n{body}"))
}

/// PUT `/api/session/{id}/knob` with a [`crucible_core::types::KnobValue`]
/// body and assert it reached `session.knob.set` unchanged.
async fn assert_knob_put_reaches_daemon(value: Value) {
    let (status, _, mock) = call("PUT", "/api/session/s1/knob", Some(value.clone())).await;
    assert_eq!(status, StatusCode::OK, "PUT /knob {value} should succeed");
    let params = mock
        .received_params(RpcMethod::SessionKnobSet)
        .unwrap_or_else(|| panic!("PUT /knob {value} did not call session.knob.set"));
    assert_eq!(
        params.get("knob"),
        value.get("knob"),
        "PUT /knob must forward the knob tag unchanged; params were {params}"
    );
    assert_eq!(
        params.get("value"),
        value.get("value"),
        "PUT /knob must forward the value unchanged; params were {params}"
    );
    assert_eq!(
        params.get("session_id").and_then(Value::as_str),
        Some("s1"),
        "the path id must reach the daemon: {params}"
    );
}

/// GET `/api/session/{id}/knob/{knob}` and assert the reply is the whole
/// [`crucible_core::types::KnobValue`] the mock daemon answered.
async fn assert_knob_get_returns(knob: &str, expected: Value) {
    let uri = format!("/api/session/s1/knob/{knob}");
    let (status, body, _) = call("GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "GET {uri} should succeed");
    assert_eq!(body, expected, "GET {uri} answered {body}");
}

// ── Enum-valued knob ──────────────────────────────────────────────────────

#[tokio::test]
async fn context_strategy_round_trips_its_string_spelling() {
    assert_knob_put_reaches_daemon(json!({"knob": "context_strategy", "value": "truncate"})).await;
    assert_knob_get_returns(
        "context_strategy",
        json!({"knob": "context_strategy", "value": "truncate"}),
    )
    .await;
}

#[tokio::test]
async fn plugin_approval_routes_forward_plugin_and_value() {
    let uri = "/api/session/s1/config/plugins/alpha/approval";
    let (status, _, mock) = call("PUT", uri, Some(json!({"approval": "ask"}))).await;
    assert_eq!(status, StatusCode::OK);
    let params = mock
        .received_params(RpcMethod::SessionSetPluginApproval)
        .unwrap();
    assert_eq!(params["session_id"], "s1");
    assert_eq!(params["plugin"], "alpha");
    assert_eq!(params["approval"], "ask");

    let (status, body, mock) = call("GET", uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"plugin": "alpha", "approval": "ask"}));
    assert_eq!(
        mock.received_params(RpcMethod::SessionGetPluginApproval)
            .unwrap()["plugin"],
        "alpha"
    );

    let (status, body, _) = call("GET", "/api/session/s1/config/plugin-approvals", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["approvals"], json!({"alpha": "ask", "beta": "stop"}));
}

#[tokio::test]
async fn plugin_turn_limit_routes_forward_and_read_session_value() {
    assert_knob_put_reaches_daemon(json!({"knob": "plugin_turn_limit", "value": 7})).await;
    assert_knob_get_returns(
        "plugin_turn_limit",
        json!({"knob": "plugin_turn_limit", "value": 25}),
    )
    .await;
}

// ── mode, a knob like the rest ────────────────────────────────────────────

/// `GET /api/session/{id}/knob/mode`. Mode used to have its own route pair,
/// exempt from gate A2e by design; it is `set_knob`/`get_knob` now, like
/// every other knob.
#[tokio::test]
async fn mode_can_be_read_back_not_only_set() {
    assert_knob_get_returns("mode", json!({"knob": "mode", "value": "plan"})).await;
}

// ── The agent's own settings, which are not Crucible knobs ────────────────

/// `GET /api/session/{id}/config/agent-options` answers the declared shape.
///
/// The kind and the value are read as one: an option is a select carrying
/// choices or a toggle carrying a bool, and the browser's hand-written type
/// declared `current: string | boolean` beside optional choices because
/// nothing described the pairing.
#[tokio::test]
async fn list_agent_options_answers_the_declared_shape() {
    let uri = "/api/session/s1/config/agent-options";
    let (status, body, _) = call("GET", uri, None).await;
    assert_eq!(status, StatusCode::OK, "GET {uri}: {body}");

    let options: AgentOptionsResponse = shaped(uri, &body);
    assert_eq!(options.session_id, "s1");
    assert_eq!(options.options[0].id, "reasoning");
    match &options.options[0].kind {
        AgentOptionKindRow::Select { current, choices } => {
            assert_eq!(current, "medium");
            assert_eq!(choices[1].value, "high");
        }
        AgentOptionKindRow::Toggle { .. } => panic!("the select read back as a toggle: {body}"),
    }
    match &options.options[1].kind {
        AgentOptionKindRow::Toggle { current } => assert!(!current),
        AgentOptionKindRow::Select { .. } => panic!("the toggle read back as a select: {body}"),
    }
}

/// `POST /api/session/{id}/config/agent-options` answers the declared shape.
#[tokio::test]
async fn set_agent_option_answers_the_declared_shape() {
    let uri = "/api/session/s1/config/agent-options";
    let (status, body, mock) = call(
        "POST",
        uri,
        Some(json!({ "option_id": "reasoning", "value": "high" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "POST {uri}: {body}");
    assert_eq!(body, json!({ "ok": true }));

    let params = mock
        .received_params(RpcMethod::SessionSetAgentOption)
        .expect("the POST calls session.set_agent_option");
    assert_eq!(params.get("option_id"), Some(&json!("reasoning")));
    assert_eq!(params.get("value"), Some(&json!("high")));
}

/// The agent's own options survive the reply struct, field for field.
///
/// Built from `crucible_core::types::AgentConfigOption` rather than from a
/// JSON literal, because that is the type the daemon serialises: a field added
/// there fails here instead of disappearing between the daemon and the
/// browser. The flattened kind is the part worth holding — the tag and the
/// value are one object on the wire and two fields in the type.
#[test]
fn the_agent_options_reply_writes_back_the_object_the_daemon_sent() {
    use crucible_core::types::{AgentConfigOption, AgentOptionChoice, AgentOptionKind};

    let sent = json!({
        "session_id": "s1",
        "options": [
            AgentConfigOption {
                id: "reasoning".to_string(),
                name: "Reasoning effort".to_string(),
                description: Some("How long the agent thinks".to_string()),
                category: Some("model".to_string()),
                kind: AgentOptionKind::Select {
                    current: "medium".to_string(),
                    choices: vec![AgentOptionChoice {
                        value: "high".to_string(),
                        name: "High".to_string(),
                    }],
                },
            },
            AgentConfigOption {
                id: "web_search".to_string(),
                name: "Web search".to_string(),
                description: None,
                category: None,
                kind: AgentOptionKind::Toggle { current: true },
            },
        ],
    });

    let reply: AgentOptionsResponse =
        serde_json::from_value(sent.clone()).expect("the reply reads the daemon's object");
    assert_eq!(
        serde_json::to_value(reply).expect("the reply writes JSON"),
        sent,
        "the reply changed the object on the way through"
    );
}
