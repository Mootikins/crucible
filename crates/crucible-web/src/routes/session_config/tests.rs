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

use axum::http::StatusCode;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::routes::session_routes_fail_closed;
use crate::test_support::{build_mock_state, start_mock_daemon, MockDaemon};

use super::basic::{AgentOptionKindRow, AgentOptionsResponse};

/// `(method, uri, body)` → `(status, response JSON, the mock daemon)`.
async fn call(method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value, MockDaemon) {
    let (mock, client) = start_mock_daemon().await;
    // The group carries an OpenAPI document now, and only the axum half of it
    // answers a request.
    let app = axum::Router::from(session_routes_fail_closed()).with_state(build_mock_state(client));

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

/// PUT the knob and assert the value reached the daemon under `wire_field`.
async fn assert_put_reaches_daemon(tail: &str, rpc_method: &str, wire_field: &str, value: Value) {
    let uri = format!("/api/session/s1/config/{tail}");
    let body = json!({ wire_field: value.clone() });
    let (status, _, mock) = call("PUT", &uri, Some(body)).await;

    assert_eq!(status, StatusCode::OK, "PUT {uri} should succeed");
    let params = mock
        .received_params(rpc_method)
        .unwrap_or_else(|| panic!("PUT {uri} did not call {rpc_method}"));
    assert_eq!(
        params.get(wire_field),
        Some(&value),
        "PUT {uri} must forward {wire_field} to {rpc_method} unchanged; \
         params were {params}"
    );
    assert_eq!(
        params.get("session_id").and_then(Value::as_str),
        Some("s1"),
        "the path id must reach the daemon: {params}"
    );
}

/// GET the knob and assert the daemon's answer surfaced under `web_key`.
async fn assert_get_returns(tail: &str, web_key: &str, expected: Value) {
    let uri = format!("/api/session/s1/config/{tail}");
    let (status, body, _) = call("GET", &uri, None).await;

    assert_eq!(status, StatusCode::OK, "GET {uri} should succeed");
    assert_eq!(
        body.get(web_key),
        Some(&expected),
        "GET {uri} must answer {web_key} = {expected}; body was {body}"
    );
}

// ── Context ───────────────────────────────────────────────────────────────

// ── Execution ─────────────────────────────────────────────────────────────

// ── Prompt and enum-valued knobs ──────────────────────────────────────────

#[tokio::test]
async fn context_strategy_round_trips_its_string_spelling() {
    assert_put_reaches_daemon(
        "context-strategy",
        "session.set_context_strategy",
        "context_strategy",
        json!("truncate"),
    )
    .await;
    assert_get_returns("context-strategy", "context_strategy", json!("recent")).await;
}

// ── Nullable knobs ────────────────────────────────────────────────────────

// ── mode, which is not a config/ knob ─────────────────────────────────────

/// `GET /api/session/{id}/mode`. Exempt from gate A2e by design — `mode` has its
/// own route pair because switching it changes tool policy rather than a scalar
/// — so nothing failed while the web could set a mode it could not read.
#[tokio::test]
async fn mode_can_be_read_back_not_only_set() {
    let (status, body, _) = call("GET", "/api/session/s1/mode", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("mode"),
        Some(&json!("plan")),
        "GET mode must answer the daemon's stored mode; body was {body}"
    );
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
        .received_params("session.set_agent_option")
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
