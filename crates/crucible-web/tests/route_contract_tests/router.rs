//! Router Wiring + Providers Route Contract Tests
//!
//! `GET /api/providers` only forwarded `providers.list` and is gone
//! ([[Simplification Plan#Step 19]] item 9); the browser calls
//! `rpc('providers.list', ...)` now.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

#[tokio::test]
async fn get_on_post_only_route_returns_method_not_allowed() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    // /api/rpc/{method} is POST-only
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rpc/session.get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn unknown_api_route_returns_404() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_providers_returns_200_with_providers_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc/providers.list")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["providers"].is_array(),
        "Response must have 'providers' array"
    );
}
