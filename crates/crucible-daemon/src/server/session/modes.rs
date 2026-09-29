use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::Scoped;

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
/// Each descriptor carries the **effective** write mode, not the configured
/// one. The Lua declaration of the mode gives the value. An external ACP
/// agent runs its tools in its own process, so the daemon cannot hold its
/// writes, and the session reads `apply`. A session with no agent yet also
/// reads `apply`, because `WriteMode::effective_for` treats every agent type
/// except `"internal"` so.
pub(crate) async fn handle_session_list_modes(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    // A read, so a session in storage only answers too; see
    // `AgentManager::read_session_with_agent`.
    let agent_type = match am.read_session_with_agent(session_id).await {
        Ok((_, agent)) => agent.agent_type,
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(_) => String::new(),
    };

    // The read brings the agent up (the handshake is the resume), so a
    // resumed session's dropdown answers the agent's own modes instead of
    // the Lua stand-in.
    let state = am.live_session_modes(session_id, Some(event_tx)).await;
    let modes: Vec<crucible_core::types::mode::ModeDescriptor> = state
        .available_modes
        .iter()
        .map(crucible_core::types::mode::ModeDescriptor::from)
        // The ACP mode has no `writes` field, so the Lua declaration gives
        // it. The degrade below then sets `apply` for an agent that runs its
        // own tools.
        .map(|mut d| {
            d.writes = am.mode_writes(&d.id);
            d
        })
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

/// The session's command catalog: built-in, mode, plugin, skill and agent
/// commands, in that order. See `agent_manager::commands`.
pub(crate) async fn handle_session_commands(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    match am
        .session_commands(&params.session_id, Some(event_tx))
        .await
    {
        Ok(commands) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": params.session_id,
                "commands": commands,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
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
pub(crate) async fn handle_session_list_knobs(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    // The model knob's answer depends on a selector only the handshake
    // knows — bring the agent up for the read.
    let support = crucible_core::types::SessionKnobSupport {
        knobs: am
            .live_session_knobs(session_id, Some(event_tx))
            .await
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
/// Empty for an internal agent always: Crucible defines its settings rather
/// than discovering them.
pub(crate) async fn handle_session_list_agent_options(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    if let Err(crate::agent_manager::AgentError::SessionNotFound(id)) =
        am.get_session_with_agent(session_id)
    {
        return session_not_found(req.id, &id);
    }

    match serde_json::to_value(
        am.live_agent_config_options(session_id, Some(event_tx))
            .await,
    ) {
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
    event_tx: &crate::EventBus,
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
        .set_agent_config_option(
            &params.session_id,
            &params.option_id,
            &params.value,
            Some(event_tx),
        )
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
        let mut session = Session::new(SessionType::Chat, vec![]);
        session.agent = Some(crate::test_fixtures::test_session_agent());
        storage.save(&session).await.unwrap();
        let reader = Arc::new(SessionManager::with_storage(storage));
        assert!(reader.get_session(session.id.as_ref()).is_none());
        (session.id.to_string(), reader)
    }

    #[tokio::test]
    async fn list_modes_answers_for_a_session_held_in_storage_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = stored_only(&tmp).await;
        let (event_tx, _) = crate::EventBus::channel(8);
        let am = crate::test_fixtures::test_agent_manager(
            Arc::new(crate::kiln_manager::KilnManager::new()),
            sm,
            event_tx.clone(),
            None,
        );
        let resp =
            handle_session_list_modes(request("session.list_modes", &id), &am, &event_tx).await;
        assert!(resp.error.is_none(), "{resp:?}");
        let result = resp.result.unwrap();
        assert!(!result["modes"].as_array().unwrap().is_empty());
        assert!(result["current_mode_id"].is_string());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn list_models_answers_for_a_session_held_in_storage_only() {
        use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

        // Both the injected provider table and the cleared environment are
        // load-bearing. `AgentManager::list_models` enumerates the configured
        // providers AND the ones the process environment holds a credential
        // for; with neither, it falls back to `get_session_with_agent`, which
        // reads the live map only and answers "Session not found" for the
        // session this test seeded in storage. A developer's `GLM_AUTH_TOKEN`
        // supplied the ambient provider that hid it.
        let _env_lock = crate::agent_manager::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env_guards = crate::agent_manager::clear_provider_env();

        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = stored_only(&tmp).await;
        let (event_tx, _) = crate::EventBus::channel(8);
        // Static models, so the offer is answered without dialling anything.
        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers: std::collections::BTreeMap::from([(
                "ollama".to_string(),
                LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    available_models: Some(vec!["llama3.2".to_string()]),
                    ..Default::default()
                },
            )]),
            models: Default::default(),
        };
        let am = crate::test_fixtures::test_agent_manager(
            Arc::new(crate::kiln_manager::KilnManager::new()),
            sm,
            event_tx,
            Some(llm_config),
        );
        let resp = super::super::models::handle_session_list_models(
            request("session.list_models", &id),
            &am,
        )
        .await;
        assert!(resp.error.is_none(), "{resp:?}");
        assert_eq!(
            resp.result.unwrap()["models"],
            serde_json::json!(["ollama/llama3.2"]),
            "the stored session's models are answered from the injected table"
        );
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

#[cfg(test)]
mod writes_tests {
    //! `writes` follows the mode of the session: a mode change and a resume
    //! both reach the descriptor of the current mode. The mode is already a
    //! session knob, so `writes` needs no knob of its own.
    use super::*;
    use crate::session_manager::SessionManager;
    use crate::session_storage::FileSessionStorage;
    use crucible_core::session::{Session, SessionType};
    use crucible_core::types::WriteMode;

    fn storage(tmp: &tempfile::TempDir) -> Arc<FileSessionStorage> {
        Arc::new(FileSessionStorage::new(FileSessionStorage::root_for(
            tmp.path(),
        )))
    }

    /// A session with an internal agent, live in a manager that writes it
    /// to storage.
    async fn live(tmp: &tempfile::TempDir) -> (String, Arc<SessionManager>) {
        let sm = Arc::new(SessionManager::with_storage(storage(tmp)));
        let mut session = Session::new(SessionType::Chat, vec![]);
        session.agent = Some(crate::test_fixtures::test_session_agent());
        sm.storage().save(&session).await.unwrap();
        let id = session.id.to_string();
        sm.register_transient(session);
        (id, sm)
    }

    fn registry() -> crucible_lua::ModeRegistry {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::ModeRegistry::new();
        crucible_lua::register_modes(&lua, registry.clone()).unwrap();
        lua.load(
            r#"cru.modes.ask = { permissions = "ask" }
               cru.modes.propose = { permissions = "allow", writes = "propose" }"#,
        )
        .exec()
        .unwrap();
        registry
    }

    fn manager(sm: Arc<crate::session_manager::SessionManager>) -> Arc<AgentManager> {
        let (event_tx, _) = crate::EventBus::channel(8);
        let am = crate::test_fixtures::test_agent_manager(
            Arc::new(crate::kiln_manager::KilnManager::new()),
            sm,
            event_tx,
            None,
        );
        Arc::new(
            Arc::try_unwrap(am)
                .ok()
                .expect("a new manager has one owner")
                .with_modes(Some(registry())),
        )
    }

    /// The `writes` value of the current mode, as `session.list_modes` answers.
    async fn current_writes(am: &Arc<AgentManager>, id: &str) -> (String, WriteMode) {
        let (event_tx, _) = crate::EventBus::channel(8);
        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list_modes",
            "params": { "session_id": id },
        }))
        .unwrap();
        let resp = handle_session_list_modes(req, am, &event_tx).await;
        let result = resp.result.expect("list_modes answers");
        let current = result["current_mode_id"].as_str().unwrap().to_string();
        let modes: Vec<crucible_core::types::mode::ModeDescriptor> =
            serde_json::from_value(result["modes"].clone()).unwrap();
        let writes = modes
            .iter()
            .find(|m| m.id == current)
            .expect("the current mode is listed")
            .writes;
        (current, writes)
    }

    #[tokio::test]
    async fn writes_follows_a_mode_change() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = live(&tmp).await;
        let am = manager(sm);

        assert_eq!(
            current_writes(&am, &id).await,
            ("ask".to_string(), WriteMode::Apply)
        );
        am.set_mode(&id, "propose", None).await.unwrap();
        assert_eq!(
            current_writes(&am, &id).await,
            ("propose".to_string(), WriteMode::Propose)
        );
        am.set_mode(&id, "ask", None).await.unwrap();
        assert_eq!(
            current_writes(&am, &id).await,
            ("ask".to_string(), WriteMode::Apply)
        );
    }

    #[tokio::test]
    async fn writes_survives_resume() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, sm) = live(&tmp).await;
        manager(sm).set_mode(&id, "propose", None).await.unwrap();

        // A second manager over the same storage, as a restarted daemon that
        // resumes the session.
        let sm = Arc::new(SessionManager::with_storage(storage(&tmp)));
        sm.resume_session_from_storage(&crucible_core::session::SessionId::parse(&id).unwrap())
            .await
            .unwrap();
        let resumed = manager(sm);
        assert_eq!(
            current_writes(&resumed, &id).await,
            ("propose".to_string(), WriteMode::Propose)
        );
    }
}
