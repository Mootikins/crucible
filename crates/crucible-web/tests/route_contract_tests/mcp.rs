//! MCP contract tests (with mock daemon).
//!
//! `GET /api/mcp/status` only forwarded the `mcp.status` RPC row, so it is
//! gone ([[Simplification Plan#Step 19]]). The browser reaches `mcp.status`
//! through `POST /api/rpc/{method}` now.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

#[tokio::test]
async fn mcp_status_answers_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "mcp.status", json!(null)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["running"], json!(false), "{json}");
    assert_eq!(
        json.as_object().expect("an object").len(),
        1,
        "a stopped server reports only `running`: {json}"
    );
}
