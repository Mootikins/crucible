//! Execution-loop knobs: iteration cap, per-turn timeout, validation retries.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::super::session::OkResponse;

/// **`timeout_secs`, not `execution_timeout`.**
///
/// Required, not `Option`: the daemon's setter takes a bare `u32`, so there is
/// no "unset" to express. The getter still answers `Option`, because a session
/// that has never been configured has no stored value.
#[derive(Debug, Serialize)]
pub(crate) struct ValidationRetriesResponse {
    validation_retries: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetValidationRetriesRequest {
    validation_retries: u32,
}

pub(crate) async fn set_validation_retries(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetValidationRetriesRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_validation_retries(&id, req.validation_retries)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_validation_retries(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ValidationRetriesResponse>, WebError> {
    let validation_retries = state
        .daemon
        .session_get_validation_retries(&id)
        .await
        .daemon_err()?;
    Ok(Json(ValidationRetriesResponse { validation_retries }))
}
