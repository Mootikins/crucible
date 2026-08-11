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

#[derive(Debug, Serialize)]
pub(crate) struct OutputValidationResponse {
    output_validation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetOutputValidationRequest {
    output_validation: String,
}

pub(crate) async fn set_output_validation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetOutputValidationRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_output_validation(&id, &req.output_validation)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_output_validation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OutputValidationResponse>, WebError> {
    let output_validation = state
        .daemon
        .session_get_output_validation(&id)
        .await
        .daemon_err()?;
    Ok(Json(OutputValidationResponse { output_validation }))
}

#[derive(Debug, Serialize)]
pub(crate) struct SystemPromptResponse {
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetSystemPromptRequest {
    system_prompt: String,
}

pub(crate) async fn set_system_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetSystemPromptRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_system_prompt(&id, &req.system_prompt)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_system_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SystemPromptResponse>, WebError> {
    let system_prompt = state
        .daemon
        .session_get_system_prompt(&id)
        .await
        .daemon_err()?;
    Ok(Json(SystemPromptResponse { system_prompt }))
}
