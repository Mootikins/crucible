use super::super::*;
use super::DEFAULT_MAX_TOOL_DEPTH;
use crucible_config::components::permissions::PermissionMode;

impl AgentManager {
    pub async fn send_message(
        &self,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
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
                is_interactive,
                permission_override,
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
            session_dir: session.storage_path(),
            agent_stream_config: AgentStreamConfig::from_session_agent(&agent_config),
            tool_dispatcher: self.get_or_create_session_dispatcher(&session).await,
            permission_override,
        };

        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();
            let stream_config = stream_ctx.agent_stream_config.clone();
            // Use session-configured max_iterations, falling back to the default
            let max_tool_depth = stream_config
                .max_iterations
                .map(|n| n as usize)
                .unwrap_or(DEFAULT_MAX_TOOL_DEPTH);

            let stream_future = Self::execute_agent_stream(
                agent,
                content,
                stream_ctx.clone(),
                stream_config,
                &mut accumulated_response,
                false,
                0,
                max_tool_depth,
            );

            // Wrap in execution timeout if configured
            let timed_future = async {
                if let Some(timeout_secs) = stream_ctx.agent_stream_config.execution_timeout_secs {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(timeout_secs),
                        stream_future,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(_) => {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                timeout_secs = timeout_secs,
                                "Execution timeout reached"
                            );
                            if !emit_event(
                                &stream_ctx.event_tx,
                                SessionEventMessage::ended(
                                    &stream_ctx.session_id,
                                    "error: execution timeout reached",
                                ),
                            ) {
                                warn!(
                                    session_id = %stream_ctx.session_id,
                                    "No subscribers for execution timeout ended event"
                                );
                            }
                        }
                    }
                } else {
                    stream_future.await;
                }
            };

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
                _ = timed_future => {}
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
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<Arc<Mutex<BoxedAgentHandle>>, AgentError> {
        // Check cache first
        if let Some(cached) = self.agent_cache.get(session_id) {
            debug!(session_id = %session_id, "Using cached agent");
            return Ok(cached.clone());
        }

        // Build the agent handle from configuration
        let (agent, resolved_config) = self
            .build_agent_from_config(
                session_id,
                agent_config,
                workspace,
                event_tx,
                is_interactive,
                permission_override,
            )
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
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> Result<(BoxedAgentHandle, SessionAgent), AgentError> {
        let mut resolved_config = if agent_config.endpoint.is_none() {
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

        // Inject tool spilling context into system prompt (once, at agent creation)
        if !resolved_config.system_prompt.is_empty() {
            resolved_config.system_prompt.push_str(
                "\n\nLarge tool outputs are saved to $CRU_SESSION_DIR/tools/. Use this path in shell commands to access full content.",
            );
        }

        info!(
            session_id = %session_id,
            provider = %resolved_config.provider,
            model = %resolved_config.model,
            endpoint = ?resolved_config.endpoint,
            "Creating new agent"
        );

        let agent_permissions = resolved_config.agent_name.as_deref().and_then(|name| {
            self.acp_config.as_ref().and_then(|acp| {
                let available = crate::acp::discovery::default_agent_profiles();
                resolve_agent_profile(name, &acp.agents, &available).and_then(|p| p.permissions)
            })
        });

        let acp_permission_handler = if resolved_config.agent_type == "acp" {
            Some(self.build_acp_permission_handler(
                session_id,
                event_tx,
                is_interactive,
                permission_override,
                agent_permissions,
            ))
        } else {
            None
        };

        let session_for_factory = self.session_manager.get_session(session_id);
        let kiln_path = session_for_factory.as_ref().map(|s| s.kiln.as_path());
        let connected_kilns = session_for_factory
            .as_ref()
            .map(|s| s.connected_kilns.clone())
            .unwrap_or_default();
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

        // Resolve the Lua reference: clone the inner Lua handle (Arc-backed)
        // so we don't need to hold the MutexGuard across the async agent creation.
        let lua_handle: Option<mlua::Lua> = match &self.plugin_loader {
            Some(loader) => {
                let guard = loader.lock().await;
                guard.as_ref().map(|l| l.executor().lua().clone())
            }
            None => None,
        };

        let agent = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            agent_config: &resolved_config,
            lua: lua_handle.as_ref(),
            workspace,
            kiln_path,
            connected_kilns: &connected_kilns,
            parent_session_id: Some(session_id),
            background_spawner: Some(self.background_manager.clone()),
            mcp_gateway: self.mcp_gateway.clone(),
            acp_permission_handler,
            acp_config: self.acp_config.as_ref(),
            knowledge_repo,
            embedding_provider,
        })
        .await?;

        Ok((agent, resolved_config))
    }
}
