//! HTTP transport for daemon-owned Obsidian Bases.
//!
//! Every route answers a daemon refusal by its JSON-RPC code: a missing kiln,
//! base, note or view is 404, a request that cannot run is 422, and any other
//! daemon failure is 502. A write that the daemon did not apply answers 409
//! (stale) or 403 (refused), so a client never mistakes it for success.
use crate::{error::WebResultExt, services::daemon::AppState, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
use crucible_daemon::bases::{
    operation::NOT_FOUND, CreateEntryParams, QueryParams, QueryResult, ReorderGroupsParams,
    SetPropertyParams, Source, ViewSummary, ViewsParams, WriteOutcome,
};
use serde::Deserialize;
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
impl TryFrom<BaseQueryParams> for QueryParams {
    type Error = WebError;
    fn try_from(params: BaseQueryParams) -> Result<Self, WebError> {
        let source = match (params.path, params.yaml) {
            (Some(path), None) => Source::Path { path },
            (None, Some(yaml)) => Source::Inline { yaml },
            _ => return Err(WebError::Validation("Specify path or yaml".into())),
        };
        Ok(Self {
            kiln: Some(params.kiln),
            source,
            view: params.view,
            host: params.host,
        })
    }
}

/// A daemon failure as the status its JSON-RPC code names: `NOT_FOUND` is
/// 404; the other codes keep the shared mapping (422 for invalid params,
/// else 502).
fn daemon_result<T>(result: anyhow::Result<T>) -> Result<T, WebError> {
    result.or_else(
        |error| match crate::error::rpc_error_parts(&error.to_string()) {
            (Some(code), message) if code == i64::from(NOT_FOUND) => {
                Err(WebError::NotFound(message))
            }
            _ => Err(error).daemon_err(),
        },
    )
}

/// A write that the daemon did not apply is an HTTP failure: stale is 409
/// with the hash on disk, refused is 403 with the reason.
fn write_answer(outcome: anyhow::Result<WriteOutcome>) -> Result<Json<WriteOutcome>, WebError> {
    match daemon_result(outcome)? {
        WriteOutcome::Stale { current_hash, .. } => Err(WebError::StaleBase { current_hash }),
        WriteOutcome::Refused { reason, .. } => Err(WebError::Forbidden(reason)),
        outcome @ (WriteOutcome::Applied { .. }
        | WriteOutcome::Unchanged { .. }
        | WriteOutcome::Proposed { .. }) => Ok(Json(outcome)),
    }
}

#[utoipa::path(get, path = "/api/bases/query", params(BaseQueryParams), responses(
    (status = 200, body = QueryResult),
    (status = 404, description = "The kiln, the base or the view is absent"),
    (status = 422, description = "The query cannot run as asked"),
    (status = 502, description = "The daemon failed"),
))]
async fn query_base(
    State(state): State<AppState>,
    Query(params): Query<BaseQueryParams>,
) -> Result<Json<QueryResult>, WebError> {
    daemon_result(state.daemon.base_query(params.try_into()?).await).map(Json)
}
#[utoipa::path(get, path = "/api/bases/views", params(BaseQueryParams), responses(
    (status = 200, body = Vec<ViewSummary>),
    (status = 404, description = "The kiln or the base is absent"),
    (status = 422, description = "The request cannot run as asked"),
    (status = 502, description = "The daemon failed"),
))]
async fn base_views(
    State(state): State<AppState>,
    Query(params): Query<BaseQueryParams>,
) -> Result<Json<Vec<ViewSummary>>, WebError> {
    let QueryParams { kiln, source, .. } = params.try_into()?;
    daemon_result(state.daemon.base_views(ViewsParams { kiln, source }).await).map(Json)
}
#[utoipa::path(post, path = "/api/bases/entries", request_body = CreateEntryParams, responses(
    (status = 200, body = WriteOutcome, description = "Applied, unchanged or proposed"),
    (status = 403, description = "A permission rule or a base policy refused the write"),
    (status = 404, description = "The kiln, the base or the view is absent"),
    (status = 409, description = "The file changed since it was read"),
    (status = 422, description = "The entry cannot be created as asked"),
    (status = 502, description = "The daemon failed"),
))]
async fn create_entry(
    State(state): State<AppState>,
    Json(params): Json<CreateEntryParams>,
) -> Result<Json<WriteOutcome>, WebError> {
    write_answer(state.daemon.base_create_entry(params).await)
}
#[utoipa::path(put, path = "/api/bases/property", request_body = SetPropertyParams, responses(
    (status = 200, body = WriteOutcome, description = "Applied, unchanged or proposed"),
    (status = 403, description = "A permission rule or a base policy refused the write"),
    (status = 404, description = "The kiln or the note is absent"),
    (status = 409, description = "The file changed since it was read"),
    (status = 422, description = "The property cannot be written as asked"),
    (status = 502, description = "The daemon failed"),
))]
async fn set_property(
    State(state): State<AppState>,
    Json(params): Json<SetPropertyParams>,
) -> Result<Json<WriteOutcome>, WebError> {
    write_answer(state.daemon.base_set_property(params).await)
}
#[utoipa::path(put, path = "/api/bases/group-order", request_body = ReorderGroupsParams, responses(
    (status = 200, body = WriteOutcome, description = "Applied, unchanged or proposed"),
    (status = 403, description = "A permission rule or a base policy refused the write"),
    (status = 404, description = "The kiln, the base or the view is absent"),
    (status = 409, description = "The file changed since it was read"),
    (status = 422, description = "The group order cannot be saved as asked"),
    (status = 502, description = "The daemon failed"),
))]
async fn reorder_groups(
    State(state): State<AppState>,
    Json(params): Json<ReorderGroupsParams>,
) -> Result<Json<WriteOutcome>, WebError> {
    write_answer(state.daemon.base_reorder_groups(params).await)
}
