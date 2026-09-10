//! Session Config Endpoint Contract Tests (with mock daemon)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

#[tokio::test]
async fn set_precognition_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/session/test-session-001/config/precognition")
                .header("content-type", "application/json")
                .body(Body::from(json!({"enabled": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn get_precognition_returns_200_with_enabled_field() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001/config/precognition")
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
    assert!(
        json.get("precognition_enabled").is_some(),
        "Response must contain precognition_enabled field"
    );
}

#[tokio::test]
async fn set_precognition_results_returns_200_with_valid_count() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/session/test-session-001/config/precognition/results")
                .header("content-type", "application/json")
                .body(Body::from(json!({"count": 7}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_precognition_results_rejects_out_of_range() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    for bad_count in [0_u64, 21, 100] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/session/test-session-001/config/precognition/results")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"count": bad_count}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            response.status().is_client_error(),
            "count={bad_count} should 4xx, got {}",
            response.status()
        );
    }
}

#[tokio::test]
async fn get_precognition_results_returns_count() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/session/test-session-001/config/precognition/results")
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
    assert!(
        json.get("precognition_results").is_some(),
        "Response must contain precognition_results field"
    );
}
