//! String-valued knobs: the system prompt, plus the two enum knobs that
//! round-trip their *string spelling*.
//!
//! **No web-side allowlist of valid strategy or validation names.** The daemon
//! parses them and answers `INVALID_PARAMS` on anything it does not recognise
//! (`server/session/params.rs`), which `daemon_err` maps to 422. A second list
//! here would be a second place to update every time the enum grows, and the
//! silent-failure mode is the web accepting a name the daemon rejects — or worse,
//! rejecting one it accepts.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::super::session::OkResponse;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ContextStrategyResponse {
    /// The strategy's string spelling, or `null` where the session carries no
    /// choice of its own.
    pub(super) context_strategy: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SetContextStrategyRequest {
    context_strategy: String,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/config/context-strategy",
    params(("id" = String, Path, description = "The session to configure")),
    request_body = SetContextStrategyRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 422, description = "The daemon does not know the strategy named"),
        (status = 502, description = "The daemon could not store the value"),
    )
)]
pub(crate) async fn set_context_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetContextStrategyRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_context_strategy(&id, &req.context_strategy)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/config/context-strategy",
    params(("id" = String, Path, description = "The session to read")),
    responses(
        (status = 200, body = ContextStrategyResponse),
        (status = 502, description = "The daemon could not read the value"),
    )
)]
pub(crate) async fn get_context_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ContextStrategyResponse>, WebError> {
    let context_strategy = state
        .daemon
        .session_get_context_strategy(&id)
        .await
        .daemon_err()?;
    Ok(Json(ContextStrategyResponse { context_strategy }))
}
