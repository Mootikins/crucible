use super::*;

impl BackgroundJobManager {
    pub fn register_subagent_context(&self, session_id: &str, config: SubagentContext) {
        self.subagent_contexts
            .insert(session_id.to_string(), config);
    }

    pub async fn spawn_subagent(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<JobId, BackgroundError> {
        let prepared = self
            .prepare_subagent_execution(session_id, prompt, context)
            .await?;
        let job_id = prepared.info.id.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        let task_handle = {
            let running = self.running.clone();
            let history = self.history.clone();
            let event_tx = self.event_tx.clone();
            let job_id = job_id.clone();
            let session_id = prepared.session_id.clone();
            let max_history = self.max_history;
            let session_link = prepared.session_link.clone();
            let fallback_prompt = prepared.prompt.clone();
            let fallback_context = prepared.context.clone();
            let agent = prepared.agent;
            let prompt = prepared.prompt;
            let context = prepared.context;
            let subagent_writer = prepared.subagent_writer;

            tokio::spawn(async move {
                let result = Self::execute_subagent_with_options(
                    agent,
                    prompt.clone(),
                    context,
                    cancel_rx,
                    subagent_writer,
                    SubagentExecutionOptions {
                        max_turns: DEFAULT_SUBAGENT_MAX_TURNS,
                        max_output_bytes: MAX_SUBAGENT_OUTPUT,
                        timeout: None,
                    },
                )
                .await;

                // Extract original JobInfo and delegation metadata to preserve started_at timestamp
                let (info, job_is_delegation, job_parent_session_id) = running
                    .remove(&job_id)
                    .map(|(_, rt)| (rt.info, rt.is_delegation, rt.parent_session_id))
                    .unwrap_or_else(|| {
                        (
                            JobInfo::new(
                                session_id.clone(),
                                JobKind::Subagent {
                                    prompt: fallback_prompt,
                                    context: fallback_context,
                                },
                            ),
                            false,
                            None,
                        )
                    });

                let job_result = Self::build_subagent_result(info, result);
                Self::emit_subagent_completion_events(
                    &event_tx,
                    &session_id,
                    &job_result.info.id.clone(),
                    &job_result,
                    &session_link,
                    job_is_delegation,
                    job_parent_session_id.as_deref(),
                );
                Self::add_to_history(&history, &session_id, job_result, max_history);

                debug!(job_id = %job_id, "Background subagent completed");
            })
        };

        self.running.insert(
            job_id.clone(),
            RunningJob {
                info: prepared.info,
                is_delegation: prepared.is_delegation,
                parent_session_id: prepared.parent_session_id,
                cancel_tx,
                task_handle,
            },
        );

        Ok(job_id)
    }

    pub async fn spawn_subagent_blocking(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
        config: SubagentBlockingConfig,
        cancel_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<JobResult, BackgroundError> {
        // KNOWN LIMITATION: Blocking delegation does not support streaming responses.
        // The subagent's output is collected entirely before returning to the caller.
        // Streaming delegation is a future enhancement that would require async streaming
        // channels and client-side buffering. For now, blocking mode is synchronous only.
        let prepared = self
            .prepare_subagent_execution(session_id, prompt, context)
            .await?;

        let mut cancel_tx_keepalive = None;
        let cancel_rx = match cancel_rx {
            Some(rx) => rx,
            None => {
                let (cancel_tx, cancel_rx) = oneshot::channel();
                cancel_tx_keepalive = Some(cancel_tx);
                cancel_rx
            }
        };

        let result = Self::execute_subagent_with_options(
            prepared.agent,
            prepared.prompt,
            prepared.context,
            cancel_rx,
            prepared.subagent_writer,
            SubagentExecutionOptions {
                max_turns: DEFAULT_SUBAGENT_MAX_TURNS,
                max_output_bytes: config.result_max_bytes,
                timeout: Some(config.timeout),
            },
        )
        .await;
        drop(cancel_tx_keepalive);

        let job_result = Self::build_subagent_result(prepared.info, result);
        Self::emit_subagent_completion_events(
            &self.event_tx,
            &prepared.session_id,
            &job_result.info.id.clone(),
            &job_result,
            &prepared.session_link,
            prepared.is_delegation,
            prepared.parent_session_id.as_deref(),
        );
        Self::add_to_history(
            &self.history,
            &prepared.session_id,
            job_result.clone(),
            self.max_history,
        );

        Ok(job_result)
    }

    async fn prepare_subagent_execution(
        &self,
        session_id: &str,
        prompt: String,
        context: Option<String>,
    ) -> Result<PreparedSubagentExecution, BackgroundError> {
        let factory = self
            .subagent_factory
            .as_ref()
            .ok_or(BackgroundError::NoSubagentFactory)?;

        let (
            parent_agent_config,
            available_agents,
            workspace,
            parent_session_dir,
            parent_session_id,
            delegator_name,
            default_target_name,
            delegation_depth,
        ) = {
            let ctx = self.subagent_contexts.get(session_id).ok_or_else(|| {
                BackgroundError::SpawnFailed("Subagent context not registered".into())
            })?;
            (
                ctx.agent.clone(),
                ctx.available_agents.clone(),
                ctx.workspace.clone(),
                ctx.parent_session_dir.clone(),
                ctx.parent_session_id.clone(),
                ctx.delegator_agent_name.clone(),
                ctx.target_agent_name.clone(),
                ctx.delegation_depth,
            )
        };
        let mut agent_config = parent_agent_config.clone();

        let requested_target_name = parse_target_agent_name(context.as_deref());
        let effective_target_name = requested_target_name
            .clone()
            .or_else(|| default_target_name.clone());

        if let Some(target_name) = requested_target_name {
            agent_config = target_profile_to_session_agent(&target_name, &available_agents)?;
        }

        let is_delegation = parent_session_id.is_some();
        let child_delegation_depth = delegation_depth.saturating_add(1);
        let child_parent_session_id = parent_session_id.clone();

        self.enforce_delegation_capabilities(
            &parent_agent_config,
            delegator_name.as_deref(),
            effective_target_name.as_deref(),
            child_delegation_depth,
            session_id,
        )?;

        // KNOWN LIMITATION: No nested delegation (depth=1 only).
        // Subagents cannot spawn their own subagents. This is enforced by clearing
        // the delegation_config before passing the agent to the subagent factory.
        // Future versions could support configurable nesting depth with proper
        // authorization checks at each level.
        agent_config.delegation_config = None;

        let kind = JobKind::Subagent {
            prompt: prompt.clone(),
            context: context.clone(),
        };
        let mut info = JobInfo::new(session_id.to_string(), kind);
        let job_id = info.id.clone();

        let (subagent_writer, session_link, child_session_id) = if let Some(ref parent_dir) =
            parent_session_dir
        {
            match SessionWriter::create_subagent(parent_dir).await {
                Ok((mut writer, link)) => {
                    let subagent_session_id = writer.id().as_str().to_string();
                    if let Some(ref parent_id) = child_parent_session_id {
                        let metadata = serde_json::json!({
                            "delegation_metadata": {
                                "parent_session_id": parent_id,
                                "delegation_depth": child_delegation_depth,
                            }
                        })
                        .to_string();
                        if let Err(e) = writer.append(LogEvent::system(metadata)).await {
                            warn!(error = %e, "Failed to write delegation metadata to child session");
                        }
                    }

                    info.session_path = Some(writer.session_dir().to_path_buf());
                    (
                        Some(Arc::new(Mutex::new(writer))),
                        link,
                        Some(subagent_session_id),
                    )
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create subagent session, continuing without persistence");
                    (None, format!("[[subagent:{}]]", job_id), None)
                }
            }
        } else {
            (None, format!("[[subagent:{}]]", job_id), None)
        };

        if is_delegation {
            let child_context_key = child_session_id.unwrap_or_else(|| job_id.clone());
            self.subagent_contexts.insert(
                child_context_key,
                SubagentContext {
                    agent: agent_config.clone(),
                    available_agents: available_agents.clone(),
                    workspace: workspace.clone(),
                    parent_session_id: child_parent_session_id,
                    parent_session_dir: info.session_path.clone(),
                    delegator_agent_name: effective_target_name.clone(),
                    target_agent_name: None,
                    delegation_depth: child_delegation_depth,
                },
            );
        }

        if !emit_event(
            &self.event_tx,
            SessionEventMessage::new(
                session_id,
                events::SUBAGENT_SPAWNED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "prompt": truncate(&prompt, 100),
                }),
            ),
        ) {
            tracing::debug!("Failed to emit SUBAGENT_SPAWNED event (no subscribers)");
        }

        if is_delegation {
            if let Some(ref parent_id) = parent_session_id {
                if !emit_event(
                    &self.event_tx,
                    SessionEventMessage::new(
                        parent_id,
                        events::DELEGATION_SPAWNED,
                        serde_json::json!({
                            "delegation_id": job_id,
                            "prompt": truncate(&prompt, 100),
                            "parent_session_id": parent_id,
                            "target_agent": effective_target_name,
                        }),
                    ),
                ) {
                    tracing::debug!("Failed to emit DELEGATION_SPAWNED event (no subscribers)");
                }
            }
        }

        info!(
            job_id = %job_id,
            session_id = %session_id,
            session_link = %session_link,
            prompt_len = prompt.len(),
            "Spawning background subagent"
        );

        let agent = factory(&agent_config, &workspace)
            .await
            .map_err(BackgroundError::SpawnFailed)?;

        Ok(PreparedSubagentExecution {
            info,
            prompt,
            context,
            session_id: session_id.to_string(),
            session_link,
            agent,
            subagent_writer,
            is_delegation,
            parent_session_id,
        })
    }

