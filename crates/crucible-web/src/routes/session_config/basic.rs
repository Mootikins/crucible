//! The five session config knobs the web has always had: thinking budget,
//! temperature, max tokens, precognition, precognition results.
//!
//! Moved here verbatim when `session_config.rs` became a directory — nine more
//! knob pairs would have taken one file past the 1000-line module budget.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use super::super::session::OkResponse;

/// Response for thinking budget config.
#[derive(Debug, Serialize)]
pub(crate) struct ThinkingBudgetResponse {
    thinking_budget: Option<i64>,
}

/// Response for temperature config.
#[derive(Debug, Serialize)]
pub(crate) struct TemperatureResponse {
    temperature: Option<f64>,
}

/// Response for max tokens config.
#[derive(Debug, Serialize)]
pub(crate) struct MaxTokensResponse {
    max_tokens: Option<u32>,
}

/// Response for precognition config.
#[derive(Debug, Serialize)]
pub(crate) struct PrecognitionResponse {
    precognition_enabled: bool,
}

/// Response for precognition results-count config.
#[derive(Debug, Serialize)]
pub(crate) struct PrecognitionResultsResponse {
    precognition_results: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetPrecognitionResultsRequest {
    count: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetThinkingBudgetRequest {
    thinking_budget: Option<i64>,
}

pub(crate) async fn set_thinking_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetThinkingBudgetRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_thinking_budget(&id, req.thinking_budget)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_thinking_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ThinkingBudgetResponse>, WebError> {
    let thinking_budget = state
        .daemon
        .session_get_thinking_budget(&id)
        .await
        .daemon_err()?;
    Ok(Json(ThinkingBudgetResponse { thinking_budget }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetTemperatureRequest {
    temperature: f64,
}

pub(crate) async fn set_temperature(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetTemperatureRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_temperature(&id, req.temperature)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_temperature(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TemperatureResponse>, WebError> {
    let temperature = state
        .daemon
        .session_get_temperature(&id)
        .await
        .daemon_err()?;
    Ok(Json(TemperatureResponse { temperature }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetMaxTokensRequest {
    max_tokens: Option<u32>,
}

pub(crate) async fn set_max_tokens(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetMaxTokensRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_max_tokens(&id, req.max_tokens)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_max_tokens(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MaxTokensResponse>, WebError> {
    let max_tokens = state
        .daemon
        .session_get_max_tokens(&id)
        .await
        .daemon_err()?;
    Ok(Json(MaxTokensResponse { max_tokens }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetPrecognitionRequest {
    enabled: bool,
}

pub(crate) async fn set_precognition(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetPrecognitionRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_precognition(&id, req.enabled)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn set_precognition_results(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetPrecognitionResultsRequest>,
) -> Result<Json<OkResponse>, WebError> {
    // Range guard for the web UX. The daemon accepts any usize today —
    // this is a user-friendly clamp matching the TUI's settings UI, not
    // an authoritative limit. If we ever tighten the daemon-side bounds,
    // mirror them here.
    if !(1..=20).contains(&req.count) {
        return Err(WebError::Validation(format!(
            "precognition results count must be in 1..=20, got {}",
            req.count
        )));
    }
    state
        .daemon
        .session_set_precognition_results(&id, req.count)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

pub(crate) async fn get_precognition_results(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PrecognitionResultsResponse>, WebError> {
    let count = state
        .daemon
        .session_get_precognition_results(&id)
        .await
        .daemon_err()?;
    Ok(Json(PrecognitionResultsResponse {
        precognition_results: count,
    }))
}

pub(crate) async fn get_precognition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PrecognitionResponse>, WebError> {
    let enabled = state
        .daemon
        .session_get_precognition(&id)
        .await
        .daemon_err()?;
    Ok(Json(PrecognitionResponse {
        precognition_enabled: enabled,
    }))
}
