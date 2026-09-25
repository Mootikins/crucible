use super::super::*;
use crucible_core::config::components::permissions::{
    PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::types::CanonicalToolCall;
use crucible_lua::StageId;
use std::ops::ControlFlow;

use super::gate_decision::{decide_permission, Decision, PermissionContext, Prompt};
use crate::agent_manager::slot::TurnGate;
use crate::agent_manager::vm_pass::run_handlers;
use std::sync::atomic::{AtomicBool, Ordering};

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
///
/// `None` for a command that Crucible cannot read, and for an edit with no
/// path. A grant for it would allow each such call, so no grant is stored.
fn pattern_kind(call: &CanonicalToolCall) -> Option<PatternKind> {
    match (call.kind.as_str(), &call.command) {
        ("command", Some(command)) => Some(PatternKind::Bash(command.clone())),
        ("command", None) => None,
        ("file_edit", _) if call.paths.is_empty() => None,
        ("file_edit", _) => Some(PatternKind::File),
        _ => Some(PatternKind::Tool),
    }
}

/// The agent option that carries the gate decision.
///
/// An allow always answers `allow_once`, also for a saved pattern or a wide
/// scope. An agent can store `allow_always` and stop asking about the tool
/// (gemini-cli does), and then Crucible has no gate for its later calls.
/// Crucible keeps the grant itself. A one-time allow never takes
/// `allow_always`, because that grant is wider than the user chose. For the
/// same reason a denial never takes `reject_always`: the agent stores it as
/// a deny that nobody chose. With no option of the one kind, the outcome is
/// `Cancelled`, and the agent stops the whole turn.
fn select_option(
    options: &[agent_client_protocol::schema::v1::PermissionOption],
    allowed: bool,
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    use agent_client_protocol::schema::v1::{
        PermissionOptionKind as Kind, RequestPermissionOutcome, SelectedPermissionOutcome,
    };

    let kind = if allowed {
        Kind::AllowOnce
    } else {
        Kind::RejectOnce
    };

    options
        .iter()
        .find(|opt| opt.kind == kind)
        .map(|opt| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                opt.option_id.clone(),
            ))
        })
        .unwrap_or(RequestPermissionOutcome::Cancelled)
}

/// The permission gate of one ACP session, in its two parts.
///
/// The agent runs its own tools and asks about some of them with
/// `session/request_permission`: [`Self::handler`] answers. The agent calls
/// Crucible's tools through the in-process MCP server, which runs them in
/// the daemon: [`Self::mcp_gate`] decides each such call before it runs.
/// Both parts decide with `decide_permission` and share the prompt of the
/// session.
#[derive(Clone)]
pub struct AcpPermissions {
    gate: Arc<AcpGate>,
}

impl AcpPermissions {
    /// The answer to the agent's `session/request_permission`.
    pub fn handler(&self) -> crate::acp::client::PermissionRequestHandler {
        let gate = self.gate.clone();
        Arc::new(move |call, options| {
            let gate = gate.clone();
            Box::pin(async move { gate.decide(call, &options).await })
        })
    }

    /// The gate of the in-process MCP server: each call of a Crucible tool
    /// passes it before it runs.
    pub fn mcp_gate(&self) -> crate::tools::mcp_server::McpCallGate {
        let gate = self.gate.clone();
        Arc::new(move |tool, args| {
            let gate = gate.clone();
            Box::pin(async move { gate.decide_crucible_call(&tool, args).await })
        })
    }

    /// Record that the agent takes the in-process MCP server with
    /// [`Self::mcp_gate`]. The server then decides each call to it, and the
    /// handler does not ask a second time about the same call.
    pub fn mcp_server_decides(&self) {
        self.gate.mcp_server_decides.store(true, Ordering::SeqCst);
    }
}

/// What the ACP permission handler of one session reads.
///
/// The handler lives as long as the cached agent handle. The turn state
/// (interactivity, override, mode) comes from the session slot for each
/// call, not from the turn that built the handle.
struct AcpGate {
    slot: Arc<crate::agent_manager::slot::SessionSlot>,
    session_id: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    workspace: PathBuf,
    whitelists_dir: Option<PathBuf>,
    hooks: Option<PluginHandlers>,
    /// Read at each call, as the internal path reads it for each turn.
    rules: crate::agent_manager::session_permissions::SessionRules,
    tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    /// True when the agent takes the in-process MCP server with the gate.
    mcp_server_decides: AtomicBool,
    /// The mode of an ACP session is the agent's own mode. Its id can name a
    /// Crucible mode (`auto`, `plan`) with another rule, so no Crucible mode
    /// stance applies.
    no_modes: crucible_lua::ModeRegistry,
    no_mcp: std::collections::HashSet<String>,
    sessions: Arc<crate::session_manager::SessionManager>,
}

