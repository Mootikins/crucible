//! Web UI layout persistence.
//!
//! The SolidJS frontend serializes its pane layout and persists it via this
//! endpoint (`saveLayout`/`loadLayout`/`resetLayout` in `web/src/lib/api.ts`).
//! The blob is opaque to the server — it is stored as-is and handed back.

use crate::services::daemon::AppState;
use crate::WebError;
use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn layout_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/layout",
            get(get_layout).post(save_layout).delete(reset_layout),
        )
        .route("/api/recents", get(get_recents).post(record_recent))
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

// =========================================================================
// Recently opened files
// =========================================================================
//
// The splash's "pick up where you left off" list. Persisted server-side —
// localStorage recents are per-origin, so they vanished across ports,
// browsers, and debug instances. Stored next to the layout blob and
// inheriting its standalone isolation: `web-layout.json` →
// `web-layout.recents.json`.

const MAX_RECENTS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentFile {
    abs_path: String,
    name: String,
    /// Unix millis of the last open — set server-side on record.
    opened_at: u64,
}

fn recents_path(state: &AppState) -> std::path::PathBuf {
    state.layout_path.with_extension("recents.json")
}

async fn read_recents(state: &AppState) -> Vec<RecentFile> {
    match tokio::fs::read(recents_path(state)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

async fn get_recents(State(state): State<AppState>) -> Json<serde_json::Value> {
    let recents = read_recents(&state).await;
    Json(json!({ "recents": recents }))
}

#[derive(Debug, Deserialize)]
struct RecordRecentRequest {
    abs_path: String,
    name: String,
}

async fn record_recent(
    State(state): State<AppState>,
    Json(req): Json<RecordRecentRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut recents = read_recents(&state).await;
    recents.retain(|r| r.abs_path != req.abs_path);
    recents.insert(
        0,
        RecentFile {
            abs_path: req.abs_path,
            name: req.name,
            opened_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        },
    );
    recents.truncate(MAX_RECENTS);

    let path = recents_path(&state);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| WebError::Internal(format!("Failed to create recents dir: {e}")))?;
    }
    let bytes = serde_json::to_vec(&recents)
        .map_err(|e| WebError::Internal(format!("Failed to serialize recents: {e}")))?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| WebError::Internal(format!("Failed to write recents: {e}")))?;
    Ok(Json(json!({ "recents": recents })))
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
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let mut state = crate::test_support::build_mock_state(client);
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
    async fn recents_record_dedupes_and_orders_newest_first() {
        let (_tmp, app) = layout_test_app().await;

        // Empty before anything is recorded.
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/recents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["recents"], json!([]));

        let record = |path: &str, name: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/recents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"abs_path": path, "name": name}).to_string(),
                ))
                .unwrap()
        };
        app.clone().oneshot(record("/a.md", "a.md")).await.unwrap();
        app.clone().oneshot(record("/b.md", "b.md")).await.unwrap();
        // Re-opening /a.md moves it back to the front — no duplicate entry.
        app.clone().oneshot(record("/a.md", "a.md")).await.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/recents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let recents = body_json(response).await["recents"].clone();
        let paths: Vec<&str> = recents
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["abs_path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["/a.md", "/b.md"]);
        assert!(recents[0]["opened_at"].as_u64().unwrap() > 0);
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
