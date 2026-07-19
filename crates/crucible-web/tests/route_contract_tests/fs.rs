//! Contract tests for `/api/fs/*` — thin daemon-proxy routes for the
//! file-tree explorer (listing + DnD move).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

#[tokio::test]
async fn fs_list_returns_200_with_array() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/fs/list?root=/tmp/proj")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_json(response).await.is_array());
}

#[tokio::test]
async fn fs_move_returns_200_with_moved_true() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/fs/move")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "root": "/tmp/proj",
                        "kind": "project",
                        "from_rel": "a.md",
                        "to_rel": "notes/a.md"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["moved"], true);
}

#[tokio::test]
async fn fs_move_rejects_missing_fields() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    // Missing `kind`/`from_rel`/`to_rel` → axum Json rejection, never a move.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/fs/move")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "root": "/tmp/proj" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
