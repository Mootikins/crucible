use super::*;
use crucible_core::events::InternalSessionEvent;

const DEFAULT_MAX_TOOL_DEPTH: usize = 10;

impl AgentManager {
    pub async fn send_message(
        &self,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<String, AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let agent_config = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        use dashmap::mapref::entry::Entry;
        let (cancel_tx, cancel_rx) = oneshot::channel();

        match self.request_state.entry(session_id.to_string()) {
            Entry::Occupied(_) => {
                return Err(AgentError::ConcurrentRequest(session_id.to_string()));
            }
            Entry::Vacant(e) => {
                e.insert(RequestState {
                    cancel_tx: Some(cancel_tx),
                    task_handle: None,
                    started_at: Instant::now(),
                });
            }
        }

        let event_tx_clone = event_tx.clone();
        let agent = match self
            .get_or_create_agent(
                session_id,
                &agent_config,
                &session.workspace,
                &event_tx_clone,
            )
            .await
        {
            Ok(agent) => agent,
            Err(e) => {
                self.request_state.remove(session_id);
                return Err(e);
            }
        };

        let message_id = format!("msg-{}", uuid::Uuid::new_v4());
        let original_content = content;

        if !emit_event(
            event_tx,
            SessionEventMessage::user_message(session_id, &message_id, &original_content),
        ) {
            warn!(session_id = %session_id, "No subscribers for user_message event");
        }

        let content = if agent_config.precognition_enabled
            && !original_content.starts_with("/search")
            && !session.kiln.as_os_str().is_empty()
        {
            self.enrich_with_precognition(
                session_id,
                &original_content,
                &session,
                &agent_config,
                event_tx,
            )
            .await
        } else {
            original_content.clone()
        };

        let session_id_owned = session_id.to_string();
        let request_state = self.request_state.clone();
        let stream_ctx = StreamContext {
            session_id: session_id_owned.clone(),
            message_id: message_id.clone(),
            event_tx: event_tx_clone.clone(),
            session_state: self.get_or_create_session_state(session_id),
            pending_permissions: self.pending_permissions.clone(),
            workspace_path: session.workspace.clone(),
            agent_stream_config: AgentStreamConfig::from_session_agent(&agent_config),
            tool_dispatcher: self.tool_dispatcher.clone(),
        };

        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();
            let stream_config = stream_ctx.agent_stream_config.clone();

            tokio::select! {
                _ = cancel_rx => {
                    debug!(session_id = %session_id_owned, "Request cancelled");
                    if !emit_event(
                        &event_tx_clone,
                        SessionEventMessage::ended(&session_id_owned, "cancelled"),
                    ) {
                        warn!(session_id = %session_id_owned, "No subscribers for cancelled event");
                    }
                }
                _ = Self::execute_agent_stream(
                    agent,
                    content,
                    stream_ctx,
                    stream_config,
                    &mut accumulated_response,
                    false,
                    DEFAULT_MAX_TOOL_DEPTH,
                ) => {}
            }

            request_state.remove(&session_id_owned);
        });

        if let Some(mut state) = self.request_state.get_mut(session_id) {
            state.task_handle = Some(task);
        }

        Ok(message_id)
    }

    async fn get_or_create_agent(
        &self,
        session_id: &str,
        agent_config: &SessionAgent,
        workspace: &std::path::Path,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<Arc<Mutex<BoxedAgentHandle>>, AgentError> {
        // Check cache first
        if let Some(cached) = self.agent_cache.get(session_id) {
            debug!(session_id = %session_id, "Using cached agent");
            return Ok(cached.clone());
        }

        // Build the agent handle from configuration
        let (agent, resolved_config) = self
            .build_agent_from_config(session_id, agent_config, workspace, event_tx)
            .await?;

        // Register delegation/permission handlers if configured
        self.setup_permission_handlers(session_id, &resolved_config);

        // Cache and return
        let agent = Arc::new(Mutex::new(agent));
        self.agent_cache
            .insert(session_id.to_string(), agent.clone());

        Ok(agent)
    }

    /// Create the appropriate agent handle from session configuration.
    ///
    /// Resolves provider endpoint, acquires knowledge repository and embedding
    /// provider from the session's kiln, builds ACP permission handler if needed,
    /// and creates the agent handle via the agent factory.
    async fn build_agent_from_config(
        &self,
        session_id: &str,
        agent_config: &SessionAgent,
        workspace: &std::path::Path,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<(BoxedAgentHandle, SessionAgent), AgentError> {
        let resolved_config = if agent_config.endpoint.is_none() {
            let provider_key = agent_config
                .provider_key
                .as_deref()
                .unwrap_or_else(|| agent_config.provider.as_str());
            if let Some(provider) = self.resolve_provider_config(provider_key) {
                let mut config = agent_config.clone();
                config.endpoint = provider.endpoint;
                debug!(
                    provider_key = %provider_key,
                    endpoint = ?config.endpoint,
                    "Resolved endpoint from llm config"
                );
                config
            } else {
                agent_config.clone()
            }
        } else {
            agent_config.clone()
        };

        info!(
            session_id = %session_id,
            provider = %resolved_config.provider,
            model = %resolved_config.model,
            endpoint = ?resolved_config.endpoint,
            "Creating new agent"
        );

        let acp_permission_handler = if resolved_config.agent_type == "acp" {
            Some(self.build_acp_permission_handler(session_id, event_tx))
        } else {
            None
        };

        let session_for_factory = self.session_manager.get_session(session_id);
        let kiln_path = session_for_factory.as_ref().map(|s| s.kiln.as_path());
        let mut knowledge_repo = None;
        let mut embedding_provider = None;

        if let Some(kiln_path) = kiln_path {
            let storage = self
                .kiln_manager
                .get_or_open(kiln_path)
                .await
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            knowledge_repo = Some(storage.as_knowledge_repository());

            if let Some(config) = self.kiln_manager.enrichment_config().cloned() {
                embedding_provider = Some(
                    crate::embedding::get_or_create_embedding_provider(&config)
                        .await
                        .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?,
                );
            }
        }

        let agent = if let Some(plugin_loader) = &self.plugin_loader {
            let guard = plugin_loader.lock().await;
            let lua = guard.as_ref().map(|loader| loader.executor().lua());
            create_agent_from_session_config(CreateAgentFromSessionConfigParams {
                agent_config: &resolved_config,
                lua,
                workspace,
                kiln_path,
                parent_session_id: Some(session_id),
                background_spawner: Some(self.background_manager.clone()),
                event_tx,
                mcp_gateway: self.mcp_gateway.clone(),
                acp_permission_handler,
                acp_config: self.acp_config.as_ref(),
                knowledge_repo,
                embedding_provider,
            })
            .await?
        } else {
            create_agent_from_session_config(CreateAgentFromSessionConfigParams {
                agent_config: &resolved_config,
                lua: None,
                workspace,
                kiln_path,
                parent_session_id: Some(session_id),
                background_spawner: Some(self.background_manager.clone()),
                event_tx,
                mcp_gateway: self.mcp_gateway.clone(),
                acp_permission_handler,
                acp_config: self.acp_config.as_ref(),
                knowledge_repo,
                embedding_provider,
            })
            .await?
        };

        Ok((agent, resolved_config))
    }

    /// Register delegation context and permission handlers for an agent session.
    ///
    /// If the agent has delegation configuration, registers a subagent context
    /// with the background manager for cross-agent delegation support.
    fn setup_permission_handlers(&self, session_id: &str, resolved_config: &SessionAgent) {
        if resolved_config.delegation_config.is_some() {
            if let Some(session) = self.session_manager.get_session(session_id) {
                let parent_session_id = session
                    .parent_session_id
                    .clone()
                    .or_else(|| Some(session.id.clone()));
                let available_agents = self.build_available_agents();
                self.background_manager.register_subagent_context(
                    session_id,
                    SubagentContext {
                        agent: resolved_config.clone(),
                        available_agents,
                        workspace: session.kiln.clone(),
                        parent_session_id,
                        parent_session_dir: Some(session.storage_path()),
                        delegator_agent_name: resolved_config.agent_name.clone(),
                        target_agent_name: None,
                        delegation_depth: 0,
                    },
                );
            }
        }
    }

    fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> crucible_acp::client::PermissionRequestHandler {
        let pending_permissions = self.pending_permissions.clone();
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();

        let ask_callback: PermissionPromptCallback = Arc::new(move |perm_request: PermRequest| {
            let pending_permissions = pending_permissions.clone();
            let session_id_owned = session_id_owned.clone();
            let event_tx_owned = event_tx_owned.clone();

            Box::pin(async move {
                let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
                let (response_tx, response_rx) = oneshot::channel();

                let pending = PendingPermission {
                    request: perm_request.clone(),
                    response_tx,
                };

                pending_permissions
                    .entry(session_id_owned.clone())
                    .or_default()
                    .insert(permission_id.clone(), pending);

                let interaction_request = InteractionRequest::Permission(perm_request);
                if !emit_event(
                    &event_tx_owned,
                    SessionEventMessage::interaction_requested(
                        &session_id_owned,
                        &permission_id,
                        &interaction_request,
                    ),
                ) {
                    tracing::debug!("Failed to emit interaction_requested event (no subscribers)");
                }

                let result =
                    tokio::time::timeout(std::time::Duration::from_secs(300), response_rx).await;

                match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(_)) => {
                        if let Some(mut session_map) =
                            pending_permissions.get_mut(&session_id_owned)
                        {
                            session_map.remove(&permission_id);
                        }
                        PermResponse::deny_with_reason(
                            "Permission request channel closed before response",
                        )
                    }
                    Err(_) => {
                        if let Some(mut session_map) =
                            pending_permissions.get_mut(&session_id_owned)
                        {
                            session_map.remove(&permission_id);
                        }
                        PermResponse::deny_with_reason("Permission request timed out")
                    }
                }
            })
        });

        let gate: Arc<dyn PermissionGate> = Arc::new(
            DaemonPermissionGate::new(self.permission_config.clone(), true)
                .with_prompt_callback(ask_callback),
        );

        Arc::new(
            move |request: agent_client_protocol::RequestPermissionRequest| {
                let gate = gate.clone();

                Box::pin(async move {
                    use agent_client_protocol::{
                        PermissionOptionKind, RequestPermissionOutcome, SelectedPermissionOutcome,
                    };

                    let tool_name = request
                        .tool_call
                        .fields
                        .title
                        .as_deref()
                        .unwrap_or("acp_tool")
                        .to_string();
                    let args = request
                        .tool_call
                        .fields
                        .raw_input
                        .clone()
                        .unwrap_or(serde_json::Value::Null);

                    let permission = PermRequest::tool(tool_name, args);
                    let response = gate.request_permission(permission).await;

                    let desired_kind = if response.allowed {
                        if response.scope == PermissionScope::Project
                            || response.scope == PermissionScope::User
                            || response.scope == PermissionScope::Session
                            || response.pattern.is_some()
                        {
                            PermissionOptionKind::AllowAlways
                        } else {
                            PermissionOptionKind::AllowOnce
                        }
                    } else {
                        PermissionOptionKind::RejectOnce
                    };

                    request
                        .options
                        .iter()
                        .find(|opt| opt.kind == desired_kind)
                        .map(|opt| {
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                opt.option_id.clone(),
                            ))
                        })
                        .unwrap_or(RequestPermissionOutcome::Cancelled)
                })
            },
        )
    }

    async fn apply_pre_llm_call_handlers(
        content: String,
        stream_ctx: &StreamContext,
        stream_config: &AgentStreamConfig,
    ) -> Option<String> {
        let mut state = stream_ctx.session_state.lock().await;
        let pre_event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: content.clone(),
            model: stream_config.model.clone(),
        });

        match state.reactor.emit(pre_event).await {
            Ok(EmitResult::Completed { event, .. }) => {
                if let SessionEvent::Internal(inner) = event {
                    if let InternalSessionEvent::PreLlmCall { prompt, .. } = inner.as_ref() {
                        Some(prompt.clone())
                    } else {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "PreLlmCall handler returned unexpected event type, using original prompt"
                        );
                        Some(content)
                    }
                } else {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "PreLlmCall handler returned unexpected event type, using original prompt"
                    );
                    Some(content)
                }
            }
            Ok(EmitResult::Cancelled { by_handler, .. }) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    handler = %by_handler,
                    "PreLlmCall cancelled by handler"
                );
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::ended(
                        &stream_ctx.session_id,
                        format!("cancelled by handler: {}", by_handler),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for cancelled event"
                    );
                }
                None
            }
            Ok(EmitResult::Failed { handler, error, .. }) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    handler = %handler,
                    error = %error,
                    "PreLlmCall handler failed, using original prompt (fail-open)"
                );
                Some(content)
            }
            Err(error) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    error = %error,
                    "PreLlmCall emit failed, using original prompt (fail-open)"
                );
                Some(content)
            }
        }
    }

    async fn handle_permission_request(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call_id: &str,
        args: &serde_json::Value,
    ) -> bool {
        let project_path = stream_ctx.workspace_path.to_string_lossy();
        let pattern_store = PatternStore::load_sync(&project_path).unwrap_or_default();
        let pattern_matched = Self::check_pattern_match(&tool_call.name, args, &pattern_store);

        if pattern_matched {
            debug!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "Tool call matches whitelisted pattern, skipping permission prompt"
            );
            return true;
        }

        let hook_result = Self::execute_permission_hooks_with_timeout(
            &stream_ctx.session_state,
            &tool_call.name,
            args,
            &stream_ctx.session_id,
        )
        .await;

        match hook_result {
            PermissionHookResult::Allow => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook allowed tool, skipping permission prompt"
                );
                true
            }
            PermissionHookResult::Deny => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook denied tool"
                );
                let resource_desc = Self::brief_resource_description(args);
                let error_msg = format!(
                    "Lua hook denied permission to {} {}",
                    tool_call.name, resource_desc
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for hook denied tool_result event"
                    );
                }
                false
            }
            PermissionHookResult::Prompt => {
                let perm_request = PermRequest::tool(&tool_call.name, args.clone());
                let interaction_request = InteractionRequest::Permission(perm_request.clone());
                let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
                let (response_tx, response_rx) = oneshot::channel();

                let pending = PendingPermission {
                    request: perm_request,
                    response_tx,
                };

                stream_ctx
                    .pending_permissions
                    .entry(stream_ctx.session_id.to_string())
                    .or_default()
                    .insert(permission_id.clone(), pending);

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    permission_id = %permission_id,
                    "Emitting permission request for destructive tool"
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::interaction_requested(
                        &stream_ctx.session_id,
                        &permission_id,
                        &interaction_request,
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for permission request event"
                    );
                }

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    permission_id = %permission_id,
                    "Waiting for permission response"
                );

                let (permission_granted, deny_reason) = match response_rx.await {
                    Ok(response) => {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            permission_id = %permission_id,
                            allowed = response.allowed,
                            pattern = ?response.pattern,
                            "Permission response received"
                        );

                        if response.allowed {
                            if let Some(ref pattern) = response.pattern {
                                if response.scope == PermissionScope::Project {
                                    if let Err(e) =
                                        Self::store_pattern(&tool_call.name, pattern, &project_path)
                                    {
                                        warn!(
                                            session_id = %stream_ctx.session_id,
                                            tool = %tool_call.name,
                                            pattern = %pattern,
                                            error = %e,
                                            "Failed to store pattern"
                                        );
                                    } else {
                                        info!(
                                            session_id = %stream_ctx.session_id,
                                            tool = %tool_call.name,
                                            pattern = %pattern,
                                            "Pattern stored for future use"
                                        );
                                    }
                                }
                            }
                            (true, None)
                        } else {
                            (false, response.reason)
                        }
                    }
                    Err(_) => {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            permission_id = %permission_id,
                            "Permission channel dropped, treating as deny"
                        );
                        (false, None)
                    }
                };

                if permission_granted {
                    return true;
                }

                let resource_desc = Self::brief_resource_description(args);
                let error_msg = if let Some(reason) = &deny_reason {
                    format!(
                        "User denied permission to {} {}. Feedback: {}",
                        tool_call.name, resource_desc, reason
                    )
                } else {
                    format!(
                        "User denied permission to {} {}",
                        tool_call.name, resource_desc
                    )
                };

                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    error = %error_msg,
                    "Permission denied, emitting error result"
                );

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for permission denied tool_result event"
                    );
                }
                false
            }
        }
    }

    async fn handle_tool_call_in_stream(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
    ) -> Option<crucible_core::traits::chat::ChatToolResult> {
        let call_id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let args = tool_call
            .arguments
            .clone()
            .unwrap_or(serde_json::Value::Null);

        {
            let mut state = stream_ctx.session_state.lock().await;
            let pre_tool_event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
                name: tool_call.name.clone(),
                args: args.clone(),
            });
            match state.reactor.emit(pre_tool_event).await {
                Ok(EmitResult::Cancelled { by_handler, .. }) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        handler = %by_handler,
                        "PreToolCall cancelled by handler"
                    );
                    let error_msg = format!("Tool call denied by handler: {}", by_handler);
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            &call_id,
                            &tool_call.name,
                            serde_json::json!({ "error": error_msg }),
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            "No subscribers for handler denied tool_result event"
                        );
                    }
                    return Some(crucible_core::traits::chat::ChatToolResult {
                        name: tool_call.name.clone(),
                        result: String::new(),
                        error: Some(error_msg),
                        call_id: Some(call_id.clone()),
                    });
                }
                Ok(EmitResult::Failed { handler, error, .. }) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        handler = %handler,
                        error = %error,
                        "PreToolCall handler failed, continuing (fail-open)"
                    );
                }
                Ok(EmitResult::Completed { .. }) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %error,
                        "PreToolCall emit failed, continuing (fail-open)"
                    );
                }
            }
        }

        if !is_safe(&tool_call.name)
            && !Self::handle_permission_request(stream_ctx, tool_call, &call_id, &args).await
        {
            return Some(crucible_core::traits::chat::ChatToolResult {
                name: tool_call.name.clone(),
                result: String::new(),
                error: Some(format!(
                    "Tool call denied by permission gate: {}",
                    tool_call.name
                )),
                call_id: Some(call_id.clone()),
            });
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::tool_call(
                &stream_ctx.session_id,
                &call_id,
                &tool_call.name,
                args.clone(),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "No subscribers for tool_call event"
            );
        }

        let tool_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            stream_ctx.tool_dispatcher.dispatch_tool(&tool_call.name, args.clone()),
        )
        .await;
        let (result_str, error_str) = match tool_result {
            Ok(Ok(val)) => (val.to_string(), None),
            Ok(Err(e)) => (String::new(), Some(e)),
            Err(_elapsed) => (
                String::new(),
                Some(anyhow::anyhow!(
                    "Tool '{}' timed out after 30 seconds",
                    tool_call.name
                )
                .to_string()),
            ),
        };

        let event_result = if let Some(error) = &error_str {
            serde_json::json!({ "error": error })
        } else {
            serde_json::json!({ "result": result_str })
        };

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::tool_result(
                &stream_ctx.session_id,
                &call_id,
                &tool_call.name,
                event_result,
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "No subscribers for tool_result event"
            );
        }

        Some(crucible_core::traits::chat::ChatToolResult {
            name: tool_call.name.clone(),
            result: result_str,
            error: error_str,
            call_id: Some(call_id),
        })
    }

    async fn emit_stream_events(
        stream_ctx: &StreamContext,
        chunk: &crucible_core::traits::chat::ChatChunk,
        accumulated_response: &mut String,
    ) {
        if !chunk.delta.is_empty() {
            if !accumulated_response.is_empty() && chunk.delta == *accumulated_response {
                debug!(
                    session_id = %stream_ctx.session_id,
                    delta_len = chunk.delta.len(),
                    "Skipping duplicate full-text delta (matches accumulated response)"
                );
            } else {
                accumulated_response.push_str(&chunk.delta);
                debug!(
                    session_id = %stream_ctx.session_id,
                    delta_len = chunk.delta.len(),
                    "Sending text_delta event"
                );
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::text_delta(&stream_ctx.session_id, &chunk.delta),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for text_delta event"
                    );
                }
            }
        }

        if let Some(reasoning) = &chunk.reasoning {
            debug!(session_id = %stream_ctx.session_id, "Sending thinking event");
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::thinking(&stream_ctx.session_id, reasoning),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for thinking event"
                );
            }
        }

        if !chunk.done {
            if let Some(tool_calls) = &chunk.tool_calls {
                for tool_call in tool_calls {
                    let _ = Self::handle_tool_call_in_stream(stream_ctx, tool_call).await;
                }
            }
        }

        if let Some(tool_results) = &chunk.tool_results {
            for tool_result in tool_results {
                let call_id = uuid::Uuid::new_v4().to_string();
                let result = if let Some(err) = &tool_result.error {
                    serde_json::json!({ "error": err })
                } else {
                    serde_json::json!({ "result": tool_result.result })
                };

                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        &call_id,
                        &tool_result.name,
                        result,
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_result.name,
                        "No subscribers for tool_result event"
                    );
                }
            }
        }
    }

    #[allow(clippy::ptr_arg)]
    async fn run_reactor_handlers(
        stream_ctx: &StreamContext,
        chunk: &crucible_core::traits::chat::ChatChunk,
        accumulated_response: &mut String,
        is_continuation: bool,
    ) -> Option<(String, String)> {
        debug!(
            session_id = %stream_ctx.session_id,
            message_id = %stream_ctx.message_id,
            response_len = accumulated_response.len(),
            "Sending message_complete event"
        );
        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::message_complete(
                &stream_ctx.session_id,
                &stream_ctx.message_id,
                accumulated_response.clone(),
                chunk.usage.as_ref(),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for message_complete event"
            );
        }

        let injection = Self::dispatch_turn_complete_handlers(
            &stream_ctx.session_id,
            &stream_ctx.message_id,
            accumulated_response,
            &stream_ctx.session_state,
            is_continuation,
        )
        .await;

        if let Some((injected_content, position)) = &injection {
            info!(
                session_id = %stream_ctx.session_id,
                content_len = injected_content.len(),
                position = %position,
                "Processing handler injection"
            );

            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::new(
                    &stream_ctx.session_id,
                    "injection_pending",
                    serde_json::json!({
                        "content": injected_content,
                        "position": position,
                        "is_continuation": true,
                    }),
                ),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for injection_pending event"
                );
            }
        }

        injection
    }

    async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        stream_ctx: StreamContext,
        stream_config: AgentStreamConfig,
        accumulated_response: &mut String,
        is_continuation: bool,
        max_tool_depth: usize,
    ) {
        let Some(content) =
            Self::apply_pre_llm_call_handlers(content, &stream_ctx, &stream_config).await
        else {
            return;
        };

        let stream_start = Instant::now();
        let mut agent_guard = agent.lock().await;
        let mut stream = agent_guard.send_message_stream(content);
        let mut tool_depth = 0usize;

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    Self::emit_stream_events(&stream_ctx, &chunk, accumulated_response).await;

                    if chunk.done {
                        if let Some(tool_calls) = chunk.tool_calls.clone() {
                            if tool_depth < max_tool_depth {
                                tool_depth += 1;
                                let mut tool_results = Vec::new();
                                for tool_call in &tool_calls {
                                    if let Some(result) =
                                        Self::handle_tool_call_in_stream(&stream_ctx, tool_call).await
                                    {
                                        tool_results.push(result);
                                    }
                                }

                                drop(stream);
                                drop(agent_guard);
                                agent_guard = agent.lock().await;
                                stream =
                                    agent_guard.continue_with_tool_results(tool_calls, tool_results);
                                continue;
                            }

                            warn!(
                                session_id = %stream_ctx.session_id,
                                max_tool_depth = max_tool_depth,
                                "max_tool_depth exceeded"
                            );
                            if !emit_event(
                                &stream_ctx.event_tx,
                                SessionEventMessage::ended(
                                    &stream_ctx.session_id,
                                    "error: max_tool_depth exceeded",
                                ),
                            ) {
                                warn!(
                                    session_id = %stream_ctx.session_id,
                                    "No subscribers for max_tool_depth ended event"
                                );
                            }
                            break;
                        }

                        let injection = Self::run_reactor_handlers(
                            &stream_ctx,
                            &chunk,
                            accumulated_response,
                            is_continuation,
                        )
                        .await;

                        if let Some((injected_content, _)) = injection {
                            drop(stream);
                            drop(agent_guard);

                            accumulated_response.clear();
                            let continuation_ctx = StreamContext {
                                session_id: stream_ctx.session_id.clone(),
                                message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                                event_tx: stream_ctx.event_tx.clone(),
                                session_state: stream_ctx.session_state.clone(),
                                pending_permissions: stream_ctx.pending_permissions.clone(),
                                workspace_path: stream_ctx.workspace_path.clone(),
                                agent_stream_config: stream_ctx.agent_stream_config.clone(),
                                tool_dispatcher: stream_ctx.tool_dispatcher.clone(),
                            };

                            Box::pin(Self::execute_agent_stream(
                                agent,
                                injected_content,
                                continuation_ctx,
                                stream_config.clone(),
                                accumulated_response,
                                true,
                                max_tool_depth,
                            ))
                            .await;
                        }

                        break;
                    }
                }
                Err(e) => {
                    error!(session_id = %stream_ctx.session_id, error = %e, "Agent stream error");
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {}", e)),
                    ) {
                        warn!(session_id = %stream_ctx.session_id, "No subscribers for error event");
                    }
                    break;
                }
            }
        }

        let duration_ms = stream_start.elapsed().as_millis() as u64;
        let response_summary: String = accumulated_response.chars().take(200).collect();

        if stream_config.model.is_empty() {
            warn!(
                session_id = %stream_ctx.session_id,
                "PostLlmCall model string is empty, possible upstream issue"
            );
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::new(
                &stream_ctx.session_id,
                "post_llm_call",
                serde_json::json!({
                    "response_summary": &response_summary,
                    "model": &stream_config.model,
                    "duration_ms": duration_ms,
                    "token_count": Option::<u64>::None,
                }),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for post_llm_call event"
            );
        }

        let mut state = stream_ctx.session_state.lock().await;
        let post_event = SessionEvent::internal(InternalSessionEvent::PostLlmCall {
            response_summary,
            model: stream_config.model,
            duration_ms,
            token_count: None,
        });
        if let Err(e) = state.reactor.emit(post_event).await {
            warn!(
                session_id = %stream_ctx.session_id,
                error = %e,
                "PostLlmCall Reactor emit failed (fail-open)"
            );
        }
    }

    pub(super) async fn dispatch_turn_complete_handlers(
        session_id: &str,
        message_id: &str,
        response: &str,
        session_state: &Arc<Mutex<SessionEventState>>,
        is_continuation: bool,
    ) -> Option<(String, String)> {
        use crucible_lua::ScriptHandlerResult;

        let state = session_state.lock().await;
        let handlers = state.registry.runtime_handlers_for("turn:complete");

        if handlers.is_empty() {
            return None;
        }

        debug!(
            session_id = %session_id,
            handler_count = handlers.len(),
            is_continuation = is_continuation,
            "Dispatching turn:complete handlers"
        );

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "response_length": response.len(),
                "is_continuation": is_continuation,
            }),
        };

        let mut pending_injection: Option<(String, String)> = None;

        for handler in handlers {
            match state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
            {
                Ok(result) => {
                    debug!(
                        session_id = %session_id,
                        handler = %handler.name,
                        result = ?result,
                        "Handler executed"
                    );

                    if let ScriptHandlerResult::Inject { content, position } = result {
                        debug!(
                            session_id = %session_id,
                            handler = %handler.name,
                            content_len = content.len(),
                            position = %position,
                            "Handler returned inject"
                        );
                        pending_injection = Some((content, position));
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %session_id,
                        handler = %handler.name,
                        error = %e,
                        "Handler failed"
                    );
                }
            }
        }

        pending_injection
    }

    pub(super) fn brief_resource_description(args: &serde_json::Value) -> String {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            return path.to_string();
        }
        if let Some(file) = args.get("file").and_then(|v| v.as_str()) {
            return file.to_string();
        }
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            let truncated: String = command.chars().take(50).collect();
            if command.len() > 50 {
                return format!("{}...", truncated);
            }
            return truncated;
        }
        if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
            return name.to_string();
        }
        String::new()
    }

    pub(super) fn check_pattern_match(
        tool_name: &str,
        args: &serde_json::Value,
        pattern_store: &PatternStore,
    ) -> bool {
        match tool_name {
            "bash" => {
                if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
                    pattern_store.matches_bash(command)
                } else {
                    false
                }
            }
            "write_file" | "edit_file" | "create_note" | "update_note" | "delete_note" => {
                let path = args
                    .get("path")
                    .or_else(|| args.get("file"))
                    .or_else(|| args.get("name"))
                    .and_then(|v| v.as_str());
                if let Some(path) = path {
                    pattern_store.matches_file(path)
                } else {
                    false
                }
            }
            _ => pattern_store.matches_tool(tool_name),
        }
    }

    pub(super) fn store_pattern(
        tool_name: &str,
        pattern: &str,
        project_path: &str,
    ) -> Result<(), crucible_config::PatternError> {
        let mut store = PatternStore::load_sync(project_path).unwrap_or_default();

        match tool_name {
            "bash" => store.add_bash_pattern(pattern)?,
            "write_file" | "edit_file" | "create_note" | "update_note" | "delete_note" => {
                store.add_file_pattern(pattern)?
            }
            _ => store.add_tool_pattern(pattern)?,
        }

        store.save_sync(project_path)?;
        Ok(())
    }

    async fn execute_permission_hooks_with_timeout(
        session_state: &Arc<Mutex<SessionEventState>>,
        tool_name: &str,
        args: &serde_json::Value,
        session_id: &str,
    ) -> PermissionHookResult {
        let file_path = args
            .get("path")
            .or_else(|| args.get("file"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let request = PermissionRequest {
            tool_name: tool_name.to_string(),
            args: args.clone(),
            file_path,
        };

        let state = session_state.lock().await;
        let hooks_guard = state
            .permission_hooks
            .lock()
            .expect("permission_hooks: poisoned while executing Lua permission hook");
        let functions_guard = state
            .permission_functions
            .lock()
            .expect("permission_functions: poisoned while executing Lua permission hook");

        if hooks_guard.is_empty() {
            return PermissionHookResult::Prompt;
        }

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(1);

        let result = execute_permission_hooks(&state.lua, &hooks_guard, &functions_guard, &request);

        if start.elapsed() > timeout {
            warn!(
                session_id = %session_id,
                tool = %tool_name,
                elapsed_ms = start.elapsed().as_millis(),
                "Permission hook exceeded 1 second timeout"
            );
            return PermissionHookResult::Prompt;
        }

        match result {
            Ok(hook_result) => hook_result,
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    tool = %tool_name,
                    error = %e,
                    "Permission hook execution failed"
                );
                PermissionHookResult::Prompt
            }
        }
    }

    pub async fn cancel(&self, session_id: &str) -> bool {
        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }

            if let Some(handle) = state.task_handle.take() {
                // Give task 500ms to respond to cancellation signal before force-aborting
                match tokio::time::timeout(std::time::Duration::from_millis(500), handle).await {
                    Ok(Ok(())) => debug!(session_id = %session_id, "Task completed gracefully"),
                    Ok(Err(e)) => warn!(session_id = %session_id, error = %e, "Task panicked"),
                    Err(_) => {
                        debug!(session_id = %session_id, "Task did not respond to cancellation, was aborted");
                    }
                }
            }

            info!(session_id = %session_id, "Request cancelled");
            true
        } else {
            warn!(session_id = %session_id, "No active request to cancel");
            false
        }
    }
}
