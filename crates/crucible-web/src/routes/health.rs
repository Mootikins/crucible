//! Health check endpoints
//!
//! Two different questions, and they must not be answered by the same code:
//! `/health` is liveness — the web process is up and answering. `/ready` is
//! readiness — the daemon this server proxies is reachable. A caller routing
//! traffic on the second one needs the daemon answer, because every accepted
//! request but the static bundle goes there.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::services::daemon::AppState;

pub fn health_routes(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
        .with_state(state)
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "crucible-web"
    }))
}

/// One `ping` round trip, the cheapest call that proves the daemon's socket
/// still answers. The forwarder reconnects and retries once, so a probe that
/// finds a broken connection repairs it rather than only reporting it.
///
/// The refusal carries a fixed reason and no detail: this route sits outside
/// `bearer_auth` by design, and a daemon error names the socket path it could
/// not reach. The error goes to the log instead.
async fn ready_check(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match state.daemon.ping().await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Err(err) => {
            tracing::warn!(error = %err, "readiness probe: daemon unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "not_ready", "reason": "daemon unreachable" })),
            )
        }
    }
}
