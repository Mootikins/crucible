//! Skills contract tests (with mock daemon).
//!
//! `GET /api/skills`, `/api/skills/{name}` and `/api/skills/search` are gone
//! ([[Simplification Plan#Step 19]] item 3, the "migration"): a REST route
//! that only forwarded one RPC row. The browser reaches `skills.list`,
//! `skills.get` and `skills.search` through `POST /api/rpc/{method}` now, so
//! these tests drive that one route instead.

use axum::http::StatusCode;
use serde_json::json;

use super::shared::{build_state, build_test_app, call_rpc, start_mock_daemon};

#[tokio::test]
async fn list_skills_returns_200_with_skills_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) =
        call_rpc(app, "skills.list", json!({ "kiln_path": "/tmp/test-kiln" })).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["skills"].is_array(), "must have skills array");
    assert_eq!(json["skills"][0]["name"], "test-skill");
}

#[tokio::test]
async fn list_skills_accepts_scope_filter() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, _json) = call_rpc(
        app,
        "skills.list",
        json!({ "kiln_path": "/tmp/test-kiln", "scope_filter": "user" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn get_skill_returns_200_with_body() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(
        app,
        "skills.get",
        json!({ "name": "test-skill", "kiln_path": "/tmp/test-kiln" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["name"], "test-skill");
    assert!(json["body"].as_str().unwrap().contains("Test Skill"));
}

#[tokio::test]
async fn search_skills_returns_200_with_matches() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(
        app,
        "skills.search",
        json!({ "query": "match", "kiln_path": "/tmp/test-kiln", "limit": 5 }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["skills"].is_array());
    assert_eq!(json["skills"][0]["name"], "matched-skill");
}
