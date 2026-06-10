use crate::web::error::WebResultExt;
use crate::web::services::daemon::AppState;
use crate::web::WebError;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;

pub fn skills_routes() -> Router<AppState> {
    Router::new()
        .route("/api/skills", get(list_skills))
        .route("/api/skills/search", get(search_skills))
        .route("/api/skills/{name}", get(get_skill))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    kiln: PathBuf,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetQuery {
    kiln: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    kiln: PathBuf,
    q: String,
    limit: Option<usize>,
}

async fn list_skills(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .skills_list(&query.kiln, query.scope.as_deref())
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn get_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<GetQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .skills_get(&name, &query.kiln)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn search_skills(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .skills_search(&query.q, &query.kiln, query.limit)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_routes_builds() {
        let _router = skills_routes();
    }
}
