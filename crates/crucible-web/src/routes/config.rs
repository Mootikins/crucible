use crate::services::daemon::AppState;
use crate::WebError;
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct ConfigResponse {
    kiln_path: String,
}

pub fn config_routes() -> Router<AppState> {
    Router::new().route("/api/config", get(get_config))
}

async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, WebError> {
    let kiln_path = state.config.kiln_path_str().unwrap_or_default();
    Ok(Json(ConfigResponse { kiln_path }))
}