impl AcpGate {
    /// What one decision reads, for the turn `turn`.
    fn context<'a>(
        &'a self,
        turn: &'a TurnGate,
        engine: &'a PermissionEngine,
    ) -> PermissionContext<'a> {
        PermissionContext {
            session_id: &self.session_id,
            tool_policy: self.tool_policy.as_ref(),
            engine,
            permission_override: turn.permission_override,
            plugin: turn.origin.plugin(),
            plugin_approval: (turn.origin.plugin())
                .and_then(|plugin| {
                    self.sessions
                        .get_session(&self.session_id)
                        .map(|s| s.plugin_approval(plugin))
                })
                .unwrap_or_default(),
            patterns: (self.whitelists_dir.as_deref()).map(|dir| (dir, self.workspace.as_path())),
            slot: Some(&self.slot),
            hooks: self.hooks.as_ref(),
            mode: "",
            modes: &self.no_modes,
            mcp_read_only: &self.no_mcp,
            prompt: turn.is_interactive.then_some(Prompt {
                slot: &self.slot,
                event_tx: &self.event_tx,
            }),
        }
    }

    /// Decide one call of the Crucible tool `tool` that the agent makes
    /// through the in-process MCP server. `Err` holds why it is refused;
    /// the agent reads it as the result of the call.
    ///
    /// The call is Crucible's own tool, so it is decided as a call of the
    /// daemon's own agent: the same canonical call, the same diffs.
    async fn decide_crucible_call(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<(), String> {
        let Some(turn) = self.slot.turn_gate() else {
            return Err(format!(
                "Tool '{tool}' is refused: no turn of this session runs, so nobody can decide the call"
            ));
        };
        let mut call = CanonicalToolCall {
            diffs: crate::tools::diff_synth::synthesize_diffs(tool, &args),
            ..CanonicalToolCall::crucible_tool(tool, &args)
        };
        super::tool_hooks::render_call(
            self.hooks.as_ref(),
            &self.session_id,
            &mut call,
            &args,
            &turn.origin,
        )
        .await;
        let engine = self.rules.engine(&self.session_id);
        match decide_permission(&self.context(&turn, &engine), &call, &args).await {
            Decision::Allow(_) | Decision::UserAllowed => Ok(()),
            Decision::Deny(reason) => Err(reason),
            Decision::NoAnswer => Err("The permission prompt ended with no answer".to_string()),
        }
    }

    /// Answer one `session/request_permission` with the one tool policy.
    ///
    /// `call` is the canonical call that the ACP client joined from the
    /// request and the earlier frames of its `toolCallId`. A call the agent
    /// never asks about is the agent's own decision: it runs its own tools,
    /// so a refusal after the fact stops nothing.
    ///
    /// The answer is only an option of the agent: the protocol has no field
    /// for a reason. So the user gets what the agent cannot carry. The layer
    /// that allowed the call goes on the card, as it does for Crucible's own
    /// tools, and the reason of a denial becomes the error of the call.
    async fn decide(
        &self,
        mut call: CanonicalToolCall,
        options: &[agent_client_protocol::schema::v1::PermissionOption],
    ) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
        // No turn runs, so no turn state can answer the request.
        let Some(turn) = self.slot.turn_gate() else {
            return agent_client_protocol::schema::v1::RequestPermissionOutcome::Cancelled;
        };
        // The agent asks before it calls Crucible's MCP server. The server
        // decides that call on its real name when the agent makes it, so a
        // prompt here is a second prompt for one call. An agent that sends a
        // false name skips only this question: the agent may make any call
        // without a question, and the server still decides a call to it.
        if self.mcp_server_decides.load(Ordering::SeqCst)
            && call.raw.is_some()
            && call.runs_in_crucible()
        {
            return select_option(options, true);
        }
        let args = (call.raw.as_ref())
            .and_then(|raw| raw.raw_input.clone())
            .unwrap_or(serde_json::Value::Null);
        super::tool_hooks::render_call(
            self.hooks.as_ref(),
            &self.session_id,
            &mut call,
            &args,
            &turn.origin,
        )
        .await;
        let engine = self.rules.engine(&self.session_id);
        let decision = decide_permission(&self.context(&turn, &engine), &call, &args).await;
        let id = (call.raw.as_ref()).and_then(|raw| raw.tool_call_id.clone());
        match (&decision, id) {
            (super::gate_decision::Decision::Allow(Some(layer)), Some(id)) => {
                emit_event(
                    &self.event_tx,
                    SessionEventMessage::tool_call_update(
                        &self.session_id,
                        id,
                        call,
                        Some(layer.clone()),
                    ),
                );
            }
            (super::gate_decision::Decision::Deny(reason), Some(id)) => {
                self.slot.note_denial(&id, reason.clone());
            }
            _ => {}
        }
        match decision {
            // The turn ended before the user answered. A reject would say
            // that the user refused the call.
            super::gate_decision::Decision::NoAnswer => {
                agent_client_protocol::schema::v1::RequestPermissionOutcome::Cancelled
            }
            decision => select_option(options, decision.allowed()),
        }
    }
}

/// Put one prompt to the user and wait for the answer.
///
/// The one prompt of every gate: register the prompt in the session, emit
/// `interaction_requested`, wait, and clean up.
///
/// One prompt at a time for each session. An ACP agent asks about a
/// parallel tool batch with concurrent calls. If all of them emit at once,
/// the prompts pile up in the TUI queue, and the user sees them as a batch
/// and not as one prompt for each tool. The session lock is held across the
/// whole wait, so caller N+1 emits only after caller N has its answer.
///
/// A queued caller therefore waits while an earlier prompt waits. A prompt
/// waits without a limit, because a client that attaches later shows it at
/// once. A cancel of the turn drops every pending prompt of the session
/// (`AgentManager::cancel`), which ends the wait at once with no answer:
/// `None`.
pub(in crate::agent_manager) async fn prompt_user(
    slot: &crate::agent_manager::slot::SessionSlot,
    session_id: &str,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    request: PermRequest,
) -> Option<PermResponse> {
    let _one_at_a_time = slot.prompt_lock().await;
    let interaction = InteractionRequest::Permission(request.clone());
    let (permission_id, response_rx) = slot.register_permission(request);
    let mut open = OpenPrompt {
        slot,
        session_id,
        event_tx,
        id: &permission_id,
        answered: false,
    };
    if !emit_event(
        event_tx,
        SessionEventMessage::interaction_requested(session_id, &permission_id, &interaction),
    ) {
        debug!(session_id = %session_id, "no subscribers for the permission prompt");
    }

    let response = response_rx.await.ok()?;
    open.answered = true;
    Some(response)
}

/// A prompt that waits for an answer.
///
/// A prompt can end with no answer: the turn is cancelled, or the caller
/// drops the wait (the ACP client drops it when the turn ends). Then the drop removes the prompt from the session, and tells each
/// client to remove it. Without this the web Inbox listed a prompt that
/// nobody waited for.
struct OpenPrompt<'a> {
    slot: &'a crate::agent_manager::slot::SessionSlot,
    session_id: &'a str,
    event_tx: &'a broadcast::Sender<SessionEventMessage>,
    id: &'a str,
    answered: bool,
}

