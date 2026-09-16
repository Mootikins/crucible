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
use utoipa::ToSchema;

use super::super::session::{daemon_shape, OkResponse};

/// Response for precognition config.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PrecognitionResponse {
    pub(super) precognition_enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SetPrecognitionRequest {
    enabled: bool,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/config/precognition",
    params(("id" = String, Path, description = "The session to configure")),
    request_body = SetPrecognitionRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 422, description = "The session cannot carry the knob"),
        (status = 502, description = "The daemon could not store the value"),
    )
)]
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

#[utoipa::path(
    get,
    path = "/api/session/{id}/config/precognition",
    params(("id" = String, Path, description = "The session to read")),
    responses(
        (status = 200, body = PrecognitionResponse),
        (status = 502, description = "The daemon could not read the value"),
    )
)]
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

/// One choice in an agent's select option.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct AgentOptionChoiceRow {
    /// The value to send back when this choice is picked.
    pub(super) value: String,
    /// What to show for it.
    pub(super) name: String,
}

/// The control an agent option asks for.
///
/// A tagged union rather than a `kind` string beside a loose `current`: a
/// select carries choices and a string, a toggle carries a bool, and the
/// browser's hand-written type declared `current: string | boolean` with
/// optional choices because nothing described the pairing.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum AgentOptionKindRow {
    /// Pick one of several values.
    Select {
        /// The value the agent reports as current.
        current: String,
        /// Every value it accepts, in the order it listed them.
        choices: Vec<AgentOptionChoiceRow>,
    },
    /// On or off.
    Toggle {
        /// The value the agent reports as current.
        current: bool,
    },
}

/// One setting an external agent advertised for itself.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct AgentOptionRow {
    /// The id to name when setting the option.
    pub(super) id: String,
    /// What to label the control.
    pub(super) name: String,
    /// Help text the agent supplied, or `null`. Always written, so
    /// `required` rather than optional.
    #[schema(required = true)]
    pub(super) description: Option<String>,
    /// The agent's own category string, or `null`. Presentation only.
    #[schema(required = true)]
    pub(super) category: Option<String>,
    /// The control to draw, and its current value.
    #[serde(flatten)]
    pub(super) kind: AgentOptionKindRow,
}

/// What `GET /api/session/{id}/config/agent-options` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct AgentOptionsResponse {
    pub(super) session_id: String,
    /// Empty until the first message, and empty for an internal agent always:
    /// an agent advertises nothing until the daemon connects to it.
    pub(super) options: Vec<AgentOptionRow>,
}

/// The settings this session's external agent advertised for itself.
///
/// These belong to the agent, not to Crucible: a reasoning-level selector, a
/// toggle it invented. The daemon passes them through, so the browser renders
/// whatever this particular agent happens to have.
#[utoipa::path(
    get,
    path = "/api/session/{id}/config/agent-options",
    params(("id" = String, Path, description = "The session whose agent is asked")),
    responses(
        (status = 200, body = AgentOptionsResponse),
        (status = 422, description = "The daemon knows no such session"),
        (status = 502, description = "The daemon could not ask the agent"),
    )
)]
pub(crate) async fn list_agent_options(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentOptionsResponse>, WebError> {
    let options = state
        .daemon
        .session_list_agent_options(&id)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(options, "session.list_agent_options")?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SetAgentOptionRequest {
    pub(crate) option_id: String,
    pub(crate) value: String,
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/config/agent-options",
    params(("id" = String, Path, description = "The session whose agent is set")),
    request_body = SetAgentOptionRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 422, description = "The agent knows no such option, or refuses the value"),
        (status = 502, description = "The daemon could not reach the agent"),
    )
)]
pub(crate) async fn set_agent_option(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetAgentOptionRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_agent_option(&id, &body.option_id, &body.value)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}
