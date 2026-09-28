//! Health Route Tests

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_daemon::test_support::InProcessDaemon;
use crucible_web::routes::health_routes;
use crucible_web::test_support::{
    build_state, start_mock_daemon_with_errors, start_real_daemon_with_kilns, MockErrors,
};
use serde_json::Value;
use tower::ServiceExt;

/// The app plus the real daemon that backs it. The handle owns the data home
/// of the daemon, so a caller that drops it takes the daemon with it.
async fn health_app() -> (Router, InProcessDaemon) {
    let (daemon, client) = start_real_daemon_with_kilns(&[]).await;
    let app = Router::new().merge(health_routes(build_state(client)));
    (app, daemon)
}

async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn health_check_returns_200_with_json() {
    let (app, _daemon) = health_app().await;

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

    let json = body_json(response).await;
    assert_eq!(json["status"], "healthy");
    assert_eq!(json["service"], "crucible-web");
}

#[tokio::test]
async fn health_check_response_is_json_content_type() {
    let (app, _daemon) = health_app().await;

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

/// Readiness is the daemon answering, not the web process being up: it is the
/// answer an orchestrator routes traffic on, so a server whose daemon is gone
/// must not claim it.
#[tokio::test]
async fn ready_reports_ready_when_the_daemon_answers() {
    let (app, _daemon) = health_app().await;

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

    let json = body_json(response).await;
    assert_eq!(json["status"], "ready");
}

#[tokio::test]
async fn ready_reports_unavailable_when_the_daemon_refuses() {
    // A daemon that answers the ping with an error is not ready to serve.
    let errors: MockErrors = [(RpcMethod::Ping, (-32603, "ping failed".to_string()))].into();
    let (mock, client) = start_mock_daemon_with_errors(errors).await;
    let app = Router::new().merge(health_routes(build_state(client)));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let json = body_json(response).await;
    assert_eq!(json["status"], "not_ready");
    // The route is public, so the refusal names no socket path and no daemon
    // error text.
    assert_eq!(json["reason"], "daemon unreachable");
    drop(mock);
}

#[tokio::test]
async fn health_nonexistent_route_returns_404() {
    let (app, _daemon) = health_app().await;

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
