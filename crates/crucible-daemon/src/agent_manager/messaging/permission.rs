use super::super::*;
use crucible_core::config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::types::CanonicalToolCall;
use crucible_lua::StageId;
use std::future::Future;
use std::ops::ControlFlow;

use crate::agent_manager::vm_pass::run_handlers;

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

/// Which of a [`PatternStore`]'s three rule tables owns a tool call.
///
/// A closed set: the store has these three tables and no fourth, so the
/// matches below are exhaustive and there is no `Default` — a name that fits
/// nowhere must be decided here, not fall into a table by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternKind {
    /// A bash rule, matched against the shell command line carried here.
    Bash(String),
    /// A file rule, matched against each path of the call.
    File,
    /// A tool rule, matched against the tool name.
    Tool,
}

/// Classify a tool call for the pattern store.
///
/// **One classifier, read by the writer and the reader.** Storing a grant and
/// checking it are two halves of one "always allow" click; if they disagree
/// about what a call is, the click writes a rule that can never match and the
/// user is prompted again for ever, with no way to see why.
///
/// Both halves read the canonical call, which Crucible's own tools and the
/// ACP boundary both produce. A `command` call with a command line is a bash
/// grant, whichever tool made it, so one grant for `cargo test` covers the
/// same command from each shell tool. A `file_edit` call with
/// paths is a file grant. Any other call is a grant for its canonical tool
/// name. [`PermRequest::suggested_pattern`] offers the same thing: the
/// command line for a command, and the tool name for another tool.
fn pattern_kind(call: &CanonicalToolCall) -> PatternKind {
    match (call.kind.as_str(), &call.command) {
        ("command", Some(command)) => PatternKind::Bash(command.clone()),
        ("file_edit", _) if !call.paths.is_empty() => PatternKind::File,
        _ => PatternKind::Tool,
    }
}

/// The agent option that carries the gate decision.
///
/// An allow always answers `allow_once`, also for a saved pattern or a wide
/// scope. An agent can store `allow_always` and stop asking about the tool
/// (gemini-cli does), and then Crucible has no gate for its later calls.
/// Crucible keeps the grant itself. A one-time allow never takes
/// `allow_always`, because that grant is wider than the user chose. A denial
/// takes `reject_always` when the agent offers no `reject_once`. `Cancelled`
/// remains only for no usable option, because the agent then stops the
/// whole turn.
fn select_option(
    options: &[agent_client_protocol::schema::v1::PermissionOption],
    response: &PermResponse,
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    use agent_client_protocol::schema::v1::{
        PermissionOptionKind as Kind, RequestPermissionOutcome, SelectedPermissionOutcome,
    };

    let acceptable: &[Kind] = if response.allowed {
        &[Kind::AllowOnce]
    } else {
        &[Kind::RejectOnce, Kind::RejectAlways]
    };

    acceptable
        .iter()
        .find_map(|kind| options.iter().find(|opt| opt.kind == *kind))
        .map(|opt| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                opt.option_id.clone(),
            ))
        })
        .unwrap_or(RequestPermissionOutcome::Cancelled)
}

