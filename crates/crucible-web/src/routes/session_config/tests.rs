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

/// `(method, uri, body)` → `(status, response JSON, the mock daemon)`.
async fn call(method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value, MockDaemon) {
    let (mock, client) = start_mock_daemon().await;
    let app = session_routes_fail_closed().with_state(build_mock_state(client));

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

#[tokio::test]
async fn context_budget_round_trips() {
    assert_put_reaches_daemon(
        "context-budget",
        "session.set_context_budget",
        "context_budget",
        json!(8000),
    )
    .await;
    assert_get_returns("context-budget", "context_budget", json!(111)).await;
}

#[tokio::test]
async fn context_window_round_trips() {
    assert_put_reaches_daemon(
        "context-window",
        "session.set_context_window",
        "context_window",
        json!(32000),
    )
    .await;
    assert_get_returns("context-window", "context_window", json!(222)).await;
}

/// Compared with a tolerance, not for equality: the daemon's setter takes
/// `Option<f32>`, so `0.9` from the browser is narrowed to f32 and widened again
/// for JSON, arriving as `0.8999999761581421`. That is the daemon's field type,
/// not a fault in this route — asserting exact equality here would encode a
/// precision the wire does not have. `0.75` survives exactly because it is
/// representable in binary.
#[tokio::test]
async fn autocompact_threshold_round_trips() {
    let uri = "/api/session/s1/config/autocompact-threshold";
    let (status, _, mock) = call("PUT", uri, Some(json!({ "autocompact_threshold": 0.9 }))).await;
    assert_eq!(status, StatusCode::OK);

    let params = mock
        .received_params("session.set_autocompact_threshold")
        .expect("PUT did not call session.set_autocompact_threshold");
    let sent = params
        .get("autocompact_threshold")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("autocompact_threshold missing from {params}"));
    assert!(
        (sent - 0.9).abs() < 1e-6,
        "PUT must forward ~0.9, got {sent} (params {params})"
    );

    assert_get_returns(
        "autocompact-threshold",
        "autocompact_threshold",
        json!(0.75),
    )
    .await;
}

// ── Execution ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn max_iterations_round_trips() {
    assert_put_reaches_daemon(
        "max-iterations",
        "session.set_max_iterations",
        "max_iterations",
        json!(12),
    )
    .await;
    assert_get_returns("max-iterations", "max_iterations", json!(33)).await;
}

/// The knob is `execution_timeout`; the wire field is `timeout_secs`. This is
/// the test that fails if the web structs are named after the knob — which is
/// exactly what a reviewer reading `session.set_execution_timeout` would write.
#[tokio::test]
async fn execution_timeout_round_trips_under_timeout_secs_not_execution_timeout() {
    assert_put_reaches_daemon(
        "execution-timeout",
        "session.set_execution_timeout",
        "timeout_secs",
        json!(300),
    )
    .await;
    assert_get_returns("execution-timeout", "timeout_secs", json!(44)).await;
}

#[tokio::test]
async fn validation_retries_round_trips() {
    assert_put_reaches_daemon(
        "validation-retries",
        "session.set_validation_retries",
        "validation_retries",
        json!(3),
    )
    .await;
    assert_get_returns("validation-retries", "validation_retries", json!(5)).await;
}

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

#[tokio::test]
async fn output_validation_round_trips_its_string_spelling() {
    assert_put_reaches_daemon(
        "output-validation",
        "session.set_output_validation",
        "output_validation",
        json!("lenient"),
    )
    .await;
    assert_get_returns("output-validation", "output_validation", json!("strict")).await;
}

#[tokio::test]
async fn system_prompt_round_trips() {
    assert_put_reaches_daemon(
        "system-prompt",
        "session.set_system_prompt",
        "system_prompt",
        json!("you are a librarian"),
    )
    .await;
    assert_get_returns("system-prompt", "system_prompt", json!("be terse")).await;
}

// ── Nullable knobs ────────────────────────────────────────────────────────

/// Clearing an optional knob must never reach the daemon as a *value*.
///
/// It arrives as an omitted field rather than an explicit `null`, because the
/// daemon's own client request structs carry
/// `#[serde(skip_serializing_if = "Option::is_none")]`. That is fine HERE, and
/// the distinction the test originally asserted does not exist for these knobs:
/// the server reads them with `optional_param!(req, …)`, which maps absent and
/// `null` alike to `None`, and the setter always writes an `Option` — there is no
/// "leave unchanged" branch for an omitted field to fall into.
///
/// What would be a real bug is the browser's `null` being coerced to a value
/// (`0`, or the knob's default) somewhere between the web request struct and the
/// wire. That is what this asserts.
#[tokio::test]
async fn clearing_an_optional_knob_never_sends_a_value() {
    for (tail, rpc_method, wire_field) in [
        (
            "context-budget",
            "session.set_context_budget",
            "context_budget",
        ),
        (
            "context-window",
            "session.set_context_window",
            "context_window",
        ),
        (
            "autocompact-threshold",
            "session.set_autocompact_threshold",
            "autocompact_threshold",
        ),
        (
            "max-iterations",
            "session.set_max_iterations",
            "max_iterations",
        ),
        (
            "execution-timeout",
            "session.set_execution_timeout",
            "timeout_secs",
        ),
    ] {
        let uri = format!("/api/session/s1/config/{tail}");
        let (status, _, mock) = call("PUT", &uri, Some(json!({ wire_field: Value::Null }))).await;
        assert_eq!(status, StatusCode::OK, "PUT {uri} with null should succeed");

        let params = mock
            .received_params(rpc_method)
            .unwrap_or_else(|| panic!("PUT {uri} did not call {rpc_method}"));
        match params.get(wire_field) {
            None | Some(Value::Null) => {}
            Some(other) => panic!(
                "clearing {tail} sent {wire_field} = {other}; a cleared knob must \
                 never reach the daemon as a value (params {params})"
            ),
        }
    }
}

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
