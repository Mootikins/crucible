//! HTTP transport for daemon-owned Obsidian Bases.
use crate::{error::WebResultExt, services::daemon::AppState, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn bases_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(query_base))
        .routes(routes!(base_views))
        .routes(routes!(create_entry))
        .routes(routes!(set_property))
        .routes(routes!(reorder_groups))
}
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
struct BaseQueryParams {
    kiln: String,
    path: Option<String>,
    yaml: Option<String>,
    view: Option<String>,
    #[serde(rename = "this")]
    host: Option<String>,
}
impl BaseQueryParams {
    fn params(self) -> Result<Value, WebError> {
        let source = match (self.path, self.yaml) {
            (Some(path), None) => json!({"path":path}),
            (None, Some(yaml)) => json!({"yaml":yaml}),
            _ => return Err(WebError::Validation("Specify path or yaml".into())),
        };
        Ok(json!({"kiln":self.kiln,"source":source,"view":self.view,"this":self.host}))
    }
}
#[utoipa::path(get,path="/api/bases/query",params(BaseQueryParams),responses((status=200,body=crucible_daemon::bases::QueryResult),(status=422,description="Invalid base"),(status=502,description="Daemon unavailable")))]
async fn query_base(
    State(state): State<AppState>,
    Query(params): Query<BaseQueryParams>,
) -> Result<Json<Value>, WebError> {
    Ok(Json(
        state
            .daemon
            .base_query(params.params()?)
            .await
            .daemon_err()?,
    ))
}
#[utoipa::path(get,path="/api/bases/views",params(BaseQueryParams),responses((status=200,body=Value),(status=422,description="Invalid base"),(status=502,description="Daemon unavailable")))]
async fn base_views(
    State(state): State<AppState>,
    Query(params): Query<BaseQueryParams>,
) -> Result<Json<Value>, WebError> {
    Ok(Json(
        state
            .daemon
            .base_views(params.params()?)
            .await
            .daemon_err()?,
    ))
}
#[utoipa::path(post,path="/api/bases/entries",request_body=Value,responses((status=200,body=Value),(status=502,description="Daemon refused entry creation")))]
async fn create_entry(
    State(state): State<AppState>,
    Json(params): Json<Value>,
) -> Result<Json<Value>, WebError> {
    Ok(Json(
        state.daemon.base_create_entry(params).await.daemon_err()?,
    ))
}
#[utoipa::path(put,path="/api/bases/property",request_body=Value,responses((status=200,body=Value),(status=409,description="Stale ancestor"),(status=502,description="Daemon refused write")))]
async fn set_property(
    State(state): State<AppState>,
    Json(params): Json<Value>,
) -> Result<Json<Value>, WebError> {
    let result = state.daemon.base_set_property(params).await.daemon_err()?;
    if result["ok"] == false && result.get("current_hash").is_some() {
        return Err(WebError::StaleBase {
            current_hash: result["current_hash"].as_str().unwrap_or_default().into(),
        });
    }
    if result["ok"] == false {
        return Err(WebError::Validation(format!(
            "Property write refused: {result}"
        )));
    }
    Ok(Json(result))
}

#[utoipa::path(put,path="/api/bases/group-order",request_body=Value,responses((status=200,body=Value),(status=409,description="Stale ancestor"),(status=422,description="Invalid base")))]
async fn reorder_groups(
    State(state): State<AppState>,
    Json(params): Json<Value>,
) -> Result<Json<Value>, WebError> {
    let result = state
        .daemon
        .base_reorder_groups(params)
        .await
        .daemon_err()?;
    if result["ok"] == false {
        return Err(WebError::StaleBase {
            current_hash: result["current_hash"].as_str().unwrap_or_default().into(),
        });
    }
    Ok(Json(result))
}
