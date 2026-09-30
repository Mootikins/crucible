use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{ForkPoint, Scoped};
use crucible_core::protocol::requests::{
    ListAllModelsRequest, ListProvidersRequest, ModelsListReply, ProvidersListReply,
};

// `session.switch_model` is gone: `session.knob.set` writes the model knob
// now, and `handle_session_knob_set` in `params.rs` carries the same apply
// logic (the ACP live-handle path, the provider re-resolve, the persisted
// value for display/resume).

pub(crate) async fn handle_session_list_models(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    // A read, so a session in storage only answers too; see
    // `AgentManager::read_session_with_agent`.
    let classification = match am.read_session_with_agent(session_id).await {
        Ok((session, _)) => {
            // Names → directories, once, through the registry. A name it does
            // not know contributes no classification, which is not the same as
            // contributing `Public`: it is not a kiln at all, so it cannot buy
            // the model list a widening it never earned.
            let kiln_paths = am.session_manager().kiln_paths(&session.kilns);
            // The LIVE-session resolver, not the workspace-only one. A session
            // whose workspace is absent — detached, or never given one — has
            // no config to read, and an absent lookup must not decide that a
            // confidential kiln is Public and widen the offered model list.
            // `handle_models_list` below already walks up from the kiln for
            // exactly this reason; this is the same rule with a session in hand.
            crate::trust_resolution::most_restrictive_classification(&kiln_paths, |kiln| {
                crate::trust_resolution::resolve_session_classification(
                    session.workspace.as_deref(),
                    kiln,
                )
            })
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
    let params = match typed_params::<ListAllModelsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln_path.map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    match am.list_models("", classification).await {
        Ok(models) => reply(req.id, ModelsListReply { models }),
        Err(crate::agent_manager::AgentError::SessionNotFound(_)) => {
            // No session fallback path hit — return empty list
            reply(req.id, ModelsListReply { models: Vec::new() })
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available providers without requiring an active session.
///
/// `include_models: false` skips model discovery (which dials endpoints and
/// can hang on a dead provider); the CLI's chat preflight uses it to answer
/// "are there any providers at all?" quickly.
pub(crate) async fn handle_providers_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<ListProvidersRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln_path.map(PathBuf::from);
    let include_models = params.include_models.unwrap_or(true);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    let providers = if include_models {
        am.list_providers(classification).await
    } else {
        am.list_providers_summary(classification).await
    };
    reply(req.id, ProvidersListReply { providers })
}

/// Answer with `value` as JSON, or report the serialisation failure.
///
/// The reply types here hold only strings, numbers, booleans and vectors of
/// those, so the error arm is unreachable in practice. It exists because an
/// `expect` here would take the daemon down over a reply nobody can act on.
fn reply<T: serde::Serialize>(id: Option<crate::protocol::RequestId>, value: T) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => Response::success(id, value),
        Err(e) => internal_error(id, anyhow::anyhow!(e)),
    }
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
    let params = match typed_params::<Scoped<ForkPoint>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let parent = match sm.read_session(&params.session_id).await {
        Ok(Some(parent)) => parent,
        Ok(None) => return session_not_found(req.id, &params.session_id),
        Err(error) => return internal_error(req.id, error),
    };
    match am.fork_session(parent, params.body.up_to).await {
        Ok((child, count)) => Response::success(
            req.id,
            serde_json::json!({
                "id": child.id,
                "parent_id": params.session_id,
                "messages_copied": count,
            }),
        ),
        Err(
            crate::agent_manager::AgentError::InvalidConfig(message)
            | crate::agent_manager::AgentError::NotSupported(message),
        ) => Response::error(req.id, INVALID_PARAMS, message),
        Err(error) => internal_error(req.id, error),
    }
}
