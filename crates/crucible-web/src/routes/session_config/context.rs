//! Context-window knobs: how much history a turn is allowed to carry.
//!
//! All three are `Option<numeric>`, where `None` means "the daemon's default"
//! rather than zero — so the request structs use `Option` and the daemon, not a
//! web-side default, decides what absent means.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::super::session::OkResponse;

#[derive(Debug, Serialize)]
pub(crate) struct ContextBudgetResponse {
    context_budget: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetContextBudgetRequest {
    context_budget: Option<usize>,
}

pub(crate) async fn set_context_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetContextBudgetRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_context_budget(&id, req.context_budget)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_context_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ContextBudgetResponse>, WebError> {
    let context_budget = state
        .daemon
        .session_get_context_budget(&id)
        .await
        .daemon_err()?;
    Ok(Json(ContextBudgetResponse { context_budget }))
}
