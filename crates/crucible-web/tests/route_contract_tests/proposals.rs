//! Proposal contract tests (with mock daemon).
//!
//! `GET /api/proposals`, `/api/proposals/{id}` and the four decision routes
//! each only forwarded one RPC row (`proposal.list`/`get`/`accept`/
//! `reject`/`dismiss`/`resolve`), so they are gone
//! ([[Simplification Plan#Step 19]]). The browser reaches them through
//! `POST /api/rpc/{method}` now, with the id in the body rather than the
//! path — `id`'s own `INVALID_PARAMS` refusal on a malformed value is a
//! daemon-side, generic `RpcMethod` deserialisation concern now
//! (`a_daemon_invalid_params_error_is_422` in `routes/rpc.rs` covers the
//! mapping), not a route-local pre-check, so this file does not repeat the
//! pre-route "malformed id" case `routes/proposals.rs` used to answer itself.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

/// Each row reaches the daemon with the id the caller named, and the reply
/// reaches the browser as the daemon wrote it.
#[tokio::test]
async fn the_proposal_rows_answer_the_declared_shape() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state.clone());

    let (status, listed) = call_rpc(app, "proposal.list", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let id_text = listed[0]["id"].as_str().expect("an id").to_string();

    let app = build_test_app(state.clone());
    let (status, all) = call_rpc(app, "proposal.list", json!({ "all": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().expect("an array").len(), 2);

    let app = build_test_app(state.clone());
    let (status, got) = call_rpc(app, "proposal.get", json!({ "id": id_text })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["id"], id_text);

    let app = build_test_app(state.clone());
    let (status, accepted) = call_rpc(app, "proposal.accept", json!({ "id": id_text })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["state"]["kind"], "accepted");

    let app = build_test_app(state.clone());
    let (status, one) = call_rpc(
        app,
        "proposal.accept",
        json!({ "id": id_text, "paths": ["notes/a.md"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["title"], "Change notes/a.md");

    let app = build_test_app(state.clone());
    let (status, one) = call_rpc(
        app,
        "proposal.reject",
        json!({ "id": id_text, "paths": ["notes/b.md", "notes/c.md"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["title"], "Change notes/b.md, notes/c.md");

    let app = build_test_app(state.clone());
    let (status, rejected) = call_rpc(
        app,
        "proposal.reject",
        json!({ "id": id_text, "reason": "not now" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["state"]["reason"], "not now");

    let app = build_test_app(state.clone());
    let (status, dismissed) = call_rpc(app, "proposal.dismiss", json!({ "id": id_text })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(dismissed["state"]["kind"], "dismissed");

    let app = build_test_app(state);
    let (status, resolved) = call_rpc(
        app,
        "proposal.resolve",
        json!({ "id": id_text, "path": "a.md", "text": "settled" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resolved["state"]["kind"], "accepted");
}
