use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use crucible_core::session::PluginApproval;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

use super::super::session::OkResponse;

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SetPluginApprovalRequest {
    approval: PluginApproval,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct SetPluginTurnLimitRequest {
    limit: u32,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/config/plugin-turn-limit",
    params(("id" = String, Path)),
    request_body = SetPluginTurnLimitRequest,
    responses((status = 200, body = OkResponse))
)]
pub(crate) async fn set_plugin_turn_limit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetPluginTurnLimitRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_plugin_turn_limit(&id, req.limit)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PluginTurnLimitResponse {
    limit: u32,
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/config/plugin-turn-limit",
    params(("id" = String, Path)),
    responses((status = 200, body = PluginTurnLimitResponse))
)]
pub(crate) async fn get_plugin_turn_limit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginTurnLimitResponse>, WebError> {
    let session = state.daemon.session_get(&id).await.daemon_err()?;
    let limit = session.plugin_turn_limit;
    Ok(Json(PluginTurnLimitResponse { limit }))
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PluginApprovalResponse {
    plugin: String,
    approval: PluginApproval,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PluginApprovalsResponse {
    approvals: BTreeMap<String, PluginApproval>,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/config/plugins/{plugin}/approval",
    params(("id" = String, Path), ("plugin" = String, Path)),
    request_body = SetPluginApprovalRequest,
    responses((status = 200, body = OkResponse))
)]
pub(crate) async fn set_plugin_approval(
    State(state): State<AppState>,
    Path((id, plugin)): Path<(String, String)>,
    Json(req): Json<SetPluginApprovalRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_plugin_approval(&id, &plugin, req.approval)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/config/plugins/{plugin}/approval",
    params(("id" = String, Path), ("plugin" = String, Path)),
    responses((status = 200, body = PluginApprovalResponse))
)]
pub(crate) async fn get_plugin_approval(
    State(state): State<AppState>,
    Path((id, plugin)): Path<(String, String)>,
) -> Result<Json<PluginApprovalResponse>, WebError> {
    let approval = state
        .daemon
        .session_get_plugin_approval(&id, &plugin)
        .await
        .daemon_err()?;
    Ok(Json(PluginApprovalResponse { plugin, approval }))
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/config/plugin-approvals",
    params(("id" = String, Path)),
    responses((status = 200, body = PluginApprovalsResponse))
)]
pub(crate) async fn list_plugin_approvals(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginApprovalsResponse>, WebError> {
    let approvals = state
        .daemon
        .session_list_plugin_approvals(&id)
        .await
        .daemon_err()?;
    Ok(Json(PluginApprovalsResponse { approvals }))
}
