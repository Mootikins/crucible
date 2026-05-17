//! Plugin route contract tests (with mock daemon).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

#[tokio::test]
async fn list_plugins_returns_rich_plugin_info() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["plugins"].is_array());

    let plugin = &json["plugins"][0];
    assert_eq!(plugin["name"], "mock-plugin");
    assert_eq!(plugin["version"], "0.1.0");
    assert_eq!(plugin["source"], "User");
    assert_eq!(plugin["state"], "Active");
    assert_eq!(plugin["tools"], 3);
    assert_eq!(plugin["commands"], 1);
    assert_eq!(plugin["handlers"], 2);
    assert_eq!(plugin["services"], 0);
}

#[tokio::test]
async fn reload_plugin_returns_counts() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins/mock-plugin/reload")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "mock-plugin");
    assert_eq!(json["reloaded"], true);
    assert_eq!(json["tools"], 3);
}
