//! Agents/models contract tests (with mock daemon).
//!
//! `GET /api/agents` and `GET /api/models` each only forwarded one RPC row
//! (`agents.list_profiles`, `models.list`), so they are gone
//! ([[Simplification Plan#Step 19]]). The browser reaches them through
//! `POST /api/rpc/{method}` now.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

/// The daemon builds the rows, and the reply deserialises back into the
/// declared type. A row that lost the probe verdict on the way through fails
/// here.
#[tokio::test]
async fn list_agents_answers_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "agents.list_profiles", json!(null)).await;
    assert_eq!(status, StatusCode::OK);

    let profiles = json["profiles"].as_array().expect("a profiles array");
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0]["name"], "claude");
    assert!(!profiles[0]["available"].as_bool().unwrap());
    assert_eq!(profiles[1]["name"], "opencode");
    assert!(profiles[1]["available"].as_bool().unwrap());
    assert!(profiles[0]["is_builtin"].as_bool().unwrap());
}

#[tokio::test]
async fn list_all_models_works_without_a_session() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "models.list", json!({})).await;
    assert_eq!(status, StatusCode::OK);

    let models = json["models"].as_array().expect("models array");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0], "ollama/llama3.2");
}
