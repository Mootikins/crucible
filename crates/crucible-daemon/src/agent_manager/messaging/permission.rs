use super::super::*;
use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
use crucible_core::events::InternalSessionEvent;
use std::future::Future;

/// Serializer that ensures only one permission prompt is in-flight at a time
/// per ACP session.
///
/// **Why:** ACP clients (Claude Code, etc.) invoke our permission handler
/// concurrently for parallel tool batches. Without serialization, all N
/// `interaction_requested` events emit at once and pile up in the TUI's
/// queue — the user perceives them as "batched" instead of as "each
/// permission prompt arriving as the corresponding tool finishes".
///
/// Holding the lock across the entire prompt+await window means that
/// caller N+1 waits for caller N's response before its own
/// `interaction_requested` event is emitted, so the TUI sees prompts
/// one-at-a-time even though the ACP client called us in parallel.
///
/// **UX consequence:** if the user walks away from a prompt and it hits
/// the 300 s timeout, queued callers stay blocked for the full timeout
/// before they get a chance to fire. That's the deliberate tradeoff —
/// silent batching was worse — but worth knowing if you're debugging
/// "why did my second prompt take 5 minutes to appear?".
#[derive(Clone, Default)]
pub(super) struct PermissionSerializer {
    inner: Arc<tokio::sync::Mutex<()>>,
}

impl PermissionSerializer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `fut` while holding the serializer lock. Subsequent calls on the
    /// same serializer queue behind this one.
    pub async fn run<F, R>(&self, fut: F) -> R
    where
        F: Future<Output = R>,
    {
        let _guard = self.inner.lock().await;
        fut.await
    }
}

impl AgentManager {
    /// Register delegation context and permission handlers for an agent session.
    ///
    /// If the agent has delegation configuration, registers a subagent context
    /// with the background manager for cross-agent delegation support.
    pub(super) fn setup_permission_handlers(
        &self,
        session_id: &str,
        resolved_config: &SessionAgent,
    ) {
        if resolved_config.delegation_config.is_some() {
            if let Some(session) = self.session_manager.get_session(session_id) {
                let parent_session_id = session
                    .parent_session_id
                    .clone()
                    .or(Some(session.id.clone()));
                let available_agents = self.build_available_agents();
                self.background_manager.register_subagent_context(
                    session_id,
                    SubagentContext {
                        agent: resolved_config.clone(),
                        available_agents,
                        workspace: session.workspace.clone(),
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

    pub(super) fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
        agent_permissions: Option<PermissionConfig>,
        workspace_path: std::path::PathBuf,
    ) -> crate::acp::client::PermissionRequestHandler {
        let pending_permissions = self.pending_permissions.clone();
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();
        // One serializer per handler == per session. Concurrent prompt
        // requests within a session queue behind each other; cross-session
        // prompts proceed independently.
        let serializer = PermissionSerializer::new();

        let ask_callback: PermissionPromptCallback = Arc::new(move |perm_request: PermRequest| {
            let pending_permissions = pending_permissions.clone();
            let session_id_owned = session_id_owned.clone();
            let event_tx_owned = event_tx_owned.clone();
            let serializer = serializer.clone();

            Box::pin(async move {
                serializer
                    .run(async move {
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
                            tracing::debug!(
                                "Failed to emit interaction_requested event (no subscribers)"
                            );
                        }

                        let result =
                            tokio::time::timeout(std::time::Duration::from_secs(300), response_rx)
                                .await;

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
                    .await
            })
        });

        // Priority: CLI override > agent-specific > global config.
        // For Allow and Deny overrides, the user's intent is unconditional —
        // ignore base-config rules entirely. For Ask, preserve rules (interactive default).
        let effective_config = resolve_effective_permission_config(
            permission_override,
            agent_permissions,
            self.permission_config.clone(),
        );

        let gate: Arc<dyn PermissionGate> = Arc::new(
            DaemonPermissionGate::new(effective_config, is_interactive)
                .with_prompt_callback(ask_callback),
        );

        Arc::new(
            move |request: agent_client_protocol::RequestPermissionRequest| {
                let gate = gate.clone();
                let workspace_path = workspace_path.clone();

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

                    let diffs = crate::tools::diff_synth::synthesize_diffs(
                        &tool_name,
                        &args,
                        &workspace_path,
                    );
                    let permission = PermRequest::tool(tool_name, args).with_diffs(diffs);
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

    pub(super) async fn apply_pre_llm_call_handlers(
        content: String,
        stream_ctx: &StreamContext,
        stream_config: &AgentStreamConfig,
    ) -> Option<String> {
        let mut state = stream_ctx.session_state.lock().await;
        let mut current_content = content;
        let pre_event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: current_content.clone(),
            model: stream_config.model.clone(),
        });

        current_content = match state.reactor.emit(pre_event).await {
            Ok(EmitResult::Completed { event, .. }) => match &event {
                SessionEvent::Internal(inner)
                    if matches!(inner.as_ref(), InternalSessionEvent::PreLlmCall { .. }) =>
                {
                    let InternalSessionEvent::PreLlmCall { prompt, .. } = inner.as_ref() else {
                        unreachable!()
                    };
                    prompt.clone()
                }
                _ => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "PreLlmCall handler returned unexpected event type, using original prompt"
                    );
                    current_content
                }
            },
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
                return None;
            }
            Ok(EmitResult::Failed { handler, error, .. }) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    handler = %handler,
                    error = %error,
                    "PreLlmCall handler failed, using original prompt (fail-open)"
                );
                current_content
            }
            Err(error) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    error = %error,
                    "PreLlmCall emit failed, using original prompt (fail-open)"
                );
                current_content
            }
        };