impl Drop for OpenPrompt<'_> {
    fn drop(&mut self) {
        if self.answered {
            return;
        }
        self.slot.take_permission(self.id);
        emit_event(
            self.event_tx,
            SessionEventMessage::interaction_completed(
                self.session_id,
                self.id,
                crucible_core::interaction::InteractionResponse::Cancelled,
            ),
        );
    }
}

impl AgentManager {
    /// The permission gate of an ACP session.
    pub(in crate::agent_manager) fn build_acp_permissions(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        workspace: &std::path::Path,
        tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    ) -> AcpPermissions {
        AcpPermissions {
            gate: Arc::new(AcpGate {
                slot: self.slot(session_id),
                session_id: session_id.to_string(),
                event_tx: event_tx.clone(),
                workspace: workspace.to_path_buf(),
                whitelists_dir: self.whitelists_dir(),
                hooks: self.plugin_handlers(),
                rules: self.session_rules(),
                tool_policy,
                mcp_server_decides: AtomicBool::new(false),
                no_modes: crucible_lua::ModeRegistry::new(),
                no_mcp: std::collections::HashSet::new(),
                sessions: self.session_manager.clone(),
            }),
        }
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
        if !stream_ctx.attachment_messages.is_empty() {
            current = (stream_ctx.attachment_messages.iter().cloned())
                .chain(current)
                .collect();
        }

        if let Some(ref precog_msg) = stream_ctx.precognition_message {
            let mut with_precog = Vec::with_capacity(current.len() + 1);
            with_precog.push(precog_msg.clone());
            with_precog.extend(current);
            current = with_precog;
        }
        // The blocks so far are the daemon's own. Only a block that a
        // handler adds gets the envelope of handler context.
        let prior = current.clone();

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

        crucible_core::turn::tag_new_system_messages(&prior, &mut current);

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
            Some(PatternKind::Bash(command)) => pattern_store.matches_bash(&command),
            // A grant for one path must not permit an edit of another.
            Some(PatternKind::File) => call.paths.iter().all(|p| pattern_store.matches_file(p)),
            Some(PatternKind::Tool) => pattern_store.matches_tool(&call.tool),
            None => false,
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

        if pattern_kind(call).is_none() {
            tracing::info!(tool = %call.tool, "No grant is stored for a command that Crucible cannot read");
            return Ok(());
        }
        let mut store = PatternStore::load_file(file).unwrap_or_default();
        Self::add_pattern(&mut store, call, pattern)?;
        store.save_file(file)?;
        Ok(())
    }

    /// Add `pattern` to `store`, in the table that [`pattern_kind`] gives
    /// `call`. A call with no table gets no grant.
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants
    )]
    pub(in crate::agent_manager) fn add_pattern(
        store: &mut PatternStore,
        call: &CanonicalToolCall,
        pattern: &str,
    ) -> Result<(), crucible_core::config::PatternError> {
        match pattern_kind(call) {
            Some(PatternKind::Bash(_)) => store.add_bash_pattern(pattern),
            Some(PatternKind::File) => store.add_file_pattern(pattern),
            Some(PatternKind::Tool) => store.add_tool_pattern(pattern),
            None => Ok(()),
        }
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
        registry: Option<&super::super::PluginHandlers>,
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
            // A tool name that an agent sends is not Crucible's tool.
            is_safe: call.runs_in_crucible()
                && crate::agent_manager::believed_read_only(tool_name, mcp_read_only),
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

impl StreamContext {
    /// What the tool gate reads from this turn.
    pub(super) fn permission_context(&self) -> PermissionContext<'_> {
        let config = &self.agent_stream_config;
        PermissionContext {
            session_id: &self.session_id,
            tool_policy: config.tool_policy.as_ref(),
            engine: &self.permission_engine,
            permission_override: self.permission_override,
            plugin: self.origin.plugin(),
            plugin_approval: self
                .origin
                .plugin()
                .and_then(|plugin| {
                    self.session_manager
                        .get_session(&self.session_id)
                        .map(|s| s.plugin_approval(plugin))
                })
                .unwrap_or_default(),
            patterns: (self.whitelists_dir.as_deref())
                .map(|dir| (dir, self.workspace_path.as_path())),
            slot: Some(&self.slot),
            hooks: config.plugin_handlers.as_ref(),
            mode: &self.session_mode,
            modes: &config.modes,
            mcp_read_only: &config.mcp_read_only_tools,
            prompt: self.is_interactive.then_some(Prompt {
                slot: &self.slot,
                event_tx: &self.event_tx,
            }),
        }
    }
}

