use super::*;

pub(super) fn internal_error(req_id: Option<RequestId>, err: impl std::fmt::Display) -> Response {
    error!("Internal error: {}", err);
    Response::error(req_id, INTERNAL_ERROR, "Internal server error")
}

/// Log client error details and return a sanitized error message.
pub(super) fn invalid_state_error(
    req_id: Option<RequestId>,
    operation: &str,
    err: impl std::fmt::Display,
) -> Response {
    debug!("Invalid state for {}: {}", operation, err);
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Operation '{}' not allowed in current state", operation),
    )
}

pub(super) fn session_not_found(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Session not found: {}", session_id),
    )
}

pub(super) fn agent_not_configured(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("No agent configured for session: {}", session_id),
    )
}

pub(super) fn concurrent_request(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Request already in progress for session: {}", session_id),
    )
}

pub(super) fn agent_error_to_response(req_id: Option<RequestId>, err: AgentError) -> Response {
    match err {
        AgentError::SessionNotFound(id) => session_not_found(req_id, &id),
        AgentError::NoAgentConfigured(id) => agent_not_configured(req_id, &id),
        AgentError::ConcurrentRequest(id) => concurrent_request(req_id, &id),
        e => internal_error(req_id, e),
    }
}

pub(super) async fn handle_client(
    stream: UnixStream,
    ctx: Arc<ServerContext>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let writer_clone = writer.clone();
    let sub_manager = ctx.subscription_manager.clone();
    let event_cancel = CancellationToken::new();
    let event_cancel_clone = event_cancel.clone();
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = event_cancel_clone.cancelled() => break,
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if sub_manager.is_subscribed(client_id, &event.session_id) {
                                if let Ok(json) = event.to_json_line() {
                                    let mut w = writer_clone.lock().await;
                                    if w.write_all(json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "Event forwarder lagged, dropped {} events for client {}", n, client_id
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle_request(req, client_id, &ctx).await,
            Err(e) => {
                warn!("Parse error: {}", e);
                Response::error(None, PARSE_ERROR, e.to_string())
            }
        };

        let mut output = serde_json::to_string(&response)?;
        output.push('\n');

        let mut w = writer.lock().await;
        w.write_all(output.as_bytes()).await?;
    }

    // Graceful shutdown of event forwarding
    event_cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_task).await;
    ctx.subscription_manager.remove_client(client_id);

    Ok(())
}

pub(super) fn forward_to_recording(sm: &SessionManager, event: &SessionEventMessage) {
    if let Some(tx) = sm.get_recording_sender(&event.session_id) {
        if tx.try_send(event.clone()).is_err() {
            warn!(
                session_id = %event.session_id,
                "Recording channel full or closed, dropping event"
            );
        }
    }
}

pub(super) fn should_persist(event: &SessionEventMessage) -> bool {
    if event.msg_type != "event" {
        return false;
    }

    matches!(
        event.event.as_str(),
        "user_message"
            | "message_complete"
            | "tool_call"
            | "tool_result"
            | "model_switched"
            | "ended"
    )
}

pub(super) async fn persist_event(
    event: &SessionEventMessage,
    sm: &SessionManager,
    storage: &dyn SessionStorage,
) -> Result<()> {
    if !should_persist(event) {
        return Ok(());
    }
    let session = match sm.get_session(&event.session_id) {
        Some(s) => s,
        None => return Ok(()),
    };

    let json = serde_json::to_string(event)?;
    storage
        .append_event(&session, &json)
        .await
        .map_err(|e| anyhow::anyhow!("append_event failed: {}", e))?;

    match event.event.as_str() {
        "user_message" => {
            if let Some(content) = event.data.get("content").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "User", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(User) failed: {}", e))?;
            }
        }
        "message_complete" => {
            if let Some(content) = event.data.get("full_response").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "Assistant", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(Assistant) failed: {}", e))?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) async fn handle_request(
    req: Request,
    client_id: ClientId,
    ctx: &ServerContext,
) -> Response {
    let req_clone = req.clone();
    let resp = ctx.dispatcher.dispatch(client_id, req).await;

    if let Some(ref err) = resp.error {
        if err.code == METHOD_NOT_FOUND && err.message.contains("not yet migrated") {
            return handle_legacy_request(LegacyRequestParams {
                req: req_clone,
                kiln_manager: &ctx.kiln_manager,
                session_manager: &ctx.session_manager,
                agent_manager: &ctx.agent_manager,
                project_manager: &ctx.project_manager,
                lua_sessions: &ctx.lua_sessions,
                event_tx: &ctx.event_tx,
                plugin_loader: &ctx.plugin_loader,
                llm_config: &ctx.llm_config,
                mcp_server_manager: &ctx.mcp_server_manager,
            })
            .await;
        }
    }

    resp
}

/// Parameters for handling legacy RPC requests.
pub struct LegacyRequestParams<'a> {
    req: Request,
    kiln_manager: &'a Arc<KilnManager>,
    session_manager: &'a Arc<SessionManager>,
    agent_manager: &'a Arc<AgentManager>,
    project_manager: &'a Arc<ProjectManager>,
    lua_sessions: &'a Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    event_tx: &'a broadcast::Sender<SessionEventMessage>,
    plugin_loader: &'a Arc<Mutex<Option<DaemonPluginLoader>>>,
    llm_config: &'a Option<LlmConfig>,
    mcp_server_manager: &'a Arc<McpServerManager>,
}

