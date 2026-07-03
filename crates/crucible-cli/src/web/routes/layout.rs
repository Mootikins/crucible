//! Web UI layout persistence.
//!
//! The SolidJS frontend serializes its pane layout and persists it via this
//! endpoint (`saveLayout`/`loadLayout`/`resetLayout` in `web/src/lib/api.ts`).
//! The blob is opaque to the server — it is stored as-is and handed back.

use crate::web::services::daemon::AppState;
use crate::web::WebError;
use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;

pub fn layout_routes() -> Router<AppState> {
    Router::new().route(
        "/api/layout",
        get(get_layout).post(save_layout).delete(reset_layout),
    )
}

async fn get_layout(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let bytes = match tokio::fs::read(state.layout_path.as_path()).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WebError::NotFound("No saved layout".to_string()));
        }
        Err(e) => return Err(WebError::Internal(format!("Failed to read layout: {e}"))),
    };

    let layout: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| WebError::Internal(format!("Stored layout is not valid JSON: {e}")))?;
    Ok(Json(layout))
}

async fn save_layout(
    State(state): State<AppState>,
    Json(layout): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, WebError> {
    let path = state.layout_path.as_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| WebError::Internal(format!("Failed to create layout dir: {e}")))?;
    }

    let bytes = serde_json::to_vec(&layout)
        .map_err(|e| WebError::Internal(format!("Failed to serialize layout: {e}")))?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(|e| WebError::Internal(format!("Failed to write layout: {e}")))?;
    Ok(Json(json!({"ok": true})))
}

async fn reset_layout(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    match tokio::fs::remove_file(state.layout_path.as_path()).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(WebError::Internal(format!("Failed to delete layout: {e}"))),
    }
    Ok(Json(json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// App with layout routes over a tempdir-backed layout path.
    /// Returns the TempDir so it stays alive for the test's duration.
    async fn layout_test_app() -> (tempfile::TempDir, axum::Router) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_mock, client) = crate::web::test_support::start_mock_daemon().await;
        let mut state = crate::web::test_support::build_mock_state(client);
        state.layout_path = Arc::new(tmp.path().join("web-layout.json"));
        let app = layout_routes().with_state(state);
        (tmp, app)
    }

    fn get_request() -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri("/api/layout")
            .body(Body::empty())
            .unwrap()
    }

    fn post_request(body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/layout")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap()
    }

    fn delete_request() -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri("/api/layout")
            .body(Body::empty())
            .unwrap()
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn get_layout_returns_404_when_none_saved() {
        let (_tmp, app) = layout_test_app().await;
        let response = app.oneshot(get_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn save_then_load_round_trips_the_blob() {
        let (_tmp, app) = layout_test_app().await;
        let layout = json!({
            "version": 1,
            "panes": [{"id": "chat", "size": 0.6}, {"id": "editor", "size": 0.4}]
        });

        let response = app.clone().oneshot(post_request(&layout)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app.oneshot(get_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await, layout);
    }

    #[tokio::test]
    async fn save_overwrites_previous_layout() {
        let (_tmp, app) = layout_test_app().await;
        let first = json!({"version": 1});
        let second = json!({"version": 2});

        app.clone().oneshot(post_request(&first)).await.unwrap();
        app.clone().oneshot(post_request(&second)).await.unwrap();

        let response = app.oneshot(get_request()).await.unwrap();
        assert_eq!(body_json(response).await, second);
    }

    #[tokio::test]
    async fn delete_resets_layout_and_is_idempotent() {
        let (_tmp, app) = layout_test_app().await;
        app.clone()
            .oneshot(post_request(&json!({"version": 1})))
            .await
            .unwrap();

        let response = app.clone().oneshot(delete_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app.clone().oneshot(get_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Deleting again (nothing saved) still succeeds
        let response = app.oneshot(delete_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
