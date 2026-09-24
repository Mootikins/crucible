use super::super::*;
use crucible_core::config::components::permissions::{
    PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::types::CanonicalToolCall;
use crucible_lua::StageId;
use std::ops::ControlFlow;

use super::gate_decision::{decide_permission, PermissionContext, Prompt};
use crate::agent_manager::vm_pass::run_handlers;

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
    allowed: bool,
) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
    use agent_client_protocol::schema::v1::{
        PermissionOptionKind as Kind, RequestPermissionOutcome, SelectedPermissionOutcome,
    };

    let acceptable: &[Kind] = if allowed {
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

/// The prompt for an ACP call.
///
/// A command asks as a `Bash` request. The prompt then shows the command
/// line, and "always allow" offers the command line as the pattern, which is
/// the pattern that the saved-pattern check reads for a `command` call.
pub(in crate::agent_manager) fn acp_prompt_request(
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> PermRequest {
    let request = match (call.kind.as_str(), &call.command) {
        ("command", Some(command)) => PermRequest::bash([command]),
        _ => PermRequest::tool(&call.tool, args.clone()),
    };
    request.with_diffs(call.diffs.clone())
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
    hooks: Option<DaemonPermissions>,
    modes: crucible_lua::ModeRegistry,
    engine: PermissionEngine,
    tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
}

impl AcpGate {
    /// Answer one `session/request_permission` with the one tool policy.
    ///
    /// `call` is the canonical call that the ACP client joined from the
    /// request and the earlier frames of its `toolCallId`. A call the agent
    /// never asks about is the agent's own decision: it runs its own tools,
    /// so a refusal after the fact stops nothing.
    ///
    /// The answer is only an option of the agent. A deny reason reaches the
    /// user, not the agent: the protocol has no field for it.
    async fn decide(
        &self,
        mut call: CanonicalToolCall,
        options: &[agent_client_protocol::schema::v1::PermissionOption],
    ) -> agent_client_protocol::schema::v1::RequestPermissionOutcome {
        let turn = self.slot.turn_gate();
        let args = (call.raw.as_ref())
            .and_then(|raw| raw.raw_input.clone())
            .unwrap_or(serde_json::Value::Null);
        super::tool_hooks::render_call(
            self.hooks.as_ref(),
            &self.session_id,
            &mut call,
            &args,
            turn.origin,
        )
        .await;
        let no_mcp = std::collections::HashSet::new();
        let ctx = PermissionContext {
            session_id: &self.session_id,
            tool_policy: self.tool_policy.as_ref(),
            engine: &self.engine,
            permission_override: turn.permission_override,
            patterns: (self.whitelists_dir.as_deref()).map(|dir| (dir, self.workspace.as_path())),
            hooks: self.hooks.as_ref(),
            mode: &turn.mode,
            modes: &self.modes,
            mcp_read_only: &no_mcp,
            prompt: turn.is_interactive.then_some(Prompt {
                slot: &self.slot,
                event_tx: &self.event_tx,
            }),
        };
        let decision =
            decide_permission(&ctx, &call, &args, || acp_prompt_request(&call, &args)).await;
        select_option(options, decision.allowed())
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
/// A queued caller therefore waits while an earlier prompt waits. If the
/// user leaves a prompt until the 300 s timeout, the next prompt appears
/// only after that timeout. A cancel of the turn drops every pending prompt
/// of the session (`AgentManager::cancel`), which answers the wait with a
/// denial at once.
pub(in crate::agent_manager) async fn prompt_user(
    slot: &crate::agent_manager::slot::SessionSlot,
    session_id: &str,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    request: PermRequest,
) -> PermResponse {
    let _one_at_a_time = slot.prompt_lock().await;
    let interaction = InteractionRequest::Permission(request.clone());
    let (permission_id, response_rx) = slot.register_permission(request);
    if !emit_event(
        event_tx,
        SessionEventMessage::interaction_requested(session_id, &permission_id, &interaction),
    ) {
        debug!(session_id = %session_id, "no subscribers for the permission prompt");
    }

    let reason = match tokio::time::timeout(std::time::Duration::from_secs(300), response_rx).await
    {
        Ok(Ok(response)) => return response,
        Ok(Err(_)) => "Permission request channel closed before response",
        Err(_) => "Permission request timed out",
    };
    slot.take_permission(&permission_id);
    debug!(session_id = %session_id, permission_id = %permission_id, reason, "permission prompt ended with no answer");
    PermResponse::deny_with_reason(reason)
}

impl AgentManager {
    /// The `session/request_permission` handler of an ACP session.
    pub(in crate::agent_manager) fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        workspace: &std::path::Path,
        tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    ) -> crate::acp::client::PermissionRequestHandler {
        let gate = Arc::new(AcpGate {
            slot: self.slot(session_id),
            session_id: session_id.to_string(),
            event_tx: event_tx.clone(),
            workspace: workspace.to_path_buf(),
            whitelists_dir: self.whitelists_dir(),
            hooks: self.daemon_permissions(),
            modes: self.modes.clone(),
            engine: self.session_permission_engine(session_id),
            tool_policy,
        });
        Arc::new(move |call, options| {
            let gate = gate.clone();
            Box::pin(async move { gate.decide(call, &options).await })
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

impl StreamContext {
    /// What the tool gate reads from this turn.
    pub(super) fn permission_context(&self) -> PermissionContext<'_> {
        let config = &self.agent_stream_config;
        PermissionContext {
            session_id: &self.session_id,
            tool_policy: config.tool_policy.as_ref(),
            engine: &self.permission_engine,
            permission_override: self.permission_override,
            patterns: (self.whitelists_dir.as_deref())
                .map(|dir| (dir, self.workspace_path.as_path())),
            hooks: config.daemon_permissions.as_ref(),
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

    /// A denial takes `reject_always` when the agent offers no
    /// `reject_once`. `Cancelled` stopped the whole turn instead of one call.
    #[test]
    fn a_reject_once_decision_takes_reject_always_when_the_agent_offers_only_that() {
        let options = [
            option("once", PermissionOptionKind::AllowOnce),
            option("never", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(
            selected(select_option(&options, false)).as_deref(),
            Some("never")
        );
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
    use crucible_core::types::{classify_acp, CanonicalToolCall};

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
            hooks: None,
            modes: crucible_lua::ModeRegistry::new(),
            engine: PermissionEngine::new(config.as_ref()),
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
        am.build_acp_permission_handler(SESSION, event_tx, std::path::Path::new("/w"), tool_policy)
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
                card_roots: crate::agent_cards::CardRoots {
                    config_home: Some(config_home.path().to_path_buf()),
                    agent_directories: Vec::new(),
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
            mode: "ask".to_string(),
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
        assert_eq!(asked.layer.as_deref(), Some("ask mode"));

        let id = event.data["request_id"].as_str().unwrap().to_string();
        am.respond_to_permission(SESSION, &id, PermResponse::deny())
            .unwrap();
        pending.await.unwrap();
    }

    /// A cancel of the turn answers the waiting prompt with a denial at once,
    /// and the prompt leaves the registry.
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
        assert_eq!(selected(&outcome).as_deref(), Some("reject_once"));
        assert!(am.list_all_pending_permissions().is_empty());
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
