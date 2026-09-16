use super::super::*;
use crate::rpc_client::SessionIdRequest;
use crate::rpc_helpers::typed_params;

/// The modes a session can be in, and which one it is in now.
///
/// Deliberately serialized as `ModeDescriptor` rather than the ACP
/// `SessionModeState` that backs it: `SessionModeState` is `#[non_exhaustive]`
/// with a `_meta` field and an `Arc<str>` newtype id, so clients cannot
/// construct one without a `serde_json` workaround. `ModeDescriptor` is a
/// plain struct with spare `icon`/`color` fields, which leaves room for
/// `cru.modes.review = { icon = "…" }` without a wire change.
///
/// A session with no agent configured still has modes — they come from the Lua
/// registry, not the agent — so only `SessionNotFound` is an error here.
///
/// Each descriptor carries the **effective** review policy, not the configured
/// one: `min(mode_policy, agent_capability)`. An external ACP agent runs tools
/// in its own process, so a pre-write gate arrives after the write and cannot
/// block it; reporting `pre_write` for such a session would be a mode chip
/// lying about a safety property. A session with no agent yet degrades the
/// same way — `enforceable_by` treats anything but `"internal"` as
/// post-turn — because promising enforcement we cannot yet vouch for is the
/// failure that matters.
pub(crate) async fn handle_session_list_modes(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    // A read, so a session in storage only answers too; see
    // `AgentManager::read_session_with_agent`.
    let (agent_type, persisted) = match am.read_session_with_agent(session_id).await {
        Ok((_, agent)) => (agent.agent_type, agent.mode),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(_) => (String::new(), None),
    };

    let state = am.session_modes_with(session_id, persisted);
    let modes: Vec<crucible_core::types::mode::ModeDescriptor> = state
        .available_modes
        .iter()
        .map(crucible_core::types::mode::ModeDescriptor::from)
        .map(|d| d.degraded_for(&agent_type))
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "current_mode_id": state.current_mode_id.0.as_ref(),
            "modes": modes,
        }),
    )
}

/// Which settings this session can change, and which it cannot.
///
/// A settings panel drew a fixed list of controls, which was wrong for every
/// ACP session: the protocol has no temperature and no token cap, so the web
/// rendered a slider for each that changed nothing an agent would ever read.
/// The daemon now refuses those settings outright, and this is how a client
/// learns which they are before offering them.
///
/// `supported` is the session's answer, not the agent type's: a model switch
/// depends on whether that particular agent advertised a selector at the
/// handshake, so two ACP sessions can answer differently.
pub(crate) async fn handle_session_list_knobs(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    let support = crucible_core::types::SessionKnobSupport {
        knobs: am
            .session_knobs(session_id)
            .into_iter()
            .map(|(knob, supported)| crucible_core::types::KnobDescriptor {
                id: knob.id().to_string(),
                supported,
            })
            .collect(),
    };

    match serde_json::to_value(support) {
        Ok(value) => Response::success(req.id, value),
        Err(e) => Response::error(req.id, -32603, format!("failed to encode knobs: {e}")),
    }
}

/// The settings this session's external agent advertised for itself.
///
/// These are not Crucible's knobs. A different agent advertises different
/// ones — a reasoning-level selector, whatever else it invented — and the
/// daemon does not interpret them beyond dropping the model selector, which
/// already has a control of its own. A client renders what it is given.
///
/// Empty until the first message, because an agent says nothing until the
/// daemon connects to it, and empty for an internal agent always.
pub(crate) async fn handle_session_list_agent_options(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    match serde_json::to_value(am.agent_config_options(session_id)) {
        Ok(options) => Response::success(
            req.id,
            serde_json::json!({ "session_id": session_id, "options": options }),
        ),
        Err(e) => Response::error(req.id, -32603, format!("failed to encode options: {e}")),
    }
}

/// Set one of those settings on the live agent.
pub(crate) async fn handle_session_set_agent_option(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    #[derive(serde::Deserialize)]
    struct SetAgentOptionRequest {
        session_id: String,
        option_id: String,
        value: String,
    }

    let params = match typed_params::<SetAgentOptionRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match am
        .set_agent_config_option(&params.session_id, &params.option_id, &params.value)
        .await
    {
        Ok(()) => Response::success(req.id, serde_json::json!({ "ok": true })),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => Response::error(req.id, -32602, e.to_string()),
    }
}

#[cfg(test)]
mod stored_session_tests {
    //! A session the daemon holds in storage only — after a restart, or an
    //! eviction — answers a read the same as a live one. A client used to
    //! get "Session not found" here until something read the history, so a
    //! restored pane sat on the three built-in modes for the whole session
    //! and its model chip listed nothing. Reading history is not a
    //! precondition for reading a session.
    use super::*;
    use crate::session_manager::SessionManager;
    use crate::session_storage::FileSessionStorage;
    use crucible_core::session::{Session, SessionType};
    use std::sync::Arc;

    fn request(method: &str, session_id: &str) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": { "session_id": session_id },
        }))
        .unwrap()
    }

    /// A session on disk, and a manager that has never seen it in memory.
    pub(super) async fn stored_only(tmp: &tempfile::TempDir) -> (String, Arc<SessionManager>) {
        let storage = Arc::new(FileSessionStorage::new(FileSessionStorage::root_for(
            tmp.path(),
        )));
        let writer = SessionManager::with_storage(storage.clone());
        let mut session = Session::new(SessionType::Chat, vec![]);
        session.agent = Some(crate::test_fixtures::test_session_agent());
        writer.update_session(&session).await.unwrap();
        let reader = Arc::new(SessionManager::with_storage(storage));
        assert!(reader.get_session(session.id.as_ref()).is_none());
        (session.id.to_string(), reader)
    }

    #[tokio::test]
    async fn list_modes_answers_for_a_session_held_in_storage_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = stored_only(&tmp).await;
        let (event_tx, _) = tokio::sync::broadcast::channel(8);
        let am = crate::test_fixtures::test_agent_manager(
            Arc::new(crate::kiln_manager::KilnManager::new()),
            sm,
            event_tx,
            None,
        );
        let resp = handle_session_list_modes(request("session.list_modes", &id), &am).await;
        assert!(resp.error.is_none(), "{resp:?}");
        let result = resp.result.unwrap();
        assert!(!result["modes"].as_array().unwrap().is_empty());
        assert!(result["current_mode_id"].is_string());
    }

    #[tokio::test]
    async fn list_models_answers_for_a_session_held_in_storage_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = stored_only(&tmp).await;
        let (event_tx, _) = tokio::sync::broadcast::channel(8);
        let am = crate::test_fixtures::test_agent_manager(
            Arc::new(crate::kiln_manager::KilnManager::new()),
            sm,
            event_tx,
            None,
        );
        let resp = super::super::models::handle_session_list_models(
            request("session.list_models", &id),
            &am,
        )
        .await;
        assert!(resp.error.is_none(), "{resp:?}");
        assert!(resp.result.unwrap()["models"].is_array());
    }

    #[tokio::test]
    async fn get_answers_for_a_session_held_in_storage_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = stored_only(&tmp).await;
        let resp = super::super::list::handle_session_get(request("session.get", &id), &sm).await;
        assert!(resp.error.is_none(), "{resp:?}");
        assert_eq!(resp.result.unwrap()["session_id"], id);
    }
}