        let handlers = state.registry.runtime_handlers_for("pre_llm_call", None);
        for handler in handlers {
            let event = SessionEvent::Custom {
                name: "pre_llm_call".to_string(),
                payload: serde_json::json!({
                    "prompt": &current_content,
                    "model": &stream_config.model,
                }),
            };
            match state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
                .await
            {
                Ok(crucible_lua::ScriptHandlerResult::Transform(val)) => {
                    if let Some(prompt) = val.get("prompt").and_then(|v| v.as_str()) {
                        current_content = prompt.to_string();
                    }
                }
                Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        reason = %reason,
                        "pre_llm_call handler cancelled"
                    );
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        error = %error,
                        "pre_llm_call handler error (fail-open)"
                    );
                }
            }
        }

        Some(current_content)
    }

    pub(super) async fn handle_permission_request(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call_id: &str,
        args: &serde_json::Value,
    ) -> bool {
        // Honor explicit --permissions override before running any hooks or prompt.
        // `Allow` auto-approves; `Deny` auto-rejects with an error tool_result;
        // `Ask` and `None` fall through to the standard hook/prompt flow.
        match stream_ctx.permission_override {
            Some(PermissionMode::Allow) => {
                tracing::debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "permission override Allow: auto-approving tool call"
                );
                return true;
            }
            Some(PermissionMode::Deny) => {
                let error_msg = "Tool call denied by permission override".to_string();
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result(
                        &stream_ctx.session_id,
                        call_id,
                        &tool_call.name,
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for tool_result (permission override Deny)"
                    );
                }
                return false;
            }
            Some(PermissionMode::Ask) | None => {}
        }

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
                let diffs = crate::tools::diff_synth::synthesize_diffs(
                    &tool_call.name,
                    args,
                    &stream_ctx.workspace_path,
                );
                let perm_request =
                    PermRequest::tool(&tool_call.name, args.clone()).with_diffs(diffs);
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

    pub(in crate::agent_manager) fn brief_resource_description(args: &serde_json::Value) -> String {
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

    pub(in crate::agent_manager) fn check_pattern_match(
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

    pub(in crate::agent_manager) fn store_pattern(
        tool_name: &str,
        pattern: &str,
        project_path: &str,
    ) -> Result<(), crucible_core::config::PatternError> {
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

    pub(super) async fn execute_permission_hooks_with_timeout(
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
}

/// Resolve the permission config to apply for a turn given the CLI override,
/// any agent-specific permissions, and the daemon's global permission config.
///
/// Priority: CLI override > agent-specific > global config.
///
/// For `Allow` and `Deny` overrides the user's intent is unconditional — the
/// returned config has the requested default and *empty* allow/deny/ask rule
/// lists, so base-config rules cannot re-introduce prompts or blocks. For
/// `Ask` the existing allow/deny/ask rules are preserved (interactive default).
#[cfg(test)]
mod permission_serializer_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn serializer_lets_single_caller_through() {
        let s = PermissionSerializer::new();
        let result = s.run(async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn serializer_runs_concurrent_calls_one_at_a_time() {
        // Three concurrent callers must execute strictly serially.
        // Track in-flight count: it must never exceed 1.
        let s = PermissionSerializer::new();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let s = s.clone();
            let in_flight = in_flight.clone();
            let max_seen = max_seen.clone();
            handles.push(tokio::spawn(async move {
                s.run(async {
                    let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    let mut prev = max_seen.load(Ordering::SeqCst);
                    while n > prev {
                        match max_seen.compare_exchange(prev, n, Ordering::SeqCst, Ordering::SeqCst)
                        {
                            Ok(_) => break,
                            Err(actual) => prev = actual,
                        }
                    }
                    // Hold the section briefly so concurrent callers stack up.
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "concurrent callers must execute one at a time, but the in-flight high-water mark was higher"
        );
    }

    #[tokio::test]
    async fn serializer_releases_lock_after_completion() {
        // Once one call completes, the next one must be able to proceed.
        let s = PermissionSerializer::new();
        s.run(async {}).await;
        // Must complete without deadlock.
        let started = std::time::Instant::now();
        s.run(async {}).await;
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn separate_serializers_do_not_block_each_other() {
        // Per-session serialization: two distinct serializers should NOT
        // queue behind each other.
        let a = PermissionSerializer::new();
        let b = PermissionSerializer::new();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let task = |s: PermissionSerializer| {
            let in_flight = in_flight.clone();
            let max_seen = max_seen.clone();
            tokio::spawn(async move {
                s.run(async {
                    let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    let mut prev = max_seen.load(Ordering::SeqCst);
                    while n > prev {
                        match max_seen.compare_exchange(prev, n, Ordering::SeqCst, Ordering::SeqCst)
                        {
                            Ok(_) => break,
                            Err(actual) => prev = actual,
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            })
        };

        let h1 = task(a);
        let h2 = task(b);
        h1.await.unwrap();
        h2.await.unwrap();

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            2,
            "different serializers must not block each other; both should run concurrently"
        );
    }
}

pub(super) fn resolve_effective_permission_config(
    permission_override: Option<PermissionMode>,
    agent_permissions: Option<PermissionConfig>,
    global_permission_config: Option<PermissionConfig>,
) -> Option<PermissionConfig> {
    match permission_override {
        Some(mode @ (PermissionMode::Allow | PermissionMode::Deny)) => Some(PermissionConfig {
            default: mode,
            allow: Vec::new(),
            deny: Vec::new(),
            ask: Vec::new(),
        }),
        Some(PermissionMode::Ask) => {
            let mut config = agent_permissions
                .or(global_permission_config)
                .unwrap_or_default();
            config.default = PermissionMode::Ask;
            Some(config)
        }
        None => agent_permissions.or(global_permission_config),
    }
}
