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
            return handle_legacy_request(
                req_clone,
                &ctx.kiln_manager,
                &ctx.session_manager,
                &ctx.agent_manager,
                &ctx.project_manager,
                &ctx.lua_sessions,
                &ctx.event_tx,
                &ctx.plugin_loader,
                &ctx.llm_config,
                &ctx.mcp_server_manager,
            )
            .await;
        }
    }

    resp
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_legacy_request(
    req: Request,
    kiln_manager: &Arc<KilnManager>,
    session_manager: &Arc<SessionManager>,
    agent_manager: &Arc<AgentManager>,
    project_manager: &Arc<ProjectManager>,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    llm_config: &Option<LlmConfig>,
    mcp_server_manager: &Arc<McpServerManager>,
) -> Response {
    tracing::debug!("Legacy handler for method={:?}", req.method);

    match req.method.as_str() {
        "kiln.open" => handle_kiln_open(req, kiln_manager, plugin_loader, event_tx).await,
        "kiln.close" => handle_kiln_close(req, kiln_manager).await,
        "kiln.list" => handle_kiln_list(req, kiln_manager).await,
        "kiln.set_classification" => handle_kiln_set_classification(req, kiln_manager).await,
        "search_vectors" => handle_search_vectors(req, kiln_manager).await,
        "list_notes" => handle_list_notes(req, kiln_manager).await,
        "get_note_by_name" => handle_get_note_by_name(req, kiln_manager).await,
        "note.upsert" => handle_note_upsert(req, kiln_manager).await,
        "note.get" => handle_note_get(req, kiln_manager).await,
        "note.delete" => handle_note_delete(req, kiln_manager).await,
        "note.list" => handle_note_list(req, kiln_manager).await,
        "models.list" => handle_models_list(req, agent_manager).await,
        "process_file" => handle_process_file(req, kiln_manager).await,
        "process_batch" => handle_process_batch(req, kiln_manager, event_tx).await,
        "session.create" => {
            handle_session_create(req, session_manager, project_manager, llm_config).await
        }
        "session.list" => handle_session_list(req, session_manager).await,
        "session.get" => handle_session_get(req, session_manager).await,
        "session.pause" => handle_session_pause(req, session_manager).await,
        "session.resume" => handle_session_resume(req, session_manager).await,
        "session.resume_from_storage" => {
            handle_session_resume_from_storage(req, session_manager).await
        }
        "session.end" => handle_session_end(req, session_manager, agent_manager).await,
        "session.compact" => handle_session_compact(req, session_manager).await,
        "session.configure_agent" => handle_session_configure_agent(req, agent_manager).await,
        "session.send_message" => handle_session_send_message(req, agent_manager, event_tx).await,
        "session.cancel" => handle_session_cancel(req, agent_manager).await,
        "session.interaction_respond" => {
            handle_session_interaction_respond(req, agent_manager, event_tx).await
        }
        "session.switch_model" => handle_session_switch_model(req, agent_manager, event_tx).await,
        "session.list_models" => handle_session_list_models(req, agent_manager).await,
        "session.set_thinking_budget" => {
            handle_session_set_thinking_budget(req, agent_manager, event_tx).await
        }
        "session.get_thinking_budget" => {
            handle_session_get_thinking_budget(req, agent_manager).await
        }
        "session.set_precognition" => {
            handle_session_set_precognition(req, agent_manager, event_tx).await
        }
        "session.get_precognition" => handle_session_get_precognition(req, agent_manager).await,
        "session.add_notification" => {
            handle_session_add_notification(req, agent_manager, event_tx).await
        }
        "session.list_notifications" => handle_session_list_notifications(req, agent_manager).await,
        "session.dismiss_notification" => {
            handle_session_dismiss_notification(req, agent_manager, event_tx).await
        }
        "session.set_temperature" => {
            handle_session_set_temperature(req, agent_manager, event_tx).await
        }
        "session.get_temperature" => handle_session_get_temperature(req, agent_manager).await,
        "session.set_max_tokens" => {
            handle_session_set_max_tokens(req, agent_manager, event_tx).await
        }
        "session.get_max_tokens" => handle_session_get_max_tokens(req, agent_manager).await,
        "session.test_interaction" => handle_session_test_interaction(req, event_tx).await,
        "session.replay" => handle_session_replay(req, session_manager, event_tx).await,
        "plugin.reload" => handle_plugin_reload(req, plugin_loader).await,
        "plugin.list" => handle_plugin_list(req, plugin_loader).await,
        "project.register" => handle_project_register(req, project_manager).await,
        "project.unregister" => handle_project_unregister(req, project_manager).await,
        "project.list" => handle_project_list(req, project_manager).await,
        "project.get" => handle_project_get(req, project_manager).await,
        "storage.verify" => handle_storage_verify(req).await,
        "storage.cleanup" => handle_storage_cleanup(req).await,
        "storage.backup" => handle_storage_backup(req).await,
        "storage.restore" => handle_storage_restore(req).await,
        "session.search" => handle_session_search(req, session_manager).await,
        "session.load_events" => handle_session_load_events(req).await,
        "session.list_persisted" => handle_session_list_persisted(req).await,
        "session.render_markdown" => handle_session_render_markdown(req).await,
        "session.export_to_file" => handle_session_export_to_file(req).await,
        "session.cleanup" => handle_session_cleanup(req).await,
        "session.reindex" => handle_session_reindex(req, kiln_manager).await,
        "lua.init_session" => handle_lua_init_session(req, lua_sessions).await,
        "lua.register_hooks" => handle_lua_register_hooks(req, lua_sessions).await,
        "lua.execute_hook" => handle_lua_execute_hook(req, lua_sessions).await,
        "lua.shutdown_session" => handle_lua_shutdown_session(req, lua_sessions).await,
        "lua.discover_plugins" => handle_lua_discover_plugins(req).await,
        "lua.plugin_health" => handle_lua_plugin_health(req).await,
        "lua.generate_stubs" => handle_lua_generate_stubs(req).await,
        "lua.run_plugin_tests" => handle_lua_run_plugin_tests(req).await,
        "lua.register_commands" => handle_lua_register_commands(req, lua_sessions).await,
        "mcp.start" => handle_mcp_start(req, kiln_manager, mcp_server_manager).await,
        "mcp.stop" => handle_mcp_stop(req, mcp_server_manager).await,
        "mcp.status" => handle_mcp_status(req, mcp_server_manager).await,
        "skills.list" => handle_skills_list(req).await,
        "skills.get" => handle_skills_get(req).await,
        "skills.search" => handle_skills_search(req).await,
        "agents.list_profiles" => handle_agents_list_profiles(req, agent_manager).await,
        "agents.resolve_profile" => handle_agents_resolve_profile(req, agent_manager).await,
        _ => {
            tracing::warn!("Unknown RPC method: {:?}", req.method);
            Response::error(
                req.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", req.method),
            )
        }
    }
}
