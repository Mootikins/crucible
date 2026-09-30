//! Project Route Contract Tests (with mock daemon)
//!
//! `GET /api/project/list`, `POST /api/project/unregister` and
//! `GET /api/project/get` are gone ([[Simplification Plan#Step 19]] item 3,
//! the "migration"): each only forwarded one RPC row. The browser reaches
//! `project.list`, `project.unregister` and `project.get` through
//! `POST /api/rpc/{method}` now. `POST /api/project/register` stays: its
//! `[web] registration_roots` check and its rollback are web-owned
//! behavior.

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
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn project_list_returns_200_with_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "project.list", json!(null)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array(), "Response must be an array of projects");
}

/// POST a register request for `path` against an app whose
/// `[web] registration_roots` is `roots`.
async fn register(roots: &[&std::path::Path], path: &std::path::Path) -> (StatusCode, Value) {
    use crucible_core::config::{CliAppConfig, WebConfig};

    let (_mock, client) = start_mock_daemon().await;
    let config = CliAppConfig {
        web: Some(WebConfig {
            registration_roots: roots.iter().map(|p| p.display().to_string()).collect(),
            ..WebConfig::default()
        }),
        ..CliAppConfig::default()
    };
    let app = build_test_app(crucible_web::test_support::build_state_with_config(
        client, config,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "path": path.display().to_string() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn register_project_returns_200_for_a_path_inside_a_registration_root() {
    // Registering a project root also grants the file API read access to
    // everything beneath it, so the request has to land inside a configured
    // root. The mock daemon answers with a fixed `/tmp/test-project`, which the
    // handler containment-checks too — hence the temp dir in the root list.
    let base = tempfile::tempdir().unwrap();
    let project = base.path().join("app");
    std::fs::create_dir(&project).unwrap();

    let (status, json) = register(&[base.path(), &std::env::temp_dir()], &project).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.get("name").is_some(), "Project must have a name");
    assert!(json.get("path").is_some(), "Project must have a path");
}

#[tokio::test]
async fn register_project_returns_403_for_a_path_outside_every_registration_root() {
    // Was asserted as 200 before root containment existed: an unauthenticated-
    // by-loopback POST could name any directory (up to and including `/`) and
    // turn it into a read scope for `/api/file/raw`.
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    let (status, _) = register(&[base.path()], outside.path()).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_project_returns_403_for_the_filesystem_root() {
    let base = tempfile::tempdir().unwrap();

    let (status, _) = register(&[base.path()], std::path::Path::new("/")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn project_unregister_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, _) = call_rpc(
        app,
        "project.unregister",
        json!({ "path": "/tmp/test-project" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

/// `project.get`'s reply is `Option<Project>`, so a path with no
/// registration is `null`, not a 404 — the route that used to translate
/// the option into a status is gone (Simplification Plan step 19 item 3).
#[tokio::test]
async fn project_get_missing_returns_null() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let (status, json) = call_rpc(app, "project.get", json!({ "path": "/nonexistent" })).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_null(), "{json}");
}