/// Answer one `session/request_permission`.
///
/// A free function, not a closure body: this IS the ACP half of the tool
/// policy, and a test drives it with the frames the real agents send.
///
/// `call` is the canonical call that the ACP client joined from the request
/// and the earlier frames of its `toolCallId`. The card keys on its
/// canonical `tool`: the name the agent sent, a Crucible tool without its
/// MCP prefix, or the kind of a call that nothing names. The operator's rules
/// read the whole call, as for Crucible's own tools. The diff of the prompt
/// is the diff of the call.
///
/// The card policy goes INTO the gate rather than answering ahead of it. The
/// two halves used to disagree about the same session's `tool_policy` — an
/// ACP agent's card `allow` walked past an operator `deny` that the daemon's
/// own agents obeyed.
///
/// A call the agent never asks about is the agent's own decision. It runs its
/// own tools in its own process, so a refusal that arrives after the fact
/// stops nothing; Crucible answers what it is asked, and nothing more.
async fn decide_acp_permission(
    gate: &DaemonPermissionGate,
    tool_policy: Option<&crucible_core::agent::ToolPolicyMap>,
    call: CanonicalToolCall,
    options: &[agent_client_protocol::schema::v1::PermissionOption],
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    let card_policy = tool_policy.and_then(|map| map.get(&call.tool)).copied();
    let response = gate.request_permission(call, card_policy).await;
    select_option(options, &response)
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
        let slot = self.slot(session_id);
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();
        let serializer = PermissionSerializer::new();

        let ask_callback: PermissionPromptCallback = Arc::new(move |perm_request: PermRequest| {
            let slot = slot.clone();
            let session_id_owned = session_id_owned.clone();
            let event_tx_owned = event_tx_owned.clone();
            let serializer = serializer.clone();

            Box::pin(async move {
                serializer
                    .run(async move {
                        let (permission_id, response_rx) =
                            slot.register_permission(perm_request.clone());

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
                                slot.take_permission(&permission_id);
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
                                slot.take_permission(&permission_id);
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

        let gate = Arc::new(
            DaemonPermissionGate::new(effective_config, is_interactive)
                .with_prompt_callback(ask_callback),
        );

        let tool_policy = tool_policy.map(Arc::new);

        Arc::new(move |call, options| {
            let gate = gate.clone();
            let tool_policy = tool_policy.clone();

            Box::pin(async move {
                decide_acp_permission(&gate, tool_policy.as_deref(), call, &options).await
            })
        })
    }

    /// Run one registry's `pre_llm_call` handlers over the prompt, chained.
    ///
    /// Returns the transformed prompt and whether a handler cancelled — which
    /// cancels the TURN: the caller returns `None`,
    /// same as the reactor path and `transform_context`. (The old loop
    /// `break`-ed on Cancel and sent the prompt anyway — a cancel that
    /// didn't cancel.) Extracted so every caller shares
    /// one loop body: plugins registering this event used to get documented
    /// silence, because only the per-session registry was ever dispatched.
    async fn run_pre_llm_call_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current_content: String,
    ) -> (String, bool) {
        for handler in registry.runtime_handlers_for(
            StageId::PreLlmCall.as_str(),
            None,
            crucible_lua::Firing::InSession(&stream_ctx.session_id),
        ) {
            let event = SessionEvent::Custom {
                name: "pre_llm_call".to_string(),
                payload: serde_json::json!({
                    "prompt": &current_content,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
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
        // Session-scoped handlers first (more specific), under the state
        // lock; then plugin handlers with the lock RELEASED — plugin Lua can
        // call `cru.shell`/`cru.http` for seconds, and holding the session's
        // whole state across that starves everything else on the session.
        let (current_content, cancelled) = run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            (content, false),
            |registry, lua, (content, _)| {
                Box::pin(async move {
                    let (content, cancelled) = Self::run_pre_llm_call_handlers(
                        stream_ctx,
                        &registry,
                        &lua,
                        &stream_config.model,
                        content,
                    )
                    .await;
                    if cancelled {
                        ControlFlow::Break((content, true))
                    } else {
                        ControlFlow::Continue((content, false))
                    }
                })
            },
        )
        .await;

        // A cru.on Cancel cancels the TURN — same as the reactor path
        // above and transform_context. It used to merely stop the handler
        // chain and send the prompt anyway.
        if cancelled {
            return None;
        }

        Some(current_content)
    }

    /// Run one registry's `transform_context` handlers over the messages,
    /// chained. `Err(())` means a handler cancelled the turn.
    async fn run_transform_context_handlers(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        model: &str,
        mut current: Vec<crucible_core::traits::ContextMessage>,
    ) -> Result<Vec<crucible_core::traits::ContextMessage>, ()> {
        for handler in registry.runtime_handlers_for(
            StageId::TransformContext.as_str(),
            None,
            crucible_lua::Firing::InSession(&stream_ctx.session_id),
        ) {
            let event = SessionEvent::Custom {
                name: "transform_context".to_string(),
                payload: serde_json::json!({
                    "messages": &current,
                    "model": model,
                }),
            };
            match registry
                .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
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
                                handler = handler.id,
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
                    return Err(());
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        handler = handler.id,
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
        current = run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            current,
            |registry, lua, current| {
                Box::pin(async move {
                    match Self::run_transform_context_handlers(
                        stream_ctx,
                        &registry,
                        &lua,
                        &stream_config.model,
                        current,
                    )
                    .await
                    {
                        Ok(messages) => ControlFlow::Continue(messages),
                        Err(()) => ControlFlow::Break(None),
                    }
                })
            },
        )
        .await?;

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

    /// Evaluate a mode's own rule lists with the shared permission engine.
    ///
    /// Built per call rather than cached: a mode is redefinable at any time,
    /// and the rule lists are a handful of strings. If that ever shows up in a
    /// profile, cache on the mode's identity, not on the session.
    pub(in crate::agent_manager) fn evaluate_mode_rules(
        permissions: &crucible_lua::ModePermissions,
        call: &CanonicalToolCall,
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
        // `is_interactive: true` deliberately: the non-interactive ask→deny
        // conversion is the caller's job below, and doing it here would skip
        // the prompt path entirely.
        engine.evaluate_call(call, args, true)
    }

    /// Run the permission gate.
    ///
    /// `Ok(Some(reason))` means the call was approved WITHOUT asking, and by
    /// which layer; `Ok(None)` means the user was asked and said yes. The
    /// caller carries the reason on the `tool_call` event — the decision is
    /// made before that event is emitted, so an auto-approval marker can ride
    /// along with the card rather than arriving after it and popping in.
    ///
    /// `call` is the canonical form of `tool_call`. Every rule, saved
    /// pattern and hook below reads it.
    pub(super) async fn handle_permission_request(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call: &CanonicalToolCall,
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
            match engine.evaluate_call(call, args, true) {
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
        // A grant at either persisted scope skips the prompt. Both stores
        // live under the injected directory, so a test never reads a real one.
        let pattern_store = match stream_ctx.whitelists_dir.as_deref() {
            Some(dir) => PatternStore::load_sync_in(dir, &project_path)
                .unwrap_or_default()
                .merge(&PatternStore::load_user_sync_in(dir).unwrap_or_default()),
            None => PatternStore::default(),
        };
        let pattern_matched = Self::check_pattern_match(call, &pattern_store);

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

        let hook_result = Self::run_permission_hooks(
            stream_ctx.agent_stream_config.daemon_permissions.as_ref(),
            call,
            args,
            &stream_ctx.session_id,
            &stream_ctx.session_mode,
            &stream_ctx.agent_stream_config.mcp_read_only_tools,
        );

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
                    Some(p) if p.has_rules() => match Self::evaluate_mode_rules(p, call, args) {
                        PermissionDecision::Allow => Some(crucible_lua::ModeStance::Allow),
                        PermissionDecision::Deny { .. } => Some(crucible_lua::ModeStance::Deny),
                        PermissionDecision::Ask { .. } => Some(crucible_lua::ModeStance::Ask),
                    },
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
                let (permission_id, response_rx) =
                    stream_ctx.slot.register_permission(perm_request);

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
                        stream_ctx.slot.take_permission(&permission_id);
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
                                    if let Some(file) =
                                        stream_ctx.whitelists_dir.as_deref().and_then(|dir| {
                                            PatternStore::store_file_in(
                                                dir,
                                                response.scope,
                                                &project_path,
                                            )
                                        })
                                    {
                                        if let Err(e) = Self::store_pattern_to(&file, call, pattern)
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
        crucible_core::types::CanonicalToolCall::crucible_tool(tool_name, args)
            .summary(50)
            .unwrap_or_default()
    }

    /// Does a stored grant already answer for this call?
    ///
    /// Routes through [`pattern_kind`], the same classifier
    /// [`Self::store_pattern_to`] writes with.
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants
    )]
    pub(in crate::agent_manager) fn check_pattern_match(
        call: &CanonicalToolCall,
        pattern_store: &PatternStore,
    ) -> bool {
        match pattern_kind(call) {
            PatternKind::Bash(command) => pattern_store.matches_bash(&command),
            // A grant for one path must not permit an edit of another.
            PatternKind::File => call.paths.iter().all(|p| pattern_store.matches_file(p)),
            PatternKind::Tool => pattern_store.matches_tool(&call.tool),
        }
    }

    /// Add `pattern` to the store at `file`, which a `Project` or `User`
    /// grant resolves through [`PatternStore::store_file_in`].
    ///
    /// `call` is the call the user answered about. It is what
    /// [`pattern_kind`] reads, so the table this writes into is the table
    /// [`Self::check_pattern_match`] will read on the next identical call.
    ///
    /// The daemon is the only writer of a store file, so one process-wide
    /// lock serializes the load, the update and the save. Without it two
    /// sessions that grant at the same time overwrite each other.
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants
    )]
    pub(in crate::agent_manager) fn store_pattern_to(
        file: &std::path::Path,
        call: &CanonicalToolCall,
        pattern: &str,
    ) -> Result<(), crucible_core::config::PatternError> {
        static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = STORE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut store = PatternStore::load_file(file).unwrap_or_default();

        match pattern_kind(call) {
            PatternKind::Bash(_) => store.add_bash_pattern(pattern)?,
            PatternKind::File => store.add_file_pattern(pattern)?,
            PatternKind::Tool => store.add_tool_pattern(pattern)?,
        }

        store.save_file(file)?;
        Ok(())
    }

    /// Ask this session's `cru.permissions.on_request` hooks, in priority
    /// order.
    ///
    /// One registry, on the one VM that runs Lua files. `None` is a manager
    /// with no daemon VM bound — every hook-free test — and means Prompt.
    ///
    /// `crucible_lua::handler_budget` gives the hooks 1 s. The VM stops a hook
    /// that runs longer, and this then returns `Prompt`.
    pub(in crate::agent_manager) fn run_permission_hooks(
        registry: Option<&super::super::DaemonPermissions>,
        call: &CanonicalToolCall,
        args: &serde_json::Value,
        session_id: &str,
        session_mode: &str,
        mcp_read_only: &std::collections::HashSet<String>,
    ) -> PermissionHookResult {
        let Some((hooks, lua)) = registry else {
            return PermissionHookResult::Prompt;
        };
        let tool_name = call.tool.as_str();

        let file_path = args
            .get("path")
            .or_else(|| args.get("file"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let request = PermissionRequest {
            call: call.clone(),
            args: args.clone(),
            file_path,
            mode: Some(session_mode.to_string()),
            is_safe: crate::agent_manager::believed_read_only(tool_name, mcp_read_only),
        };

        match execute_permission_hooks(
            lua,
            hooks,
            &request,
            crucible_lua::Firing::InSession(session_id),
        ) {
            Ok(hook_result) => hook_result,
            Err(e) => {
                warn!(session_id = %session_id, tool = %tool_name, error = %e, "Permission hook failed");
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
pub(in crate::agent_manager) fn resolve_effective_permission_config(
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
mod acp_permission_tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome,
    };
    use crucible_core::types::{classify_acp, CanonicalToolCall};

    /// The canonical call of an ACP frame that names no tool.
    fn unnamed(kind: &str, title: &str) -> CanonicalToolCall {
        classify_acp(
            serde_json::from_value(serde_json::json!({
                "kind": kind,
                "title": title,
                "locations": [{"path": "src/main.rs"}],
            }))
            .expect("a raw tool call"),
            &[],
        )
    }

    /// An operator's rule blocks an ACP call that names no tool. The rule
    /// names the canonical kind, which is the name of such a call.
    ///
    /// Asserted through the gate rather than on the name alone, because a
    /// name that is correct and that no rule is evaluated against is the
    /// failure this guards.
    #[tokio::test]
    async fn an_operator_rule_blocks_an_acp_tool_call() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["file_edit:*".to_string()],
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), true);

        let call = unnamed("edit", "Edit src/main.rs");
        assert_eq!(call.tool, "file_edit", "the prose title is never the name");
        let response = gate.request_permission(call, None).await;

        assert!(
            !response.allowed,
            "deny = [\"file_edit:*\"] must reach an ACP edit"
        );
    }

    /// …and an ACP `kind` never buys the read-only exemption, because the
    /// agent supplies it. Same reasoning `is_safe` gives for `readOnlyHint`.
    #[tokio::test]
    async fn a_read_kind_does_not_skip_the_prompt() {
        let gate = DaemonPermissionGate::new(None, true);
        let call = unnamed("read", "Read /etc/passwd");

        let response = gate.request_permission(call, None).await;

        assert!(
            !response.allowed,
            "an agent-supplied kind must not widen the gate"
        );
    }

    fn option(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), id.to_string(), kind)
    }

    fn selected(outcome: RequestPermissionOutcome) -> Option<String> {
        match outcome {
            RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.to_string()),
            _ => None,
        }
    }

    /// A remembered grant still answers `allow_once`. An agent can store
    /// `allow_always` and stop asking about the tool (gemini-cli does), and
    /// then Crucible has no gate for the later calls of that tool.
    #[test]
    fn a_remembered_grant_answers_allow_once() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("always", PermissionOptionKind::AllowAlways),
            option("no", PermissionOptionKind::RejectOnce),
        ];
        use crucible_core::interaction::PermissionScope;
        for scope in [PermissionScope::Session, PermissionScope::Project] {
            let response = PermResponse::allow_pattern("cargo test", scope);
            assert_eq!(
                selected(select_option(&options, &response)).as_deref(),
                Some("once"),
                "{scope:?}"
            );
        }
    }

    /// A one-time allow never takes `allow_always`. That option gives the
    /// agent a wider grant than the user chose.
    #[test]
    fn an_allow_once_decision_never_takes_allow_always() {
        let options = [
            option("always", PermissionOptionKind::AllowAlways),
            option("no", PermissionOptionKind::RejectOnce),
        ];
        assert!(matches!(
            select_option(&options, &PermResponse::allow()),
            RequestPermissionOutcome::Cancelled
        ));
    }

    /// A denial takes `reject_always` when the agent offers no
    /// `reject_once`. `Cancelled` stopped the whole turn instead of one call.
    #[test]
    fn a_reject_once_decision_takes_reject_always_when_the_agent_offers_only_that() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("never", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(
            selected(select_option(&options, &PermResponse::deny())).as_deref(),
            Some("never")
        );
    }

    /// With no option of a usable kind, the outcome is `Cancelled`.
    #[test]
    fn no_usable_option_is_cancelled() {
        assert!(matches!(
            select_option(&[], &PermResponse::deny()),
            RequestPermissionOutcome::Cancelled
        ));
    }
}

/// The two agents' own permission frames, decided by the one tool policy.
///
/// The frames below are the shapes `claude-agent-acp` and `codex-acp` put on
/// the wire. They are the input the policy actually gets, so the tests build
/// them from JSON rather than from a builder that cannot be wrong.
#[cfg(test)]
mod acp_tool_policy_tests {
    use super::*;
    use agent_client_protocol::schema::v1::{RequestPermissionOutcome, RequestPermissionRequest};
    use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// One decided permission request, and whether the user was asked.
    struct Asked {
        outcome: RequestPermissionOutcome,
        prompts: usize,
    }

    impl Asked {
        fn allowed(&self) -> bool {
            matches!(self.outcome, RequestPermissionOutcome::Selected(ref selected)
                if selected.option_id.to_string().starts_with("allow"))
        }
    }

    /// The options every agent in these tests offers.
    fn options() -> serde_json::Value {
        serde_json::json!([
            { "optionId": "allow-once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "allow-always", "name": "Always", "kind": "allow_always" },
            { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" },
        ])
    }

    /// What claude-agent-acp sends: the tool name is on the request.
    fn claude_request(name: &str) -> RequestPermissionRequest {
        serde_json::from_value(serde_json::json!({
            "sessionId": "sess-1",
            "toolCall": {
                "toolCallId": "toolu_01",
                "name": name,
                "title": "Read the note",
                "status": "pending",
                "rawInput": { "title": "Some Note" },
            },
            "options": options(),
        }))
        .expect("the claude-agent-acp frame parses")
    }

    /// What codex-acp sends for an MCP approval: `kind: "execute"` and NO
    /// name. The ACP client joins the request to the `tool_call` frame that
    /// announced the same id, so `joined_name` is the name that join gave.
    fn codex_request(joined_name: Option<&str>) -> RequestPermissionRequest {
        let mut tool_call = serde_json::json!({
            "toolCallId": "call-1",
            "kind": "execute",
            "status": "pending",
        });
        if let Some(name) = joined_name {
            tool_call["name"] = serde_json::json!(name);
        }
        serde_json::from_value(serde_json::json!({
            "sessionId": "sess-1",
            "toolCall": tool_call,
            "_meta": { "is_mcp_tool_approval": true },
            "options": options(),
        }))
        .expect("the codex-acp frame parses")
    }

    /// Decide one request against a card and an operator config, and count
    /// the prompts. A prompt that never fires is the whole point of a
    /// declared policy, so it is measured, not assumed.
    async fn decide(
        request: RequestPermissionRequest,
        card: &[(&str, ToolPolicy)],
        config: Option<PermissionConfig>,
    ) -> Asked {
        let prompts = Arc::new(AtomicUsize::new(0));
        let counter = prompts.clone();
        let callback: PermissionPromptCallback = Arc::new(move |_request| {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { PermResponse::allow() })
        });
        let gate = DaemonPermissionGate::new(config, true).with_prompt_callback(callback);

        let policy: ToolPolicyMap = card
            .iter()
            .map(|(name, policy)| ((*name).to_string(), *policy))
            .collect();
        // The ACP client hands the permission path the canonical call.
        let call = crucible_core::types::classify_acp(
            crucible_core::types::RawToolCall::from(&request.tool_call),
            &[],
        );
        let outcome = decide_acp_permission(&gate, Some(&policy), call, &request.options).await;
        Asked {
            outcome,
            prompts: prompts.load(Ordering::SeqCst),
        }
    }

    /// A card `deny` refuses a Crucible MCP tool, and asks nobody.
    ///
    /// The card names the tool the daemon's own path names — `read_note` —
    /// while the agent sends `mcp__crucible__read_note`. One key, or the card
    /// silently does nothing for an ACP agent.
    #[tokio::test]
    async fn a_card_deny_refuses_a_crucible_mcp_tool() {
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Deny)],
            None,
        )
        .await;
        assert!(!asked.allowed(), "a card deny must refuse the call");
        assert_eq!(asked.prompts, 0, "a card deny asks nobody");
    }

    /// A card `allow` runs it, and asks nobody.
    #[tokio::test]
    async fn a_card_allow_runs_a_crucible_mcp_tool_without_a_prompt() {
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Allow)],
            None,
        )
        .await;
        assert!(asked.allowed(), "a card allow must run the call");
        assert_eq!(asked.prompts, 0, "a card allow asks nobody");
    }

    /// An operator rule reaches an ACP agent's tool.
    #[tokio::test]
    async fn an_operator_rule_refuses_a_crucible_mcp_tool() {
        let config = PermissionConfig {
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[],
            Some(config),
        )
        .await;
        assert!(!asked.allowed(), "`deny = [\"read_note:*\"]` must refuse");
        assert_eq!(asked.prompts, 0, "an operator deny asks nobody");
    }

    /// An operator `deny` outranks a card `allow` for an ACP agent too.
    ///
    /// The card answered ahead of the gate before, so an untrusted kiln could
    /// ship a card that walked past a configured deny.
    #[tokio::test]
    async fn an_operator_deny_outranks_a_card_allow() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let asked = decide(
            claude_request("mcp__crucible__read_note"),
            &[("read_note", ToolPolicy::Allow)],
            Some(config),
        )
        .await;
        assert!(
            !asked.allowed(),
            "a card allow must not beat an operator deny"
        );
    }

    /// What codex-acp (TypeScript) sends for a shell command: `kind:
    /// "execute"`, no name, and the command line in `rawInput.command`.
    /// The shape is the `shell` case of `tool_frames/codex-ts.jsonl`.
    fn codex_command_request() -> RequestPermissionRequest {
        serde_json::from_value(serde_json::json!({
            "sessionId": "sess-1",
            "toolCall": {
                "toolCallId": "call_shell1",
                "kind": "execute",
                "status": "pending",
                "title": "Run command",
                "rawInput": { "command": "cargo test", "cwd": "/home/user/project" },
            },
            "options": options(),
        }))
        .expect("the codex-acp frame parses")
    }

    /// An operator `bash` rule refuses a command that the agent does not
    /// name. The rule matches the command line of each `command` call, so
    /// the operator writes one rule for Crucible's shell and for the shell
    /// of each agent.
    #[tokio::test]
    async fn an_operator_bash_rule_refuses_an_unnamed_acp_command() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["bash:cargo *".to_string()],
            ..Default::default()
        };
        let asked = decide(codex_command_request(), &[], Some(config)).await;
        assert!(
            !asked.allowed(),
            "`deny = [\"bash:cargo *\"]` must refuse the agent's `cargo test`"
        );
        assert_eq!(asked.prompts, 0, "an operator deny asks nobody");
    }

    /// A codex MCP approval carries `kind: "execute"` and no name of its own.
    /// It must not be keyed as a shell command.
    ///
    /// A coarse mapping of the kind makes every `execute` a command, so a
    /// rule about the shell decided a note read, and a rule about the note
    /// read decided nothing.
    #[tokio::test]
    async fn a_codex_mcp_approval_is_not_a_command() {
        let joined = || codex_request(Some("mcp.crucible.read_note"));

        let as_command = decide(joined(), &[("command", ToolPolicy::Deny)], None).await;
        assert!(
            as_command.allowed(),
            "a rule about the shell must not decide an MCP note read"
        );

        let as_itself = decide(joined(), &[("read_note", ToolPolicy::Deny)], None).await;
        assert!(
            !as_itself.allowed(),
            "a rule about the note read must decide it"
        );
        assert_eq!(as_itself.prompts, 0, "a card deny asks nobody");
    }

    /// With no name at all — no `tool_call` frame ever announced this id —
    /// the fallback name is the last resort, and the call is asked about.
    #[tokio::test]
    async fn an_unnamed_call_still_reaches_the_gate() {
        let asked = decide(codex_request(None), &[], None).await;
        assert_eq!(
            asked.prompts, 1,
            "a call the daemon cannot identify is put to the user"
        );
    }

    /// A tool the agent names and Crucible does not know still reaches the
    /// gate. It is neither allowed for being unknown nor refused for it.
    #[tokio::test]
    async fn a_tool_crucible_does_not_know_still_reaches_the_gate() {
        let asked = decide(claude_request("mcp__github__create_pr"), &[], None).await;
        assert_eq!(
            asked.prompts, 1,
            "an unknown tool is asked about, not guessed at"
        );
        assert!(asked.allowed(), "and the user's answer is honoured");
    }
}