#[cfg(test)]
mod acp_permission_tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome,
    };

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
        assert_eq!(
            selected(select_option(&options, true)).as_deref(),
            Some("once")
        );
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
            select_option(&options, true),
            RequestPermissionOutcome::Cancelled
        ));
    }

    /// A denial never takes `reject_always`. The agent stores that option
    /// as a deny rule that the user never chose, and then it stops asking
    /// about the tool. With no `reject_once`, the outcome is `Cancelled`.
    #[test]
    fn a_denial_never_takes_reject_always() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("never", PermissionOptionKind::RejectAlways),
        ];
        assert!(matches!(
            select_option(&options, false),
            RequestPermissionOutcome::Cancelled
        ));
    }

    /// With no option of a usable kind, the outcome is `Cancelled`.
    #[test]
    fn no_usable_option_is_cancelled() {
        assert!(matches!(
            select_option(&[], false),
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
    use crucible_core::agent::ToolPolicy;
    use crucible_core::config::components::permissions::PermissionConfig;
    use crucible_core::types::{classify_acp, CanonicalToolCall, RawToolCall};

    /// One decided permission request, and whether the user was asked.
    struct Asked {
        outcome: RequestPermissionOutcome,
        prompts: usize,
    }

    impl Asked {
        /// Whether the gate selected exactly `allow-once`. A prefix match
        /// also took `allow-always`, which the gate must never send.
        fn allowed(&self) -> bool {
            selected_id(&self.outcome).as_deref() == Some("allow-once")
        }
    }

    fn selected_id(outcome: &RequestPermissionOutcome) -> Option<String> {
        match outcome {
            RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.to_string()),
            _ => None,
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
        // The ACP client hands the permission path the canonical call.
        let call = crucible_core::types::classify_acp(
            crucible_core::types::RawToolCall::from(&request.tool_call),
            &[],
        );
        decide_call(call, card, config).await
    }

    /// Decide `call` through the ACP gate of an interactive session whose
    /// user answers each prompt with allow.
    async fn decide_call(
        call: CanonicalToolCall,
        card: &[(&str, ToolPolicy)],
        config: Option<PermissionConfig>,
    ) -> Asked {
        decide_with_hooks(call, card, config, None).await
    }

    async fn decide_with_hooks(
        call: CanonicalToolCall,
        card: &[(&str, ToolPolicy)],
        config: Option<PermissionConfig>,
        hooks: Option<PluginHandlers>,
    ) -> Asked {
        let (event_tx, mut events) = broadcast::channel::<SessionEventMessage>(16);
        let slot = Arc::new(crate::agent_manager::slot::SessionSlot::default());
        slot.set_turn_gate(crate::agent_manager::slot::TurnGate {
            is_interactive: true,
            ..Default::default()
        });
        let user = slot.clone();
        let answers = tokio::spawn(async move {
            let mut prompts = 0;
            while let Ok(msg) = events.recv().await {
                if let Some(id) = msg.data["request_id"].as_str() {
                    prompts += 1;
                    if let Some(pending) = user.take_permission(id) {
                        let _ = pending.response_tx.send(PermResponse::allow());
                    }
                }
            }
            prompts
        });
        let gate = AcpGate {
            slot,
            session_id: "sess-1".to_string(),
            event_tx,
            workspace: PathBuf::new(),
            whitelists_dir: None,
            hooks,
            rules: crate::agent_manager::session_permissions::SessionRules::global(config),
            mcp_server_decides: AtomicBool::new(false),
            no_modes: crucible_lua::ModeRegistry::new(),
            no_mcp: std::collections::HashSet::new(),
            sessions: crate::test_support::temp_session_manager(),
            tool_policy: Some(
                card.iter()
                    .map(|(name, policy)| ((*name).to_string(), *policy))
                    .collect(),
            ),
        };
        let options: Vec<agent_client_protocol::schema::v1::PermissionOption> =
            serde_json::from_value(options()).expect("the options parse");
        let outcome = gate.decide(call, &options).await;
        // The gate holds the only sender, so the answering task ends.
        drop(gate);
        Asked {
            outcome,
            prompts: answers.await.expect("join"),
        }
    }

    /// Ask about each call in turn, in one session whose user answers each
    /// prompt with "always allow" and the grant that the prompt suggests.
    /// Whether each call was put to the user.
    async fn asked_with_always_allow(calls: Vec<CanonicalToolCall>) -> Vec<bool> {
        asked_with_grants(calls, crucible_core::interaction::PermissionScope::User).await
    }

    /// [`asked_with_always_allow`] with grants of `scope`.
    async fn asked_with_grants(
        calls: Vec<CanonicalToolCall>,
        scope: crucible_core::interaction::PermissionScope,
    ) -> Vec<bool> {
        use crucible_core::interaction::InteractionRequest;
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let whitelists = tempfile::TempDir::new().unwrap();
        let (event_tx, mut events) = broadcast::channel::<SessionEventMessage>(16);
        let slot = Arc::new(crate::agent_manager::slot::SessionSlot::default());
        slot.set_turn_gate(crate::agent_manager::slot::TurnGate {
            is_interactive: true,
            ..Default::default()
        });
        let prompts = Arc::new(AtomicUsize::new(0));
        let (user, counted) = (slot.clone(), prompts.clone());
        tokio::spawn(async move {
            while let Ok(msg) = events.recv().await {
                let Some(id) = msg.data["request_id"].as_str() else {
                    continue;
                };
                let Ok(InteractionRequest::Permission(request)) =
                    serde_json::from_value(msg.data["request"].clone())
                else {
                    continue;
                };
                counted.fetch_add(1, SeqCst);
                let answer = request
                    .suggested_pattern()
                    .map_or_else(PermResponse::allow, |p| {
                        PermResponse::allow_pattern(p, scope)
                    });
                if let Some(pending) = user.take_permission(id) {
                    let _ = pending.response_tx.send(answer);
                }
            }
        });
        let gate = AcpGate {
            slot,
            session_id: "sess-1".to_string(),
            event_tx,
            workspace: PathBuf::from("/w"),
            whitelists_dir: Some(whitelists.path().to_path_buf()),
            hooks: None,
            rules: crate::agent_manager::session_permissions::SessionRules::global(None),
            tool_policy: None,
            mcp_server_decides: AtomicBool::new(false),
            no_modes: crucible_lua::ModeRegistry::new(),
            no_mcp: std::collections::HashSet::new(),
            sessions: crate::test_support::temp_session_manager(),
        };
        let options: Vec<agent_client_protocol::schema::v1::PermissionOption> =
            serde_json::from_value(options()).expect("the options parse");
        let mut asked = Vec::new();
        for call in calls {
            let before = prompts.load(SeqCst);
            let outcome = gate.decide(call, &options).await;
            // The user said "always allow", and a saved grant answers the
            // later calls. Crucible keeps the grant, so the agent still gets
            // `allow-once`: with `allow-always` it stops asking.
            assert_eq!(selected_id(&outcome).as_deref(), Some("allow-once"));
            asked.push(prompts.load(SeqCst) > before);
        }
        asked
    }

    /// No layer that allows a call sends `allow-always`, also when the
    /// agent offers it: a card, an operator rule, and a user who allowed
    /// once. The saved pattern is checked in
    /// `always_allow_answers_the_same_call_and_no_other`.
    #[tokio::test]
    async fn no_allowing_layer_sends_allow_always() {
        let call = || {
            classify_acp(
                RawToolCall::from(&claude_request("mcp__github__create_pr").tool_call),
                &[],
            )
        };
        let rule = PermissionConfig {
            allow: vec!["mcp__github__create_pr:*".to_string()],
            ..Default::default()
        };
        for (layer, asked) in [
            (
                "card",
                decide_call(
                    call(),
                    &[("mcp__github__create_pr", ToolPolicy::Allow)],
                    None,
                )
                .await,
            ),
            ("rule", decide_call(call(), &[], Some(rule)).await),
            ("user", decide_call(call(), &[], None).await),
        ] {
            assert_eq!(
                selected_id(&asked.outcome).as_deref(),
                Some("allow-once"),
                "{layer}"
            );
        }
    }

    /// One "always allow" answers the same call next time, and never a
    /// different tool. A call that nothing names gets no grant, because a
    /// grant for its kind would answer each other unnamed call.
    #[tokio::test]
    async fn always_allow_answers_the_same_call_and_no_other() {
        let named =
            |name: &str| classify_acp(RawToolCall::from(&claude_request(name).tool_call), &[]);
        let edit = |path: &str| {
            classify_acp(
                serde_json::from_value(serde_json::json!({
                    "toolCallId": "toolu_02",
                    "name": "Edit",
                    "kind": "edit",
                    "content": [{ "type": "diff", "path": path, "oldText": "a", "newText": "b" }],
                }))
                .expect("a raw tool call"),
                &[],
            )
        };
        let cases = [
            (
                "an MCP tool",
                vec![
                    named("mcp__github__create_pr"),
                    named("mcp__github__create_pr"),
                    named("mcp__github__delete_repo"),
                ],
                vec![true, false, true],
            ),
            (
                "a Claude Edit",
                vec![edit("/w/a.rs"), edit("/w/a.rs"), edit("/w/b.rs")],
                vec![true, false, true],
            ),
            (
                "an unnamed call",
                vec![unnamed("other", "Do A"), unnamed("other", "Do B")],
                vec![true, true],
            ),
            (
                "an unnamed read",
                vec![unnamed("read", "Read A"), unnamed("read", "Read B")],
                vec![true, true],
            ),
        ];
        for (what, calls, expected) in cases {
            assert_eq!(asked_with_always_allow(calls).await, expected, "{what}");
        }
    }

    /// "Allow for this session" answers the same call again in the session,
    /// and no other call. Before, a session grant was stored nowhere, so the
    /// user was asked again at the next identical call.
    #[tokio::test]
    async fn a_session_grant_answers_the_same_call_in_the_session() {
        let named =
            |name: &str| classify_acp(RawToolCall::from(&claude_request(name).tool_call), &[]);
        assert_eq!(
            asked_with_grants(
                vec![
                    named("mcp__github__create_pr"),
                    named("mcp__github__create_pr"),
                    named("mcp__github__delete_repo"),
                ],
                crucible_core::interaction::PermissionScope::Session,
            )
            .await,
            [true, false, true]
        );
    }

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
        let call = unnamed("edit", "Edit src/main.rs");
        assert_eq!(call.tool, "file_edit", "the prose title is never the name");
        let asked = decide_call(call, &[], Some(config)).await;

        assert!(
            !asked.allowed(),
            "deny = [\"file_edit:*\"] must reach an ACP edit"
        );
    }

    /// …and an ACP `kind` never buys the read-only exemption, because the
    /// agent supplies it. Same reasoning `is_safe` gives for `readOnlyHint`.
    #[tokio::test]
    async fn a_read_kind_does_not_skip_the_prompt() {
        let asked = decide_call(unnamed("read", "Read /etc/passwd"), &[], None).await;

        assert_eq!(
            asked.prompts, 1,
            "an agent-supplied kind must not widen the gate"
        );
    }

    /// An agent's own tool can have the name of a read-only Crucible tool.
    /// Gemini names its read tool `read_file` in the id. The agent runs it,
    /// so it does not take the read-only exemption of Crucible's tool.
    #[tokio::test]
    async fn an_agent_tool_with_a_crucible_name_does_not_skip_the_prompt() {
        let gemini: Vec<crucible_core::types::AgentKeys> =
            serde_json::from_value(serde_json::json!([{ "id": "^(?P<tool>[a-z_]+?)__" }]))
                .expect("the table parses");
        let raw = serde_json::from_value(serde_json::json!({
            "toolCallId": "read_file__read_file_1_2",
            "kind": "read",
            "locations": [{"path": "/etc/passwd"}],
        }))
        .expect("a raw tool call");
        let call = classify_acp(raw, &gemini);
        assert_eq!(call.tool, "read_file");
        let asked = decide_call(call, &[], None).await;
        assert_eq!(asked.prompts, 1, "the agent's read_file is asked about");
    }

    /// A hook reads `request.is_safe` as "Crucible knows this tool only
    /// reads". A name that an agent or a third-party MCP server sends does
    /// not make that true. Codex names a third-party MCP tool by its server,
    /// and gemini names its own tool in the id.
    #[tokio::test]
    async fn a_hook_does_not_see_an_agent_tool_as_safe() {
        let loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("daemon VM");
        loader
            .executor()
            .lua()
            .load("cru.permissions.on_request(function(r) if r.is_safe then return { allow = true } end end)")
            .exec()
            .unwrap();
        let codex: Vec<crucible_core::types::AgentKeys> =
            serde_json::from_value(serde_json::json!([{
                "title": "^Tool: ",
                "args": ["/rawInput/arguments"],
                "tool": ["/rawInput/tool"],
                "server": ["/rawInput/server"],
            }]))
            .expect("the table parses");
        let gemini: Vec<crucible_core::types::AgentKeys> =
            serde_json::from_value(serde_json::json!([{ "id": "^(?P<tool>[a-z_]+?)__" }]))
                .expect("the table parses");
        for tool in ["read_file", "grep", "list_notes"] {
            let codex_call = classify_acp(
                serde_json::from_value(serde_json::json!({
                    "toolCallId": "call-1",
                    "title": format!("Tool: github/{tool}"),
                    "kind": "execute",
                    "rawInput": { "server": "github", "tool": tool, "arguments": {} },
                }))
                .expect("a raw tool call"),
                &codex,
            );
            let gemini_call = classify_acp(
                serde_json::from_value(serde_json::json!({
                    "toolCallId": format!("{tool}__{tool}_1_2"),
                    "kind": "other",
                }))
                .expect("a raw tool call"),
                &gemini,
            );
            assert_eq!(gemini_call.tool, tool);
            for call in [codex_call, gemini_call] {
                let name = call.tool.clone();
                let asked = decide_with_hooks(call, &[], None, Some(loader.handlers())).await;
                assert_eq!(asked.prompts, 1, "{name} is asked about");
            }
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

    /// Hermes asks about a command with a new id, no `rawInput` and the
    /// command only in the title. Crucible cannot read the command line, so
    /// a `bash` deny rule refuses the call, and with no deny rule the user
    /// is asked, also when a `bash` rule allows each command.
    #[tokio::test]
    async fn a_command_that_crucible_cannot_read_is_still_a_command() {
        let hermes = || {
            serde_json::from_value::<RequestPermissionRequest>(serde_json::json!({
                "sessionId": "sess-1",
                "toolCall": {
                    "toolCallId": "perm-check-1",
                    "title": "terminal: rm -rf build",
                    "kind": "execute",
                    "status": "pending",
                },
                "options": options(),
            }))
            .expect("the hermes frame parses")
        };
        let rules = |deny: &str| PermissionConfig {
            default: PermissionMode::Allow,
            allow: vec!["bash:*".to_string()],
            deny: vec![deny.to_string()],
            ..Default::default()
        };

        let denied = decide(hermes(), &[], Some(rules("bash:rm *"))).await;
        assert!(!denied.allowed(), "a bash deny rule must refuse the call");
        assert_eq!(denied.prompts, 0, "an operator deny asks nobody");

        let card = decide(hermes(), &[("bash", ToolPolicy::Deny)], None).await;
        assert!(!card.allowed(), "a card bash deny must refuse the call");

        let asked = decide(hermes(), &[], Some(rules("read:/etc/*"))).await;
        assert_eq!(asked.prompts, 1, "`allow bash:*` cannot allow it");
    }

    /// An ACP edit that names no path is still an edit, as a command with
    /// no command line is still a command. An `edit` deny rule refuses it.
    /// No rule can allow it, so with no deny rule the user is asked, and
    /// "always allow" saves no grant for it.
    #[tokio::test]
    async fn an_edit_that_names_no_path_is_still_an_edit() {
        let edit = |name: Option<&str>| {
            let mut raw = serde_json::json!({ "kind": "edit", "title": "Edit a file" });
            if let Some(name) = name {
                raw["name"] = serde_json::json!(name);
            }
            classify_acp(serde_json::from_value(raw).expect("a raw tool call"), &[])
        };
        let rules = |rule: &str, deny: bool| PermissionConfig {
            default: PermissionMode::Allow,
            allow: (!deny).then(|| rule.to_string()).into_iter().collect(),
            deny: deny.then(|| rule.to_string()).into_iter().collect(),
            ..Default::default()
        };
        for name in [None, Some("Edit")] {
            assert_eq!(edit(name).kind, "file_edit", "{name:?}");
            let denied = decide_call(edit(name), &[], Some(rules("edit:*", true))).await;
            assert!(!denied.allowed(), "an edit deny rule refuses {name:?}");
            assert_eq!(denied.prompts, 0, "an operator deny asks nobody");
            let asked = decide_call(edit(name), &[], Some(rules("edit:*", false))).await;
            assert_eq!(asked.prompts, 1, "no rule can allow {name:?}");
        }
        assert_eq!(
            asked_with_always_allow(vec![edit(Some("Edit")), edit(Some("Edit"))]).await,
            [true, true],
            "no grant covers each edit of Edit"
        );
        // A grant that the user types does not cover it either.
        let mut typed = PatternStore::new();
        typed.add_tool_pattern("Edit").unwrap();
        assert!(!AgentManager::check_pattern_match(
            &edit(Some("Edit")),
            &typed
        ));
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

    /// A `read` rule allows only a tool that Crucible knows reads. A plugin
    /// or MCP gateway tool with a `path` argument can delete or rename, so
    /// the user is asked.
    #[tokio::test]
    async fn a_read_rule_does_not_allow_an_unknown_tool_with_a_path() {
        let reads = || PermissionConfig {
            allow: vec!["read:*".to_string()],
            ..Default::default()
        };
        let args = serde_json::json!({"path": "notes/a.md"});
        for (tool, prompts) in [("delete_file", 1), ("rename_note", 1), ("read_file", 0)] {
            let call = CanonicalToolCall::crucible_tool(tool, &args);
            let asked = decide_call(call, &[], Some(reads())).await;
            assert_eq!(asked.prompts, prompts, "{tool}");
        }
    }
}

/// Tests that call the real handler that `build_acp_permission_handler`
/// returns. The unit tests above cover its parts. These tests cover the
/// order of the parts: the declared policy, then the gate, then the prompt.
#[cfg(test)]
mod acp_permission_handler_tests {
    use super::*;
    use crate::agent_manager::slot::TurnGate;
    use crate::agent_manager::tests::create_test_agent_manager;
    use crate::test_support::temp_session_manager;
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
    use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
    use crucible_core::interaction::PermResponse;
    use crucible_core::session::{PluginApproval, SessionType};
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
        am.slot(SESSION).set_turn_gate(TurnGate {
            is_interactive: true,
            permission_override: None,
            ..Default::default()
        });
        am.build_acp_permissions(SESSION, event_tx, std::path::Path::new("/w"), tool_policy)
            .handler()
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
        // The card of the call names the layer that allowed it (rule 7).
        let update = event_rx.try_recv().expect("the card gets the layer");
        assert_eq!(update.event, "tool_call_update");
        assert_eq!(update.data["call_id"], "call-1");
        assert_eq!(update.data["auto_approved"], "agent card policy");
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
        // The agent gets no reason. The result of the call will carry it.
        let reason = am.slot(SESSION).take_denial("call-1");
        assert!(
            reason
                .as_deref()
                .is_some_and(|r| r.contains("card tool policy")),
            "{reason:?}"
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

    /// A card `bash` entry keys the same way as an operator `bash` rule: it
    /// applies to each `command` call, also one that the agent does not name.
    #[tokio::test]
    async fn a_card_bash_deny_refuses_an_unnamed_acp_command() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let card = ToolPolicyMap::from([("bash".to_string(), ToolPolicy::Deny)]);
        let handle = handler(&am, &event_tx, Some(card));

        let outcome = tokio::time::timeout(Duration::from_secs(5), ask(&handle))
            .await
            .expect("a card deny answers without a prompt");

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(event_rx.try_recv().is_err(), "a card deny asks nobody");
    }

    /// A grant that the user saved answers an ACP request, as it answers a
    /// call of Crucible's own shell.
    #[tokio::test]
    async fn a_saved_pattern_answers_an_acp_request() {
        let config_home = tempfile::TempDir::new().unwrap();
        let am = Arc::new(AgentManager::new(
            crate::agent_manager::AgentManagerParams {
                kiln_manager: Arc::new(crate::kiln_manager::KilnManager::new()),
                session_manager: temp_session_manager(),
                background_manager: Arc::new(crate::background_manager::BackgroundJobManager::new(
                    broadcast::channel(16).0,
                )),
                mcp_gateway: None,
                llm_config: None,
                acp_config: None,
                context_config: None,
                permission_config: None,
                plugin_loader: None,
                source_roots: crate::runtime_path::SourceRoots {
                    config_home: Some(config_home.path().to_path_buf()),
                    agent_directories: Vec::new(),
                    runtimepath: Vec::new(),
                    plugin_dirs: Default::default(),
                    kiln_registry: None,
                },
                review_snapshot_root: crate::test_support::scratch_snapshot_root(),
            },
        ));
        let dir = am
            .whitelists_dir()
            .expect("a config home gives a whitelist");
        let file = PatternStore::store_file_in(
            &dir,
            crucible_core::interaction::PermissionScope::User,
            "",
        )
        .expect("a user grant has a file");
        let bash = serde_json::json!({"command": "cargo test"});
        AgentManager::store_pattern_to(
            &file,
            &CanonicalToolCall::crucible_tool("bash", &bash),
            "cargo test",
        )
        .expect("the grant is stored");
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let outcome = tokio::time::timeout(Duration::from_secs(5), ask(&handle))
            .await
            .expect("a saved grant answers without a prompt");

        assert_eq!(selected(&outcome).as_deref(), Some("allow_once"));
        let update = event_rx.try_recv().expect("the card gets the layer");
        assert_eq!(update.data["auto_approved"], "saved pattern");
        assert!(event_rx.try_recv().is_err(), "a saved grant asks nobody");
    }

    /// A cached handle keeps no interactivity of the turn that built it. A
    /// later turn that nobody can answer (a workflow step) is not asked.
    #[tokio::test]
    async fn a_later_non_interactive_turn_is_not_asked() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);
        am.slot(SESSION).set_turn_gate(TurnGate {
            is_interactive: false,
            permission_override: None,
            ..Default::default()
        });

        let outcome = tokio::time::timeout(Duration::from_secs(5), ask(&handle))
            .await
            .expect("a turn with nobody to ask must not wait for an answer");

        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(event_rx.try_recv().is_err(), "nobody is asked");
    }

    /// A changed permission override applies on the next call of the same
    /// cached handle.
    #[tokio::test]
    async fn a_changed_override_applies_on_the_next_call() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, _event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        for (mode, expected) in [
            (PermissionMode::Allow, "allow_once"),
            (PermissionMode::Deny, "reject_once"),
        ] {
            am.slot(SESSION).set_turn_gate(TurnGate {
                is_interactive: true,
                permission_override: Some(mode),
                ..Default::default()
            });
            let outcome = tokio::time::timeout(Duration::from_secs(5), ask(&handle))
                .await
                .expect("an override answers without a prompt");
            assert_eq!(selected(&outcome).as_deref(), Some(expected), "{mode:?}");
        }
    }

    /// The prompt shows everything that is known: the render, the agent, the
    /// raw tool name, the diff and the layer that asked.
    #[tokio::test]
    async fn the_prompt_shows_the_agent_the_raw_name_the_diff_and_the_layer() {
        use crucible_core::interaction::InteractionRequest;
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);
        am.slot(SESSION).set_turn_gate(TurnGate {
            is_interactive: true,
            ..Default::default()
        });
        let request: RequestPermissionRequest = serde_json::from_value(serde_json::json!({
            "sessionId": "acp-wire-session",
            "toolCall": {
                "toolCallId": "call-1",
                "title": "Edit a.rs",
                "kind": "edit",
                "name": "Edit",
                "content": [{ "type": "diff", "path": "/w/a.rs", "oldText": "a", "newText": "b" }],
            },
            "options": [{ "optionId": "reject_once", "name": "No", "kind": "reject_once" }],
        }))
        .unwrap();
        let mut call = crucible_core::types::classify_acp(
            crucible_core::types::RawToolCall::from(&request.tool_call),
            &[],
        );
        call.agent = Some("claude".to_string());

        let pending = tokio::spawn(handle(call, request.options));
        let event = loop {
            let msg = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
                .await
                .expect("the handler must prompt")
                .expect("event channel open");
            if msg.event == "interaction_requested" {
                break msg;
            }
        };
        let InteractionRequest::Permission(asked) =
            serde_json::from_value(event.data["request"].clone()).expect("a permission request")
        else {
            panic!("the prompt must be a permission request");
        };
        let call = asked.call.expect("the prompt carries the call");
        assert_eq!(call.agent.as_deref(), Some("claude"));
        assert_eq!(call.raw.and_then(|raw| raw.name).as_deref(), Some("Edit"));
        assert!(call.render.is_some(), "the prompt carries the render");
        assert!(call.diffs.is_empty(), "the request holds the diff once");
        assert_eq!(asked.diffs[0].path, "/w/a.rs");
        assert_eq!(asked.layer.as_deref(), Some("agent"));

        let id = event.data["request_id"].as_str().unwrap().to_string();
        am.respond_to_permission(SESSION, &id, PermResponse::deny())
            .unwrap();
        pending.await.unwrap();
    }

    /// A cancel of the turn answers the waiting prompt at once with
    /// `cancelled`, not with a reject: nobody refused the call. The prompt
    /// leaves the registry.
    #[tokio::test]
    async fn a_cancel_answers_the_waiting_prompt() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let pending = tokio::spawn(ask(&handle));
        let _id = prompt_id(&mut event_rx).await;
        am.cancel(SESSION).await;

        let outcome = tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .expect("the cancel must answer the prompt")
            .expect("join");
        assert!(
            matches!(outcome, RequestPermissionOutcome::Cancelled),
            "{outcome:?}"
        );
        assert!(am.list_all_pending_permissions().is_empty());
    }

    /// Read events until a prompt ends. Return its request id.
    async fn ended_id(rx: &mut broadcast::Receiver<SessionEventMessage>) -> String {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("the prompt must end with interaction_completed")
                .expect("event channel open");
            if msg.event == "interaction_completed" {
                return msg.data["request_id"]
                    .as_str()
                    .expect("request_id")
                    .to_string();
            }
        }
    }

    /// A prompt that ends with no answer leaves the registry (the web Inbox
    /// lists it), and each client gets `interaction_completed` to remove it.
    /// A cancel of the turn ends a prompt, and so does a caller that drops
    /// the wait: the ACP client drops it when its turn ends.
    #[tokio::test]
    async fn an_abandoned_prompt_leaves_the_registry_and_the_clients() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let pending = tokio::spawn(ask(&handle));
        let id = prompt_id(&mut event_rx).await;
        am.cancel(SESSION).await;
        pending.await.expect("join");
        assert!(am.list_all_pending_permissions().is_empty());
        assert_eq!(
            ended_id(&mut event_rx).await,
            id,
            "a cancel ends the prompt"
        );

        let pending = tokio::spawn(ask(&handle));
        let id = prompt_id(&mut event_rx).await;
        pending.abort();
        let _ = pending.await;
        assert!(
            am.list_all_pending_permissions().is_empty(),
            "a dropped wait leaves no prompt"
        );
        assert_eq!(
            ended_id(&mut event_rx).await,
            id,
            "a dropped wait ends the prompt"
        );
    }

    #[tokio::test]
    async fn plugin_ask_prompts_an_acp_call_that_the_override_would_allow() {
        let sessions = temp_session_manager();
        let session = sessions
            .create_session(
                SessionType::Chat,
                vec![crate::test_support::kiln_name("kiln")],
                None,
                None,
            )
            .await
            .unwrap();
        let session_id = session.id.to_string();
        let am = create_test_agent_manager(sessions);
        am.set_plugin_approval(&session_id, "alpha", PluginApproval::Ask, None)
            .await
            .unwrap();
        am.slot(&session_id).set_turn_gate(TurnGate {
            is_interactive: true,
            permission_override: Some(PermissionMode::Allow),
            origin: crucible_core::turn::TurnOrigin::Plugin("alpha".into()),
        });
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = am
            .build_acp_permissions(&session_id, &event_tx, std::path::Path::new("/w"), None)
            .handler();

        let pending = tokio::spawn(ask(&handle));
        let id = prompt_id(&mut event_rx).await;
        let prompts = am.list_all_pending_permissions();
        assert_eq!(prompts.len(), 1);
        assert_eq!(
            prompts[0].2.origin,
            Some(crucible_core::turn::TurnOrigin::Plugin("alpha".into()))
        );
        am.respond_to_permission(&session_id, &id, PermResponse::allow())
            .unwrap();
        assert_eq!(
            selected(&pending.await.unwrap()).as_deref(),
            Some("allow_once")
        );
    }

    /// Nobody answers. The prompt waits without a limit: after five minutes
    /// it is still open, and the user can still answer it.
    #[tokio::test(start_paused = true)]
    async fn an_unanswered_prompt_waits_past_five_minutes() {
        let am = create_test_agent_manager(temp_session_manager());
        let (event_tx, mut event_rx) = broadcast::channel(16);
        let handle = handler(&am, &event_tx, None);

        let pending = tokio::spawn(ask(&handle));
        let id = prompt_id(&mut event_rx).await;
        tokio::time::advance(Duration::from_secs(301)).await;
        tokio::task::yield_now().await;
        assert!(
            !pending.is_finished(),
            "the user may answer after five minutes"
        );
        assert_eq!(am.list_all_pending_permissions().len(), 1);

        am.respond_to_permission(SESSION, &id, PermResponse::allow())
            .unwrap();
        assert_eq!(
            selected(&pending.await.unwrap()).as_deref(),
            Some("allow_once")
        );
        assert!(am.list_all_pending_permissions().is_empty());
    }
}
