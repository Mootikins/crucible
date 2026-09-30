use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{
    ForkPoint, Scoped, SessionForkReply, SessionListModelsReply,
};
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
            return typed_success(
                req.id,
                SessionListModelsReply {
                    session_id: session_id.clone(),
                    models: Vec::new(),
                },
            );
        }
        Err(e) => return internal_error(req.id, e),
    };

    match am.list_models(session_id, classification).await {
        Ok(models) => typed_success(
            req.id,
            SessionListModelsReply {
                session_id: session_id.clone(),
                models,
            },
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            // Return empty models list if no agent is configured
            typed_success(
                req.id,
                SessionListModelsReply {
                    session_id: session_id.clone(),
                    models: Vec::new(),
                },
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
        Ok(models) => typed_success(req.id, ModelsListReply { models }),
        Err(crate::agent_manager::AgentError::SessionNotFound(_)) => {
            // No session fallback path hit — return empty list
            typed_success(req.id, ModelsListReply { models: Vec::new() })
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available providers without requiring an active session.
///
/// `include_models: false` skips model discovery (which dials endpoints and
/// can hang on a dead provider); the CLI's chat preflight uses it to answer
/// "are there any providers at all?" quickly.
///
/// Cached for [`crate::agent_manager::CATALOG_CACHE_TTL`], keyed by
/// `(kiln_path, include_models)` — a provider dials its endpoint on every
/// call otherwise, and the catalog does not change per request. Every caller
/// of this RPC method shares the cache; it used to live only in the web
/// server.
pub(crate) async fn handle_providers_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<ListProvidersRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln_path.map(PathBuf::from);
    let include_models = params.include_models.unwrap_or(true);
    let cache_key = format!(
        "{}:{include_models}",
        kiln_path
            .as_deref()
            .map_or("", |p| p.to_str().unwrap_or(""))
    );

    if let Some(entry) = am.providers_cache.get(cache_key.as_str()) {
        let (reply, fetched_at) = entry.value();
        if fetched_at.elapsed() < crate::agent_manager::CATALOG_CACHE_TTL {
            return typed_success(req.id, reply.clone());
        }
    }

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    let providers = if include_models {
        am.list_providers(classification).await
    } else {
        am.list_providers_summary(classification).await
    };
    let reply = ProvidersListReply { providers };
    am.providers_cache
        .insert(cache_key, (reply.clone(), std::time::Instant::now()));
    typed_success(req.id, reply)
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
        Ok((child, count)) => typed_success(
            req.id,
            SessionForkReply {
                id: child.id,
                parent_id: params.session_id,
                messages_copied: count,
            },
        ),
        Err(
            crate::agent_manager::AgentError::InvalidConfig(message)
            | crate::agent_manager::AgentError::NotSupported(message),
        ) => Response::error(req.id, INVALID_PARAMS, message),
        Err(error) => internal_error(req.id, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::ProviderInfo;
    use std::time::Instant;

    fn list_providers_request() -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(crate::protocol::RequestId::Number(1)),
            method: "providers.list".to_string(),
            params: serde_json::json!({}),
        }
    }

    fn fabricated_provider() -> ProviderInfo {
        ProviderInfo {
            name: "not-a-real-provider".to_string(),
            provider_type: "ollama".to_string(),
            available: true,
            default_model: None,
            models: vec![],
            endpoint: None,
            reason: None,
            is_local: true,
        }
    }

    /// `providers.list`'s reply is cached, keyed by its params: a call with
    /// no `kiln_path` and the default `include_models` answers straight from
    /// `AgentManager::providers_cache` within the TTL, proved by seeding a
    /// fabricated reply no live discovery could produce.
    #[tokio::test]
    async fn providers_list_answers_from_the_cache_within_the_ttl() {
        let am = crate::test_support::bare_agent_manager();
        let fabricated = ProvidersListReply {
            providers: vec![fabricated_provider()],
        };
        am.providers_cache
            .insert(":true".to_string(), (fabricated.clone(), Instant::now()));

        let resp = handle_providers_list(list_providers_request(), &am).await;
        let reply: ProvidersListReply =
            serde_json::from_value(resp.result.expect("a reply")).unwrap();

        assert_eq!(
            reply.providers.len(),
            fabricated.providers.len(),
            "a fresh cache entry must be served as-is"
        );
        assert_eq!(reply.providers[0].name, "not-a-real-provider");
    }

    /// A stale entry (older than the TTL) is not served: the handler
    /// discovers again and replaces it.
    #[tokio::test]
    async fn providers_list_reprobes_after_the_ttl_expires() {
        let am = crate::test_support::bare_agent_manager();
        let stale = ProvidersListReply {
            providers: vec![fabricated_provider()],
        };
        let expired_at = Instant::now()
            - crate::agent_manager::CATALOG_CACHE_TTL
            - std::time::Duration::from_secs(1);
        am.providers_cache
            .insert(":true".to_string(), (stale, expired_at));

        let resp = handle_providers_list(list_providers_request(), &am).await;
        let reply: ProvidersListReply =
            serde_json::from_value(resp.result.expect("a reply")).unwrap();

        assert!(
            reply
                .providers
                .iter()
                .all(|p| p.name != "not-a-real-provider"),
            "an expired cache entry must not reach the caller: {reply:?}"
        );
    }
}