/// Tests that call the real handler that `build_acp_permission_handler`
/// returns. The unit tests above cover its parts. These tests cover the
/// order of the parts: the declared policy, then the gate, then the prompt.
#[cfg(test)]
mod acp_permission_handler_tests {
    use super::*;
    use crate::agent_manager::tests::create_test_agent_manager;
    use crate::test_support::temp_session_manager;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
    use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
    use crucible_core::interaction::PermResponse;
    use std::time::Duration;

    const SESSION: &str = "acp-perm-session";

    /// An ACP command call that offers exactly one allow and one reject.
    fn execute_request() -> RequestPermissionRequest {
        let mut fields = ToolCallUpdateFields::default();
        fields.kind = Some(ToolKind::Execute);
        fields.title = Some("Run cargo test".to_string());
        fields.raw_input = Some(serde_json::json!({ "command": "cargo test" }));
        RequestPermissionRequest::new(
            "acp-wire-session",
            ToolCallUpdate::new("call-1", fields),
            vec![
                PermissionOption::new("allow_once", "Allow once", PermissionOptionKind::AllowOnce),
                PermissionOption::new(
                    "reject_once",
                    "Reject once",
                    PermissionOptionKind::RejectOnce,
                ),
            ],
        )
    }

    fn selected(outcome: &RequestPermissionOutcome) -> Option<String> {
        match outcome {
            RequestPermissionOutcome::Selected(s) => Some(s.option_id.to_string()),
            _ => None,
        }
    }

