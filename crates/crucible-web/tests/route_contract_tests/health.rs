//! Health Route Tests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_web::routes::health_routes;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn health_check_returns_200_with_json() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
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

    assert_eq!(json["status"], "healthy");
    assert_eq!(json["service"], "crucible-web");
}

#[tokio::test]
async fn health_check_response_is_json_content_type() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("application/json"),
        "Expected JSON content-type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn ready_check_returns_200_with_ready_status() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
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

    // BUG: ready_check always returns "ready" without checking actual readiness
    assert_eq!(json["status"], "ready");
}

#[tokio::test]
async fn health_nonexistent_route_returns_404() {
    let app = Router::new().merge(health_routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
