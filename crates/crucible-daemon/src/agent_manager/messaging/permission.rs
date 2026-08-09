use super::super::*;
use crucible_core::config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::events::InternalSessionEvent;
use std::future::Future;

/// The name the permission engine matches an ACP tool call against.
///
/// **ACP carries no tool name.** `ToolCallUpdateFields` has `title` — prose
/// for a human, `"Read src/main.rs"` — and `kind`, a category. This used to
/// key on `title`, so `is_safe` was never true and no `[permissions]` rule
/// naming a tool could ever match: the whole gate was unreachable on the ACP
/// path, and its unit tests passed because they build `PermRequest::tool`
/// shapes that production never produces.
///
/// `kind` is the only tool identity on the wire, so rules match against it.
/// The names chosen are the engine's own file-operation grammar (`read`,
/// `edit`, `write`, `delete` — see `is_file_tool`) plus `bash`, so an
/// operator writes the same patterns they already write elsewhere.
///
/// A `kind` the agent supplies is **not** grounds for the read-only
/// exemption: see [`crate::agent_manager::is_safe`], which may only widen on
/// something the daemon itself knows. An ACP tool matches rules and is
/// otherwise asked about.
///
/// Schema 1.6.0 adds an `unstable_tool_call_name` field that would replace
/// this outright. Reaching it means upgrading `agent-client-protocol` 0.10 →
/// 2.0 and opting into a field its own docs call removable, so this is the
/// one place to change when that lands.
fn acp_tool_name(fields: &agent_client_protocol::ToolCallUpdateFields) -> String {
    use agent_client_protocol::ToolKind;
    match fields.kind {
        Some(ToolKind::Read) => "read",
        Some(ToolKind::Edit) => "edit",
        Some(ToolKind::Delete) => "delete",
        // No `move` in the engine's grammar, and a move is a write.
        Some(ToolKind::Move) => "write",
        Some(ToolKind::Search) => "search",
        Some(ToolKind::Execute) => "bash",
        Some(ToolKind::Fetch) => "fetch",
        Some(ToolKind::Think) => "think",
        Some(ToolKind::SwitchMode) => "switch_mode",
        // `Other` is ACP's default, so an agent that sets no kind lands here
        // too — as does any variant a later schema adds, since `ToolKind` is
        // `#[non_exhaustive]` and a kind we cannot classify must not be
        // guessed at. Deliberately not a file-operation name: an unidentified
        // call must not inherit a rule written for one that is identified.
        Some(ToolKind::Other) | Some(_) | None => "acp_tool",
    }
    .to_string()
}

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

/// Which ACP option a gate decision corresponds to.
///
/// `AllowAlways` only when the decision is one the user asked to be
/// remembered — a saved pattern, or a scope wider than this single call.
/// Anything else that was allowed is allowed once.
fn outcome_kind(response: &PermResponse) -> agent_client_protocol::PermissionOptionKind {
    use agent_client_protocol::PermissionOptionKind;

    if !response.allowed {
        return PermissionOptionKind::RejectOnce;
    }
    let remembered = response.pattern.is_some()
        || matches!(
            response.scope,
            PermissionScope::Project | PermissionScope::User | PermissionScope::Session
        );
    if remembered {
        PermissionOptionKind::AllowAlways
    } else {
        PermissionOptionKind::AllowOnce
    }
}