    /// The call names no tool, so its canonical name is its kind.
    fn policy(stance: ToolPolicy) -> ToolPolicyMap {
        ToolPolicyMap::from([("command".to_string(), stance)])
    }

    /// Ask `handle` about [`execute_request`], as the ACP client does: with
    /// the canonical call and the options of the agent.
    fn ask(
        handle: &crate::acp::client::PermissionRequestHandler,
    ) -> crate::acp::client::PermissionOutcomeFuture {
        let request = execute_request();
        let call = crucible_core::types::classify_acp(
            crucible_core::types::RawToolCall::from(&request.tool_call),
            &[],
        );
        handle(call, request.options)
    }

    /// Build the handler for an interactive session with no override.
    fn handler(
        am: &AgentManager,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        tool_policy: Option<ToolPolicyMap>,
    ) -> crate::acp::client::PermissionRequestHandler {
        am.build_acp_permission_handler(SESSION, event_tx, true, None, None, tool_policy)
    }

    /// Read events until the prompt arrives. Return its request id.
    async fn prompt_id(rx: &mut broadcast::Receiver<SessionEventMessage>) -> String {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("the handler must emit interaction_requested")
                .expect("event channel open");
            if msg.event == "interaction_requested" {
                return msg.data["request_id"]
                    .as_str()
                    .expect("request_id")
                    .to_string();
            }
        }
    }

    /// A card that allows `command` answers the call. No prompt appears.
    #[tokio::test]
    async fn a_declared_allow_selects_allow_once_without_a_prompt() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, Some(policy(ToolPolicy::Allow)));

        let outcome = ask(&handle).await;

        assert_eq!(selected(&outcome).as_deref(), Some("allow_once"));
        assert!(
            event_rx.try_recv().is_err(),
            "a declared Allow must not prompt the user"
        );
        assert!(am.list_all_pending_permissions().is_empty());
    }

    /// A card that denies `command` refuses the call. No prompt appears.
    #[tokio::test]
    async fn a_declared_deny_selects_reject_once_without_a_prompt() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, Some(policy(ToolPolicy::Deny)));

        let outcome = ask(&handle).await;

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(
            event_rx.try_recv().is_err(),
            "a declared Deny must not prompt the user"
        );
    }

    /// With no policy, the handler asks the user. The answer goes through
    /// the manager reply API, as a client answer does.
    #[tokio::test]
    async fn with_no_policy_the_user_answer_selects_the_option() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        for (answer, expected) in [
            (PermResponse::allow(), "allow_once"),
            (PermResponse::deny(), "reject_once"),
        ] {
            let pending = tokio::spawn(ask(&handle));
            let id = prompt_id(&mut event_rx).await;
            am.respond_to_permission(SESSION, &id, answer)
                .expect("the prompt must be in the session registry");

            let outcome = tokio::time::timeout(Duration::from_secs(5), pending)
                .await
                .expect("the handler must return after the answer")
                .expect("join");
            assert_eq!(selected(&outcome).as_deref(), Some(expected));
        }
    }

    /// Nobody answers. After 300 s the handler rejects the call. It does not
    /// wait forever. It also removes the prompt from the registry.
    #[tokio::test(start_paused = true)]
    async fn an_unanswered_prompt_rejects_after_the_timeout() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let pending = tokio::spawn(ask(&handle));
        let _id = prompt_id(&mut event_rx).await;
        assert_eq!(am.list_all_pending_permissions().len(), 1);

        tokio::time::advance(Duration::from_secs(301)).await;
        let outcome = tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .expect("the handler must return after the 300 s timeout")
            .expect("join");

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(
            am.list_all_pending_permissions().is_empty(),
            "a timed-out prompt must leave the registry"
        );
    }
}