pub(super) async fn handle_legacy_request(params: LegacyRequestParams<'_>) -> Response {
    tracing::debug!("Legacy handler for method={:?}", params.req.method);

    match params.req.method.as_str() {
        "session.configure_agent" => {
            handle_session_configure_agent(params.req, params.agent_manager).await
        }
        "session.send_message" => {
            handle_session_send_message(params.req, params.agent_manager, params.event_tx).await
        }
        "session.cancel" => handle_session_cancel(params.req, params.agent_manager).await,
        "session.interaction_respond" => {
            handle_session_interaction_respond(params.req, params.agent_manager, params.event_tx)
                .await
        }
        "session.switch_model" => {
            handle_session_switch_model(params.req, params.agent_manager, params.event_tx).await
        }
        "session.list_models" => handle_session_list_models(params.req, params.agent_manager).await,
        "session.add_notification" => {
            handle_session_add_notification(params.req, params.agent_manager, params.event_tx).await
        }
        "session.list_notifications" => {
            handle_session_list_notifications(params.req, params.agent_manager).await
        }
        "session.dismiss_notification" => {
            handle_session_dismiss_notification(params.req, params.agent_manager, params.event_tx)
                .await
        }
        "session.test_interaction" => {
            handle_session_test_interaction(params.req, params.event_tx).await
        }
        "session.replay" => {
            handle_session_replay(params.req, params.session_manager, params.event_tx).await
        }
        "plugin.reload" => handle_plugin_reload(params.req, params.plugin_loader).await,
        "plugin.list" => handle_plugin_list(params.req, params.plugin_loader).await,
        "project.register" => handle_project_register(params.req, params.project_manager).await,
        "project.unregister" => handle_project_unregister(params.req, params.project_manager).await,
        "project.list" => handle_project_list(params.req, params.project_manager).await,
        "project.get" => handle_project_get(params.req, params.project_manager).await,
        "storage.verify" => handle_storage_verify(params.req).await,
        "storage.cleanup" => handle_storage_cleanup(params.req).await,
        "storage.backup" => handle_storage_backup(params.req).await,
        "storage.restore" => handle_storage_restore(params.req).await,
        "lua.init_session" => handle_lua_init_session(params.req, params.lua_sessions).await,
        "lua.register_hooks" => handle_lua_register_hooks(params.req, params.lua_sessions).await,
        "lua.execute_hook" => handle_lua_execute_hook(params.req, params.lua_sessions).await,
        "lua.shutdown_session" => {
            handle_lua_shutdown_session(params.req, params.lua_sessions).await
        }
        "lua.discover_plugins" => handle_lua_discover_plugins(params.req).await,
        "lua.plugin_health" => handle_lua_plugin_health(params.req).await,
        "lua.generate_stubs" => handle_lua_generate_stubs(params.req).await,
        "lua.run_plugin_tests" => handle_lua_run_plugin_tests(params.req).await,
        "lua.register_commands" => {
            handle_lua_register_commands(params.req, params.lua_sessions).await
        }
        "mcp.start" => {
            handle_mcp_start(params.req, params.kiln_manager, params.mcp_server_manager).await
        }
        "mcp.stop" => handle_mcp_stop(params.req, params.mcp_server_manager).await,
        "mcp.status" => handle_mcp_status(params.req, params.mcp_server_manager).await,
        "skills.list" => handle_skills_list(params.req).await,
        "skills.get" => handle_skills_get(params.req).await,
        "skills.search" => handle_skills_search(params.req).await,
        "agents.list_profiles" => {
            handle_agents_list_profiles(params.req, params.agent_manager).await
        }
        "agents.resolve_profile" => {
            handle_agents_resolve_profile(params.req, params.agent_manager).await
        }
        _ => {
            tracing::warn!("Unknown RPC method: {:?}", params.req.method);
            Response::error(
                params.req.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", params.req.method),
            )
        }
    }
}
