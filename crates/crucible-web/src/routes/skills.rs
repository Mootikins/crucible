use crate::error::WebResultExt;
use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_daemon::{SkillDetail, SkillsReply};
use serde::Deserialize;
use std::path::PathBuf;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn skills_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_skills))
        .routes(routes!(search_skills))
        .routes(routes!(get_skill))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListQuery {
    /// The kiln to discover skills from.
    #[param(value_type = String)]
    kiln: PathBuf,
    /// Keep only the skills this discovery scope found.
    scope: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct GetQuery {
    /// The kiln to discover the skill from.
    #[param(value_type = String)]
    kiln: PathBuf,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct SearchQuery {
    /// The kiln to search.
    #[param(value_type = String)]
    kiln: PathBuf,
    /// The query, matched case-insensitively against name and description.
    q: String,
    /// How many rows to answer with. The daemon's own default applies when
    /// the caller names none.
    limit: Option<usize>,
}

/// The skills a kiln discovers, optionally narrowed to one scope.
#[utoipa::path(
    get,
    path = "/api/skills",
    params(ListQuery),
    responses(
        (status = 200, body = SkillsReply),
        (status = 502, description = "The daemon could not discover the skills"),
    )
)]
async fn list_skills(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<SkillsReply>, WebError> {
    let result = state
        .daemon
        .skills_list(&query.kiln, query.scope.as_deref())
        .await
        .daemon_err()?;

    Ok(Json(result))
}

/// One skill, with the Markdown body a summary row omits.
#[utoipa::path(
    get,
    path = "/api/skills/{name}",
    params(("name" = String, Path, description = "The skill's name"), GetQuery),
    responses(
        (status = 200, body = SkillDetail),
        (status = 422, description = "No skill of that name is discoverable from this kiln"),
        (status = 502, description = "The daemon could not read the skill"),
    )
)]
async fn get_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<GetQuery>,
) -> Result<Json<SkillDetail>, WebError> {
    let result = state
        .daemon
        .skills_get(&name, &query.kiln)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

/// The same rows `GET /api/skills` answers, narrowed by a query string.
#[utoipa::path(
    get,
    path = "/api/skills/search",
    params(SearchQuery),
    responses(
        (status = 200, body = SkillsReply),
        (status = 502, description = "The daemon could not search the skills"),
    )
)]
async fn search_skills(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SkillsReply>, WebError> {
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
    use crate::test_support::request_json;

    #[tokio::test]
    async fn list_skills_answers_the_declared_shape() {
        let (status, json) = request_json("GET", "/api/skills?kiln=/kilns/docs", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: SkillsReply =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.skills[0].name, "test-skill");
        assert_eq!(parsed.skills[0].scope, "user");
        assert_eq!(parsed.skills[0].shadowed_count, 0);
    }

    #[tokio::test]
    async fn search_skills_answers_the_declared_shape() {
        let (status, json) =
            request_json("GET", "/api/skills/search?kiln=/kilns/docs&q=match", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: SkillsReply =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.skills[0].name, "matched-skill");
    }

    /// The daemon writes `agent` and `license` whether or not the skill
    /// declares them, so a reader never has to tell absent from null.
    #[tokio::test]
    async fn get_skill_answers_the_declared_shape() {
        let (status, json) =
            request_json("GET", "/api/skills/test-skill?kiln=/kilns/docs", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        assert!(json.get("agent").is_some(), "{json}");
        assert!(json.get("license").is_some(), "{json}");
        let parsed: SkillDetail =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.name, "test-skill");
        assert_eq!(parsed.source_path, "/tmp/skill.md");
        assert_eq!(parsed.agent, None);
        assert_eq!(parsed.license, None);
        assert!(parsed.body.starts_with("# Test Skill"));
    }
}