    pub(super) fn enforce_delegation_capabilities(
        &self,
        session_agent: &SessionAgent,
        delegator_name: Option<&str>,
        target_name: Option<&str>,
        delegation_depth: u32,
        parent_session_id: &str,
    ) -> Result<(), BackgroundError> {
        if delegation_depth >= 3 {
            return Err(BackgroundError::SpawnFailed(
                "Delegation depth limit exceeded (hard cap at 3)".to_string(),
            ));
        }

        let delegation = session_agent
            .delegation_config
            .as_ref()
            .filter(|cfg| cfg.enabled)
            .ok_or_else(|| {
                BackgroundError::SpawnFailed("Delegation is disabled for this agent".to_string())
            })?;

        let active_delegations = self
            .running
            .iter()
            .filter(|entry| {
                entry.value().is_delegation && entry.value().info.session_id == parent_session_id
            })
            .count();

        if active_delegations >= delegation.max_concurrent_delegations as usize {
            return Err(BackgroundError::SpawnFailed(format!(
                "Maximum concurrent delegations ({}) exceeded",
                delegation.max_concurrent_delegations
            )));
        }

        if let Some(allowed_targets) = &delegation.allowed_targets {
            let target = target_name.ok_or_else(|| {
                BackgroundError::SpawnFailed(
                    "Delegation target could not be determined".to_string(),
                )
            })?;

            if !allowed_targets.iter().any(|allowed| allowed == target) {
                return Err(BackgroundError::SpawnFailed(format!(
                    "Delegation target '{target}' is not allowed"
                )));
            }
        }

        if let (Some(delegator), Some(target)) = (delegator_name, target_name) {
            if delegator == target {
                return Err(BackgroundError::SpawnFailed(
                    "Delegation rejected by self-delegation guard".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn execute_subagent_with_options(
        mut agent: Box<dyn AgentHandle + Send + Sync>,
        prompt: String,
        context: Option<String>,
        mut cancel_rx: oneshot::Receiver<()>,
        session_writer: Option<Arc<Mutex<SessionWriter>>>,
        options: SubagentExecutionOptions,
    ) -> Result<String, SubagentError> {
        let SubagentExecutionOptions {
            max_turns,
            max_output_bytes,
            timeout,
        } = options;
        let execute = async {
            let full_prompt = match context {
                Some(ctx) => format!("{}\n\n{}", ctx, prompt),
                None => prompt.clone(),
            };

            if let Some(ref writer) = session_writer {
                let mut w = writer.lock().await;
                if let Err(e) = w.append(LogEvent::user(&full_prompt)).await {
                    error!(error = %e, "Failed to write user event to subagent session");
                }
            }

            let mut accumulated_output = String::new();
            let mut turns = 0;

            while turns < max_turns {
                turns += 1;
                let input = if turns == 1 {
                    full_prompt.clone()
                } else {
                    "Continue with the task.".to_string()
                };

                let mut stream = agent.send_message_stream(input);
                let mut turn_output = String::new();
                let mut has_tool_calls = false;

                loop {
                    tokio::select! {
                        _ = &mut cancel_rx => {
                            return Err(SubagentError::Cancelled);
                        }
                        chunk = stream.next() => {
                            match chunk {
                                Some(Ok(c)) => {
                                    turn_output.push_str(&c.delta);
                                    if c.tool_calls.is_some() {
                                        has_tool_calls = true;
                                    }
                                    if c.done {
                                        break;
                                    }
                                }
                                Some(Err(e)) => {
                                    return Err(SubagentError::Failed(e.to_string()));
                                }
                                None => break,
                            }
                        }
                    }
                }

                if let Some(ref writer) = session_writer {
                    let mut w = writer.lock().await;
                    if let Err(e) = w.append(LogEvent::assistant(&turn_output)).await {
                        error!(error = %e, "Failed to write assistant event to subagent session");
                    }
                }

                accumulated_output.push_str(&turn_output);
                accumulated_output.push('\n');

                if accumulated_output.len() > max_output_bytes {
                    accumulated_output.truncate(max_output_bytes);
                    accumulated_output.push_str("\n\n[Output truncated due to size limit]");
                    break;
                }

                if !has_tool_calls {
                    break;
                }
            }

            Ok(accumulated_output.trim().to_string())
        };

        let mut output = if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, execute).await {
                Ok(inner) => inner?,
                Err(_) => return Err(SubagentError::Timeout),
            }
        } else {
            execute.await?
        };

        if output.len() > max_output_bytes {
            output = truncate(&output, max_output_bytes);
        }

        Ok(output)
    }

    fn build_subagent_result(
        mut info: JobInfo,
        result: Result<String, SubagentError>,
    ) -> JobResult {
        match result {
            Ok(output) => {
                info.mark_completed();
                JobResult::success(info, output)
            }
            Err(SubagentError::Cancelled) => {
                info.mark_cancelled();
                JobResult::failure(info, "Subagent cancelled".to_string())
            }
            Err(SubagentError::Failed(msg)) => {
                info.mark_failed();
                JobResult::failure(info, msg)
            }
            Err(SubagentError::Timeout) => {
                info.mark_failed();
                JobResult::failure(info, "Subagent timed out".to_string())
            }
        }
    }

    fn emit_subagent_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        session_id: &str,
        job_id: &JobId,
        result: &JobResult,
        session_link: &str,
        is_delegation: bool,
        parent_session_id: Option<&str>,
    ) {
        let (is_success, output_or_error) = if result.is_success() {
            (true, result.output.as_deref().unwrap_or(""))
        } else {
            (false, result.error.as_deref().unwrap_or("Unknown error"))
        };

        let (event_type, event_data) = if is_success {
            (
                events::SUBAGENT_COMPLETED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "summary": truncate(output_or_error, 500),
                }),
            )
        } else {
            (
                events::SUBAGENT_FAILED,
                serde_json::json!({
                    "job_id": job_id,
                    "session_link": session_link,
                    "error": output_or_error,
                }),
            )
        };

        if !emit_event(
            event_tx,
            SessionEventMessage::new(session_id, event_type, event_data),
        ) {
            warn!(job_id = %job_id, "No subscribers for subagent completion event");
        }

        if let Some(parent_id) = parent_session_id.filter(|_| is_delegation) {
            let (deleg_type, deleg_data) = if is_success {
                (
                    events::DELEGATION_COMPLETED,
                    serde_json::json!({
                        "delegation_id": job_id,
                        "result_summary": truncate(output_or_error, 500),
                        "parent_session_id": parent_id,
                    }),
                )
            } else {
                (
                    events::DELEGATION_FAILED,
                    serde_json::json!({
                        "delegation_id": job_id,
                        "error": output_or_error,
                        "parent_session_id": parent_id,
                    }),
                )
            };

            if !emit_event(
                event_tx,
                SessionEventMessage::new(parent_id, deleg_type, deleg_data),
            ) {
                tracing::debug!("Failed to emit delegation event (no subscribers)");
            }
        }
        Self::emit_background_completed(event_tx, session_id, job_id, result, "subagent");
    }
}
