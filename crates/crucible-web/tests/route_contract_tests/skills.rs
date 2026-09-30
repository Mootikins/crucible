//! Skills contract tests (with mock daemon).
//!
//! `GET /api/skills`, `/api/skills/{name}` and `/api/skills/search` are gone
//! ([[Simplification Plan#Step 19]] item 3, the "migration"): a REST route
//! that only forwarded one RPC row. The browser reaches `skills.list`,
//! `skills.get` and `skills.search` through `POST /api/rpc/{method}` now, so
//! these tests drive that one route instead.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

async fn call_rpc(app: axum::Router, method: &str, body: Value) -> (StatusCode, Value) {
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
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

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
