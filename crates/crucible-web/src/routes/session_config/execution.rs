//! Execution-loop knobs: iteration cap, per-turn timeout, validation retries.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::super::session::OkResponse;

#[derive(Debug, Serialize)]
pub(crate) struct MaxIterationsResponse {
    max_iterations: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetMaxIterationsRequest {
    max_iterations: Option<u32>,
}

pub(crate) async fn set_max_iterations(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetMaxIterationsRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_max_iterations(&id, req.max_iterations)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_max_iterations(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MaxIterationsResponse>, WebError> {
    let max_iterations = state
        .daemon
        .session_get_max_iterations(&id)
        .await
        .daemon_err()?;
    Ok(Json(MaxIterationsResponse { max_iterations }))
}

/// **`timeout_secs`, not `execution_timeout`.**
///
/// The RPC method is `session.set_execution_timeout` but the JSON field it reads
/// is `timeout_secs`, and `session.get_execution_timeout` answers under the same
/// key. That asymmetry is recorded on purpose in the daemon's `CONFIG_METHODS`
/// table (gate A1). A struct field named after the knob would compile, pass
/// review, and drop the value — which is why gate A2e checks route EXISTENCE and
/// leaves field names to A1, and why `tests.rs` asserts this response key
/// specifically.
#[derive(Debug, Serialize)]
pub(crate) struct ExecutionTimeoutResponse {
    timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetExecutionTimeoutRequest {
    timeout_secs: Option<u64>,
}

pub(crate) async fn set_execution_timeout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetExecutionTimeoutRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_execution_timeout(&id, req.timeout_secs)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_execution_timeout(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ExecutionTimeoutResponse>, WebError> {
    let timeout_secs = state
        .daemon
        .session_get_execution_timeout(&id)
        .await
        .daemon_err()?;
    Ok(Json(ExecutionTimeoutResponse { timeout_secs }))
}

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