impl AgentManager {
    pub(super) fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
        agent_permissions: Option<PermissionConfig>,
        tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    ) -> crate::acp::client::PermissionRequestHandler {
        let pending_permissions = self.pending_permissions.clone();
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();
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
                                tracing::debug!(
                                    permission_id = %permission_id,
                                    session_id = %session_id_owned,
                                    "permission channel closed before response"
                                );
                                PermResponse::deny_with_reason(
                                    "Permission request channel closed before response".to_string(),
                                )
                            }
                            Err(_) => {
                                if let Some(mut session_map) =
                                    pending_permissions.get_mut(&session_id_owned)
                                {
                                    session_map.remove(&permission_id);
                                }
                                tracing::debug!(
                                    permission_id = %permission_id,
                                    session_id = %session_id_owned,
                                    "permission request timed out"
                                );
                                PermResponse::deny_with_reason(
                                    "Permission request timed out".to_string(),
                                )
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

        let tool_policy = tool_policy.map(Arc::new);

        Arc::new(
            move |request: agent_client_protocol::RequestPermissionRequest| {
                let gate = gate.clone();
                let tool_policy = tool_policy.clone();

                Box::pin(async move {
                    use agent_client_protocol::{
                        PermissionOptionKind, RequestPermissionOutcome, SelectedPermissionOutcome,
                    };
                    use crucible_core::agent::ToolPolicy;

                    let tool_name = acp_tool_name(&request.tool_call.fields);
                    let declared = tool_policy
                        .as_ref()
                        .and_then(|m| m.get(&tool_name))
                        .copied();

                    // A declared policy answers before the gate, exactly as it
                    // does on the internal path (`requires_permission_gate`
                    // returns false for Allow, and `tool_call.rs` refuses Deny
                    // above the hook loop). Without this the two agent types
                    // disagreed about the same session's `tool_policy`: an
                    // internal agent honoured it and an ACP agent fell through
                    // to the interactive stance, so a non-interactive session
                    // denied every tool its card had explicitly allowed.
                    let desired_kind = match declared {
                        Some(ToolPolicy::Allow) => PermissionOptionKind::AllowOnce,
                        Some(ToolPolicy::Deny) => PermissionOptionKind::RejectOnce,
                        _ => {
                            let args = request
                                .tool_call
                                .fields
                                .raw_input
                                .clone()
                                .unwrap_or(serde_json::Value::Null);
                            let diffs =
                                crate::tools::diff_synth::synthesize_diffs(&tool_name, &args);
                            let permission = PermRequest::tool(tool_name, args).with_diffs(diffs);
                            outcome_kind(&gate.request_permission(permission).await)
                        }
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

    /// Run one registry's `pre_llm_call` handlers over the prompt, chained.
    ///
    /// Returns the transformed prompt and whether a handler cancelled — which
    /// cancels the TURN: the caller emits the ended event and returns `None`,
    /// same as the reactor path and `transform_context`. (The old loop
    /// `break`-ed on Cancel and sent the prompt anyway — a cancel that
    /// didn't cancel.) Extracted so the session VM and the plugin VM share
    /// one loop body: plugins registering this event used to get documented
    /// silence, because only the per-session registry was ever dispatched.
    async fn run_pre_llm_call_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current_content: String,
    ) -> (String, bool) {
        for handler in registry.runtime_handlers_for("pre_llm_call", None) {
            let event = SessionEvent::Custom {
                name: "pre_llm_call".to_string(),
                payload: serde_json::json!({
                    "prompt": &current_content,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, &handler.name, &event, Some(&stream_ctx.session_id))
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
                    return (current_content, true);
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
        (current_content, false)
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

        // Session-scoped handlers first (more specific), under the state
        // lock; then plugin handlers with the lock RELEASED — plugin Lua can
        // call `cru.shell`/`cru.http` for seconds, and holding the session's
        // whole state across that starves everything else on the session.
        let (content, mut cancelled) = Self::run_pre_llm_call_handlers(
            stream_ctx,
            &state.registry,
            &state.lua,
            &stream_config.model,
            current_content,
        )
        .await;
        current_content = content;
        drop(state);

        if !cancelled {
            if let Some((plugin_registry, plugin_lua)) =
                stream_ctx.agent_stream_config.plugin_handlers.as_ref()
            {
                let (content, plugin_cancelled) = Self::run_pre_llm_call_handlers(
                    stream_ctx,
                    plugin_registry,
                    plugin_lua,
                    &stream_config.model,
                    current_content,
                )
                .await;
                current_content = content;
                cancelled = plugin_cancelled;
            }
        }

        // A crucible.on Cancel cancels the TURN — same as the reactor path
        // above and transform_context. It used to merely stop the handler
        // chain and send the prompt anyway.
        if cancelled {
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::ended(
                    &stream_ctx.session_id,
                    "cancelled by pre_llm_call handler".to_string(),
                ),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for cancelled event"
                );
            }
            return None;
        }

        Some(current_content)
    }

    /// Run one registry's `transform_context` handlers over the messages,
    /// chained. `Err(())` means a handler cancelled the turn (the ended
    /// event has already been emitted).
    async fn run_transform_context_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current: Vec<crucible_core::traits::ContextMessage>,
    ) -> Result<Vec<crucible_core::traits::ContextMessage>, ()> {
        for handler in registry.runtime_handlers_for("transform_context", None) {
            let event = SessionEvent::Custom {
                name: "transform_context".to_string(),
                payload: serde_json::json!({
                    "messages": &current,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, &handler.name, &event, Some(&stream_ctx.session_id))
                .await
            {
                Ok(crucible_lua::ScriptHandlerResult::Transform(val)) => {
                    if let Some(msgs_val) = val.get("messages") {
                        match serde_json::from_value::<Vec<crucible_core::traits::ContextMessage>>(
                            msgs_val.clone(),
                        ) {
                            Ok(new_messages) => current = new_messages,
                            Err(e) => warn!(
                                session_id = %stream_ctx.session_id,
                                handler = %handler.name,
                                error = %e,
                                "transform_context handler returned invalid messages, keeping previous"
                            ),
                        }
                    }
                }
                Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        reason = %reason,
                        "transform_context handler cancelled"
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::ended(
                            &stream_ctx.session_id,
                            format!("cancelled by handler: {}", handler.name),
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for cancelled event"
                        );
                    }
                    return Err(());
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        handler = %handler.name,
                        error = %error,
                        "transform_context handler error (fail-open)"
                    );
                }
            }
        }
        Ok(current)
    }

    /// Fire `transform_context` for handlers that want to mutate the
    /// `Vec<ContextMessage>` going to the provider. This is the rich-
    /// message-array seam (Pi's `transformContext`); the existing
    /// `pre_llm_call` is the later string-level seam. Both fire per turn.
    ///
    /// Returns the (possibly mutated) message vec. `None` means a
    /// handler cancelled and the caller should abort the turn.
    pub(super) async fn apply_transform_context_handlers(
        messages: Vec<crucible_core::traits::ContextMessage>,
        stream_ctx: &StreamContext,
        stream_config: &AgentStreamConfig,
    ) -> Option<Vec<crucible_core::traits::ContextMessage>> {
        let state = stream_ctx.session_state.lock().await;
        let mut current = messages;

        // Built-in producer: prepend the pre-computed Precognition
        // system block, if any. Runs *before* Lua handlers so plugins
        // can observe and mutate it via the same `transform_context`
        // seam they'd use to inject their own context.
        //
        // We protect against accidental drop: a buggy handler that
        // returns `{messages = ...}` without including the precog block
        // would otherwise silently strip kiln context. Below, after
        // Lua handlers run, we check whether the block is still
        // present and re-prepend if not.
        // `@file` attachments go in first so the Precognition block lands
        // above them: kiln context frames the conversation, an attached file
        // is about this message.
        if let Some(ref attachment) = stream_ctx.attachment_message {
            let mut with_attachment = Vec::with_capacity(current.len() + 1);
            with_attachment.push(attachment.clone());
            with_attachment.extend(current);
            current = with_attachment;
        }

        if let Some(ref precog_msg) = stream_ctx.precognition_message {
            let mut with_precog = Vec::with_capacity(current.len() + 1);
            with_precog.push(precog_msg.clone());
            with_precog.extend(current);
            current = with_precog;
        }

        // Lua runtime handlers can replace the message array entirely by
        // returning `{ messages = ... }`. Session-scoped handlers first,
        // under the state lock; then plugin handlers with the lock released
        // (same rationale as `apply_pre_llm_call_handlers`).
        current = match Self::run_transform_context_handlers(
            stream_ctx,
            &state.registry,
            &state.lua,
            &stream_config.model,
            current,
        )
        .await
        {
            Ok(messages) => messages,
            Err(()) => return None,
        };
        drop(state);

        if let Some((plugin_registry, plugin_lua)) =
            stream_ctx.agent_stream_config.plugin_handlers.as_ref()
        {
            current = match Self::run_transform_context_handlers(
                stream_ctx,
                plugin_registry,
                plugin_lua,
                &stream_config.model,
                current,
            )
            .await
            {
                Ok(messages) => messages,
                Err(()) => return None,
            };
        }

        // Defensive: if a Lua handler returned a new `messages` array
        // that dropped the built-in Precognition block, re-prepend it.
        //
        // Identity is by `metadata.tags` containing PRECOGNITION_TAG,
        // not by content. This lets a Lua handler legitimately mutate
        // the precog content (translate, redact, summarize) as long as
        // it preserves the tag — only handlers that fully rebuild the
        // array from scratch (losing metadata) or remove the message
        // entirely trigger the re-prepend.
        //
        // A plugin that explicitly wants to suppress Precognition
        // should configure that at the agent layer
        // (`agent_config.precognition_enabled = false`), not via the
        // messages array — silent stripping is what we're guarding
        // against, not legitimate config.
        if let Some(ref precog_msg) = stream_ctx.precognition_message {
            let still_present = current.iter().any(|m| {
                m.metadata
                    .tags
                    .iter()
                    .any(|t| t == crate::agent_manager::precognition::PRECOGNITION_TAG)
            });
            if !still_present {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "transform_context handler dropped Precognition message; re-prepending. \
                     To suppress legitimately, set precognition_enabled=false in agent config."
                );
                let mut with_precog = Vec::with_capacity(current.len() + 1);
                with_precog.push(precog_msg.clone());
                with_precog.extend(current);
                current = with_precog;
            }
        }

        Some(current)
    }

    /// Evaluate ONLY the `[permissions]` config deny rules for a tool call.
    /// Used where the full gate is skipped (card `allow`) but the operator's
    /// deny must still be absolute. `None` = no config or no deny match.
    pub(super) fn config_deny_reason(
        stream_ctx: &StreamContext,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Option<String> {
        use crucible_core::config::components::permissions::PermissionDecision;
        let engine = stream_ctx.permission_engine.as_ref()?;
        let input = if tool_name == "bash" {
            args.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            args.to_string()
        };
        match engine.evaluate(tool_name, &input, true) {
            PermissionDecision::Deny { reason } => Some(reason),
            PermissionDecision::Allow | PermissionDecision::Ask { .. } => None,
        }
    }

    /// Run the permission gate.
    ///
    /// `Ok(Some(reason))` means the call was approved WITHOUT asking, and by
    /// which layer; `Ok(None)` means the user was asked and said yes. The
    /// caller carries the reason on the `tool_call` event — the decision is
    /// made before that event is emitted, so an auto-approval marker can ride
    /// along with the card rather than arriving after it and popping in.
    /// Evaluate a mode's own rule lists with the shared permission engine.
    ///
    /// Built per call rather than cached: a mode is redefinable at any time,
    /// and the rule lists are a handful of strings. If that ever shows up in a
    /// profile, cache on the mode's identity, not on the session.
    pub(in crate::agent_manager) fn evaluate_mode_rules(
        permissions: &crucible_lua::ModePermissions,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> PermissionDecision {
        use crucible_core::config::components::permissions::PermissionConfig;
        let config = PermissionConfig {
            default: match permissions.default {
                crucible_lua::ModeStance::Allow => PermissionMode::Allow,
                crucible_lua::ModeStance::Deny => PermissionMode::Deny,
                crucible_lua::ModeStance::Ask => PermissionMode::Ask,
            },
            allow: permissions.allow.clone(),
            deny: permissions.deny.clone(),
            ask: permissions.ask.clone(),
        };
        let engine = PermissionEngine::new(Some(&config));
        let input = if tool_name == "bash" {
            args.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            args.to_string()
        };
        // `is_interactive: true` deliberately: the non-interactive ask→deny
        // conversion is the caller's job below, and doing it here would skip
        // the prompt path entirely.
        engine.evaluate(tool_name, &input, true)
    }

    pub(super) async fn handle_permission_request(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call_id: &str,
        args: &serde_json::Value,
    ) -> Result<Option<String>, String> {
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
                return Ok(Some("permission override".to_string()));
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
                return Err(error_msg);
            }
            Some(PermissionMode::Ask) | None => {}
        }

        // Global `[permissions]` config — previously enforced for ACP agents
        // only. Config deny is absolute; config allow (incl. `default =
        // "allow"`) short-circuits the gate. Ask (or no matching rule) falls
        // through to PatternStore → Lua hooks → prompt. The engine is asked
        // with `is_interactive: true` deliberately: its own ask→deny
        // conversion would skip the fall-through layers, and the prompt
        // branch below already handles non-interactive turns.
        if let Some(engine) = &stream_ctx.permission_engine {
            use crucible_core::config::components::permissions::PermissionDecision;
            let engine_input = if tool_call.name == "bash" {
                args.get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                args.to_string()
            };
            match engine.evaluate(&tool_call.name, &engine_input, true) {
                PermissionDecision::Allow => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "Permissions config allows tool, skipping prompt"
                    );
                    return Ok(Some("permissions config".to_string()));
                }
                PermissionDecision::Deny { reason } => {
                    let error_msg = format!(
                        "Tool '{}' denied by permissions config: {reason}",
                        tool_call.name
                    );
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
                            tool = %tool_call.name,
                            "No subscribers for config-denied tool_result event"
                        );
                    }
                    return Err(error_msg);
                }
                PermissionDecision::Ask { .. } => {}
            }
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
            return Ok(Some("saved pattern".to_string()));
        }

        // The mode's declared stance. Consulted AFTER the hooks below, because
        // a stance is static and a hook is a decision — `cru.modes.auto` says
        // "allow by default", and a user hook that denies bash must still win.
        let mode_permissions = stream_ctx
            .agent_stream_config
            .modes
            .get(&stream_ctx.session_mode)
            .map(|m| m.permissions);

        let hook_result = Self::execute_permission_hooks_with_timeout(
            &stream_ctx.session_state,
            &tool_call.name,
            args,
            &stream_ctx.session_id,
            &stream_ctx.session_mode,
            &stream_ctx.agent_stream_config.mcp_read_only_tools,
        )
        .await;

        match hook_result {
            PermissionHookResult::Allow => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook allowed tool, skipping permission prompt"
                );
                Ok(Some(if stream_ctx.session_mode == "auto" {
                    "auto mode".to_string()
                } else {
                    "Lua permission hook".to_string()
                }))
            }
            PermissionHookResult::Deny => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    "Lua hook denied tool"
                );
                let resource_desc = Self::brief_resource_description(&tool_call.name, args);
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
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for hook denied tool_result event"
                    );
                }
                Err(error_msg)
            }
            PermissionHookResult::Prompt => {
                // No hook had an opinion: fall back to the mode's own rules,
                // then its default stance.
                //
                // The rules use the `[permissions]` grammar and the SAME
                // engine, so `bash:rg *` inherits its chained-command handling
                // — a mode that permits `rg` does not thereby permit
                // `rg foo && rm -rf /`. Writing a second matcher here would
                // have been the easy way to lose that.
                let mode_stance = match &mode_permissions {
                    Some(p) if p.has_rules() => {
                        match Self::evaluate_mode_rules(p, &tool_call.name, args) {
                            PermissionDecision::Allow => Some(crucible_lua::ModeStance::Allow),
                            PermissionDecision::Deny { .. } => Some(crucible_lua::ModeStance::Deny),
                            PermissionDecision::Ask { .. } => Some(crucible_lua::ModeStance::Ask),
                        }
                    }
                    Some(p) => Some(p.default),
                    None => None,
                };

                match mode_stance {
                    Some(crucible_lua::ModeStance::Allow) => {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            mode = %stream_ctx.session_mode,
                            "mode stance allows tool, skipping permission prompt"
                        );
                        return Ok(Some(format!("{} mode", stream_ctx.session_mode)));
                    }
                    Some(crucible_lua::ModeStance::Deny) => {
                        let error_msg = format!(
                            "Tool '{}' is not permitted in {} mode",
                            tool_call.name, stream_ctx.session_mode
                        );
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
                                tool = %tool_call.name,
                                "No subscribers for mode-denied tool_result event"
                            );
                        }
                        return Err(error_msg);
                    }
                    Some(crucible_lua::ModeStance::Ask) | None => {}
                }

                // Non-interactive turns (delegated child sessions, headless
                // sends) have nobody to answer a prompt — deny immediately
                // with an actionable message instead of hanging.
                if !stream_ctx.is_interactive {
                    let error_msg = format!(
                        "Permission required for '{}' but this session runs non-interactively. \
                         Allow it via a permission pattern, Lua permission hook, or permissions \
                         config.",
                        tool_call.name
                    );
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
                            tool = %tool_call.name,
                            "No subscribers for non-interactive deny tool_result event"
                        );
                    }
                    return Err(error_msg);
                }

                let diffs = crate::tools::diff_synth::synthesize_diffs(&tool_call.name, args);
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

                // Bounded wait (parity with the ACP gate's 300 s): an
                // unanswered prompt must not wedge the turn forever.
                let response_result =
                    tokio::time::timeout(std::time::Duration::from_secs(300), response_rx).await;
                let (permission_granted, deny_reason) = match response_result {
                    Err(_elapsed) => {
                        if let Some(mut session_map) = stream_ctx
                            .pending_permissions
                            .get_mut(stream_ctx.session_id.as_str())
                        {
                            session_map.remove(&permission_id);
                        }
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_call.name,
                            permission_id = %permission_id,
                            "Permission prompt timed out, treating as deny"
                        );
                        (false, Some("permission prompt timed out".to_string()))
                    }
                    Ok(response_rx_result) => match response_rx_result {
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
                                        if let Err(e) = Self::store_pattern(
                                            &tool_call.name,
                                            pattern,
                                            &project_path,
                                        ) {
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
                    },
                };

                if permission_granted {
                    // The user was asked and said yes: nothing was granted on
                    // their behalf, so there is nothing to mark.
                    return Ok(None);
                }

                let resource_desc = Self::brief_resource_description(&tool_call.name, args);
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
                        serde_json::json!({ "error": &error_msg }),
                    ),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "No subscribers for permission denied tool_result event"
                    );
                }
                Err(error_msg)
            }
        }
    }

    /// A short description of what a tool call is acting on, for deny
    /// messages. Delegates to the shared projection so the phrase in an error
    /// matches what the UIs show for the same call.
    pub(in crate::agent_manager) fn brief_resource_description(
        tool_name: &str,
        args: &serde_json::Value,
    ) -> String {
        crucible_core::types::ToolDisplay::of(tool_name, args)
            .summary(50)
            .unwrap_or_default()
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
        session_mode: &str,
        mcp_read_only: &std::collections::HashSet<String>,
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
            mode: Some(session_mode.to_string()),
            is_safe: crate::agent_manager::believed_read_only(tool_name, mcp_read_only),
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

#[cfg(test)]
mod acp_tool_name_tests {
    use super::*;
    use agent_client_protocol::{ToolCallUpdateFields, ToolKind};
    use crucible_core::interaction::PermRequest;
    use crucible_core::traits::PermissionGate;

    fn fields(kind: Option<ToolKind>, title: &str) -> ToolCallUpdateFields {
        // `ToolCallUpdateFields` is `#[non_exhaustive]`, so it is built by
        // mutation rather than a struct literal.
        let mut f = ToolCallUpdateFields::default();
        f.kind = kind;
        f.title = Some(title.to_string());
        f
    }

    /// The gate must key on something an operator can write a rule against.
    ///
    /// It keyed on `title` — `"Read src/main.rs"` — which is prose. No
    /// `[permissions]` entry naming a tool could ever match it, so the entire
    /// permission config was inert for ACP-hosted agents.
    #[test]
    fn the_tool_name_comes_from_the_kind_not_the_prose_title() {
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Edit), "Edit src/main.rs")),
            "edit"
        );
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Execute), "Run cargo test")),
            "bash"
        );
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Read), "Read README")),
            "read"
        );
    }

    /// An unclassified call gets a name no file rule matches, rather than
    /// being guessed into one.
    #[test]
    fn an_unidentified_call_does_not_inherit_a_file_rule() {
        assert_eq!(acp_tool_name(&fields(None, "Something")), "acp_tool");
        assert_eq!(
            acp_tool_name(&fields(Some(ToolKind::Other), "Something")),
            "acp_tool"
        );
    }

    /// The point of all of it: an operator's rule now blocks an ACP call.
    ///
    /// Asserted through the gate rather than on the mapping alone, because a
    /// name that is correct and that no rule is evaluated against is the
    /// failure this fixes.
    #[tokio::test]
    async fn an_operator_rule_blocks_an_acp_tool_call() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["edit:*".to_string()],
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), true);

        let name = acp_tool_name(&fields(Some(ToolKind::Edit), "Edit src/main.rs"));
        let response = gate
            .request_permission(PermRequest::tool(
                name,
                serde_json::json!({"path": "src/main.rs"}),
            ))
            .await;

        assert!(
            !response.allowed,
            "deny = [\"edit:*\"] must reach an ACP edit"
        );
    }

    /// …and an ACP `kind` never buys the read-only exemption, because the
    /// agent supplies it. Same reasoning `is_safe` gives for `readOnlyHint`.
    #[tokio::test]
    async fn a_read_kind_does_not_skip_the_prompt() {
        let gate = DaemonPermissionGate::new(None, true);
        let name = acp_tool_name(&fields(Some(ToolKind::Read), "Read /etc/passwd"));

        let response = gate
            .request_permission(PermRequest::tool(name, serde_json::json!({})))
            .await;

        assert!(
            !response.allowed,
            "an agent-supplied kind must not widen the gate"
        );
    }
}
