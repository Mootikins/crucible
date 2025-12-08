//! Health check endpoints

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn health_routes() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "crucible-web"
    }))
}

async fn ready_check() -> Json<Value> {
    // TODO: Check actual readiness (agent connection, etc.)
    Json(json!({
        "status": "ready"
    }))
}
