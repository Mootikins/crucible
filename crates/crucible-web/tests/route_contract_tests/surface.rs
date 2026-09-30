//! Surface contract tests (with mock daemon).
//!
//! `GET /api/surfaces` only forwarded the `surface.list` RPC row, so it is
//! gone ([[Simplification Plan#Step 19]]). The browser reaches `surface.list`
//! through `POST /api/rpc/{method}` now.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

/// Rows come with the list because a surface is a panel, not a feed: fetching
/// each one separately would draw an empty sidebar first.
#[tokio::test]
async fn surface_list_answers_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "surface.list", json!({})).await;
    assert_eq!(status, StatusCode::OK);

    let panel = &json["surfaces"][0];
    assert_eq!(panel["name"], "sessions");
    assert_eq!(panel["shape"], "list");
    // About the plugin rather than about one session, and the key is
    // written either way.
    assert_eq!(panel["session"], json!(null));
    assert_eq!(panel["rows"][0]["mark"], "busy");
    // A line with no status. `null` is "no status", never "unknown".
    assert_eq!(panel["rows"][1]["mark"], json!(null));
    assert_eq!(panel["rows"][1]["detail"], json!(null));
}
