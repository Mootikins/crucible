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

use super::super::session::OkResponse;

#[derive(Debug, Serialize)]
pub(crate) struct ContextStrategyResponse {
    context_strategy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetContextStrategyRequest {
    context_strategy: String,
}

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
