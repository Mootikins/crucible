use super::super::*;
use crate::{optional_param, require_param};

pub(crate) async fn handle_session_switch_model(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let model_id = require_param!(req, "model_id", as_str);

    match am.switch_model(session_id, model_id, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "model_id": model_id,
                "switched": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            Response::error(req.id, INVALID_PARAMS, format!("Session not found: {}", id))
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("No agent configured for session: {}", id),
        ),
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "Cannot switch model while request is in progress for session: {}",
                id
            ),
        ),
        Err(crate::agent_manager::AgentError::InvalidModelId(msg)) => {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_list_models(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let classification = match am.get_session_with_agent(session_id) {
        Ok((session, _)) => {
            crate::trust_resolution::resolve_kiln_classification(&session.workspace, &session.kiln)
        }
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            );
        }
        Err(e) => return internal_error(req.id, e),
    };

    match am.list_models(session_id, classification).await {
        Ok(models) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "models": models,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            // Return empty models list if no agent is configured
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available models without requiring an active session.
///
/// Accepts an optional `kiln_path` parameter. When provided, the handler
pub(crate) async fn handle_models_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let kiln_path = req
        .params
        .get("kiln_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    match am.list_models("", classification).await {
        Ok(models) => Response::success(req.id, serde_json::json!({ "models": models })),
        Err(crate::agent_manager::AgentError::SessionNotFound(_)) => {
            // No session fallback path hit — return empty list
            Response::success(req.id, serde_json::json!({ "models": [] }))
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available providers without requiring an active session.
pub(crate) async fn handle_providers_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let kiln_path = req
        .params
        .get("kiln_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    let providers = am.list_providers(classification).await;
    Response::success(req.id, serde_json::json!({ "providers": providers }))
}

/// Fork a session by creating a new session and replaying messages from the parent.
///
/// Params:
///   - `session_id` (string, required): Parent session ID to fork from
///   - `up_to` (u64, optional): Only copy the first N messages (user/assistant/system)
///
/// Returns: `{ id, parent_id, messages_copied }`
pub(crate) async fn handle_session_fork(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let parent_id = require_param!(req, "session_id", as_str);
    let up_to = optional_param!(req, "up_to", as_u64);

    let parent = match sm.get_session(parent_id) {
        Some(s) => s,
        None => return session_not_found(req.id, parent_id),
    };

    let child = match sm
        .create_session(
            parent.session_type,
            parent.kiln.clone(),
            Some(parent.workspace.clone()),
            parent.connected_kilns.clone(),
            None,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => return internal_error(req.id, e),
    };

    let parent_dir = FileSessionStorage::sessions_base(&parent.kiln).join(parent_id);
    let events = match crate::observe::load_events(&parent_dir).await {
        Ok(e) => e,
        Err(e) => {
            warn!(parent_id = %parent_id, error = %e, "Failed to load parent events for fork");
            Vec::new()
        }
    };

    let storage = FileSessionStorage::new();
    let mut count = 0u64;
    for event in &events {
        if let Some(limit) = up_to {
            if count >= limit {
                break;
            }
        }
        match event {
            crate::observe::LogEvent::User { .. }
            | crate::observe::LogEvent::Assistant { .. }
            | crate::observe::LogEvent::System { .. } => match serde_json::to_string(event) {
                Ok(json) => {
                    if let Err(e) = storage.append_event(&child, &json).await {
                        warn!(
                            child_id = %child.id,
                            error = %e,
                            "Failed to write forked event"
                        );
                    }
                    count += 1;
                }
                Err(e) => warn!(error = %e, "Failed to serialize event for fork"),
            },
            _ => {}
        }
    }

    // Copy agent configuration from parent so the forked session inherits
    // model, provider, system prompt, thinking budget, etc.
    if let Ok((_, parent_agent)) = am.get_session_with_agent(parent_id) {
        if let Err(e) = am.configure_agent(&child.id, parent_agent).await {
            warn!(
                child_id = %child.id,
                error = %e,
                "Failed to copy agent config to forked session"
            );
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "id": child.id,
            "parent_id": parent_id,
            "messages_copied": count,
        }),
    )
}
