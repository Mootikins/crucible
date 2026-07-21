use super::super::*;

use super::create::validate_trust_level;
use crate::agent_manager::AgentError;
use crate::trust_resolution::{find_workspace_and_resolve_classification, resolve_provider_trust};
use crucible_core::Session;

fn scope_response(
    req_id: Option<crucible_core::protocol::RequestId>,
    session: &Session,
) -> Response {
    Response::success(
        req_id,
        serde_json::json!({
            "session_id": session.id,
            "kiln": session.kiln,
            "workspace": session.workspace,
            "connected_kilns": session.connected_kilns,
        }),
    )
}

fn scope_error(req_id: Option<crucible_core::protocol::RequestId>, e: AgentError) -> Response {
    match e {
        AgentError::SessionNotFound(_)
        | AgentError::ConcurrentRequest(_)
        | AgentError::InvalidConfig(_)
        | AgentError::NotSupported(_) => Response::error(req_id, INVALID_PARAMS, e.to_string()),
        other => internal_error(req_id, other),
    }
}

/// Attach-side trust gate: the session's provider must satisfy the target's
/// data classification. Detach never needs this — removing scope can't leak.
fn check_attach_trust(
    sm: &Arc<SessionManager>,
    llm_config: &Option<LlmConfig>,
    session_id: &str,
    classification: Option<DataClassification>,
) -> Result<(), String> {
    let Some(classification) = classification else {
        return Ok(());
    };
    let trust = sm
        .get_session(session_id)
        .and_then(|s| s.agent)
        .map(|agent| resolve_provider_trust(&agent, llm_config.as_ref()))
        .unwrap_or(TrustLevel::Cloud);
    validate_trust_level(trust, classification)
}

pub(crate) async fn handle_session_connect_kiln(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    km: &Arc<KilnManager>,
    llm_config: &Option<LlmConfig>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_path = PathBuf::from(require_param!(req, "kiln_path", as_str));

    // Opening validates the path and makes the kiln discoverable.
    if let Err(e) = km.open(&kiln_path).await {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Cannot open kiln '{}': {}", kiln_path.display(), e),
        );
    }

    let classification = find_workspace_and_resolve_classification(&kiln_path);
    if let Err(message) = check_attach_trust(sm, llm_config, &session_id, classification) {
        return Response::error(req.id, INVALID_PARAMS, message);
    }

    match am
        .connect_kiln(&session_id, &kiln_path, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}

pub(crate) async fn handle_session_disconnect_kiln(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_path = PathBuf::from(require_param!(req, "kiln_path", as_str));

    match am
        .disconnect_kiln(&session_id, &kiln_path, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_workspace(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    pm: &Arc<ProjectManager>,
    llm_config: &Option<LlmConfig>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    // Absent/null → detach (workspace falls back to the kiln path).
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    if let Some(ref ws) = workspace {
        if !ws.is_dir() {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Workspace is not a directory: {}", ws.display()),
            );
        }
        if let Err(e) = pm.register_if_missing(ws) {
            tracing::warn!(path = %ws.display(), error = %e, "Failed to auto-register project");
        }
        // The project's config may classify the session's kiln.
        let kiln = sm.get_session(&session_id).map(|s| s.kiln);
        let classification = kiln
            .as_deref()
            .and_then(|k| crate::trust_resolution::resolve_kiln_classification(ws, k));
        if let Err(message) = check_attach_trust(sm, llm_config, &session_id, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

    match am
        .set_workspace(&session_id, workspace, Some(event_tx))
        .await
    {
        Ok(session) => scope_response(req.id, &session),
        Err(e) => scope_error(req.id, e),
    }
}
