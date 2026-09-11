//! The session config knobs the web has always had: precognition and
//! precognition results.
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

/// Response for precognition config.
#[derive(Debug, Serialize)]
pub(crate) struct PrecognitionResponse {
    precognition_enabled: bool,
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

/// The settings this session's external agent advertised for itself.
///
/// These belong to the agent, not to Crucible: a reasoning-level selector, a
/// toggle it invented. The daemon passes them through, so the browser renders
/// whatever this particular agent happens to have.
pub(crate) async fn list_agent_options(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let options = state
        .daemon
        .session_list_agent_options(&id)
        .await
        .daemon_err()?;
    Ok(Json(options))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetAgentOptionRequest {
    pub(crate) option_id: String,
    pub(crate) value: String,
}

pub(crate) async fn set_agent_option(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetAgentOptionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .session_set_agent_option(&id, &body.option_id, &body.value)
        .await
        .daemon_err()?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
