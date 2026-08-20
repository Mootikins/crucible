use super::super::*;
use crucible_core::types::acp::FileDiff;
use crucible_core::types::ToolSource;
use crucible_lua::{ToolBeforeExecuteEvent, ToolDisplayCompleteEvent, ToolDisplayStartEvent};

/// Deny a tool call: emit the `tool_result` so views show the outcome, and
/// hand the agent loop an errored result.
///
/// `pre_tool_call` is the enforcement point for gate-style plugins — the
/// isolation *is* the handler taking the call over. Nothing downstream
/// re-checks, so every way a gate can fail to approve must land here rather
/// than falling through to the default executor.
///
/// This guarantee holds for INTERNAL agents only: the daemon dispatches
/// their tools, so a denial here prevents execution. An ACP agent executes
/// tools in its own process and reports them as notifications — a denial
/// arrives after the fact and stops nothing. That is why the session
/// lifecycle refuses to pair an isolation claim with an external agent
/// (`unenforceable_isolation` in session_lifecycle.rs, whose rule is the pure
/// `unenforceable_reason` beside it), and why the review gate degrades to
/// post-turn review for one rather than pretending to block.
fn deny_tool_call(
    stream_ctx: &StreamContext,
    call_id: &str,
    tool_name: &str,
    error_msg: String,
) -> Option<crucible_core::traits::chat::ChatToolResult> {
    if !emit_event(
        &stream_ctx.event_tx,
        SessionEventMessage::tool_result(
            &stream_ctx.session_id,
            call_id,
            tool_name,
            serde_json::json!({ "error": &error_msg }),
        ),
    ) {
        warn!(
            session_id = %stream_ctx.session_id,
            tool = %tool_name,
            "No subscribers for handler denied tool_result event"
        );
    }
    Some(crucible_core::traits::chat::ChatToolResult {
        name: tool_name.to_string(),
        result: String::new(),
        error: Some(error_msg),
        call_id: Some(call_id.to_string()),
        terminate: false,
    })
}

/// Run every `crucible.on("pre_tool_call", …)` handler in one registry.
///
/// `Some` short-circuits the call — a handler cancelled, took it over, or
/// raised. `None` means every handler observed (possibly rewriting `args`),
/// so the caller continues to the next registry (or to real dispatch).
///
/// A `Transform` return of `{ args = {...} }` rewrites the call's arguments
/// in `args`, chained: later handlers (and the other registry) see the
/// rewritten value, and dispatch executes it. Honoured as a *returned* value
/// — never Lua-side mutation of the event table, which would skip this
/// explicit chaining — and the executor's own typed parsing remains the
/// validation boundary, exactly as it is for model-supplied arguments. This
/// was parsed and silently dropped before, so `event.args.command = ...`
/// looked like sanitisation while the original still executed.
///
/// Takes `registry` and `lua` explicitly because handler bodies are
/// `RegistryKey`s valid only against the state that created them: session
/// handlers live in the session VM, plugin handlers in the loader's.
async fn run_pre_tool_call_handlers(
    stream_ctx: &StreamContext,
    registry: &crucible_lua::LuaScriptHandlerRegistry,
    lua: &mlua::Lua,
    tool_name: &str,
    args: &mut serde_json::Value,
    call_id: &str,
) -> Option<crucible_core::traits::chat::ChatToolResult> {
    for handler in registry.runtime_handlers_for("pre_tool_call", Some(tool_name)) {
        let event = SessionEvent::Custom {
            name: "pre_tool_call".to_string(),
            payload: serde_json::json!({
                "tool": tool_name,
                "args": &*args,
            }),
        };
        match registry
            .execute_runtime_handler(lua, &handler.name, &event, Some(&stream_ctx.session_id))
            .await
        {
            Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = %handler.name,
                    reason = %reason,
                    "pre_tool_call handler cancelled"
                );
                return deny_tool_call(
                    stream_ctx,
                    call_id,
                    tool_name,
                    format!("Tool blocked by crucible.on handler: {}", reason),
                );
            }
            Ok(crucible_lua::ScriptHandlerResult::Handled { result, terminate })
                if handler.may_intercept =>
            {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = %handler.name,
                    "pre_tool_call handler provided result"
                );
                let result_string = match result {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                // Events are emitted by the CALLER after the `tool_result`
                // seam runs — a redaction handler must see a plugin-executed
                // result (oci's bash output) the same as a dispatched one,
                // and the emitted events must carry the patched value.
                return Some(crucible_core::traits::chat::ChatToolResult {
                    name: tool_name.to_string(),
                    result: result_string,
                    error: None,
                    call_id: Some(call_id.to_string()),
                    terminate,
                });
            }
            Ok(crucible_lua::ScriptHandlerResult::Transform(val)) if handler.may_intercept => {
                if let Some(new_args) = val.get("args") {
                    if new_args.is_object() {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_name,
                            handler = %handler.name,
                            "pre_tool_call handler rewrote arguments"
                        );
                        *args = new_args.clone();
                    } else {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_name,
                            handler = %handler.name,
                            "pre_tool_call Transform `args` is not an object; ignoring"
                        );
                    }
                }
            }
            // Refused, not honoured: this plugin did not declare
            // `intercept_tools`. `handled` returns before the permission gate
            // and fabricates a result the model reads as the tool's own, and
            // a transform rewrites arguments the gate then approves — both are
            // the authority the container sandbox needs, and neither is
            // something an ordinary plugin should hold by default.
            //
            // The call proceeds normally rather than being denied: a plugin
            // overreaching is not a reason to fail the user's tool call, and
            // `cancel` remains open to every handler because refusing can only
            // narrow.
            Ok(
                crucible_lua::ScriptHandlerResult::Handled { .. }
                | crucible_lua::ScriptHandlerResult::Transform(_),
            ) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = %handler.name,
                    plugin = ?handler.plugin,
                    "pre_tool_call handler tried to take over a tool call without \
                     the `intercept_tools` capability; ignoring and dispatching normally"
                );
            }
            Ok(_) => {}
            // Fail closed — see `deny_tool_call`.
            Err(error) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = %handler.name,
                    error = %error,
                    "pre_tool_call handler error, denying tool (fail-closed)"
                );
                return deny_tool_call(
                    stream_ctx,
                    call_id,
                    tool_name,
                    format!(
                        "Tool denied: pre_tool_call handler error in '{}': {error}",
                        handler.name
                    ),
                );
            }
        }
    }
    None
}

impl AgentManager {
    /// `bracket` is an out-parameter, not a return value, on purpose. The
    /// review capture handle has to be a local in the CALLER's frame: every
    /// early return below then unwinds to the caller's single close site, and
    /// a cancelled or timed-out turn drops the caller's frame and fires the
    /// handle's own `Drop`. Returning it in a tuple would need each of the ten
    /// early returns rewritten and would still not cover cancellation.
    pub(super) async fn handle_tool_call_in_stream(
        stream_ctx: &StreamContext,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        diffs: Vec<FileDiff>,
        bracket: &mut Option<crate::review::CaptureHandle>,
    ) -> Option<crucible_core::traits::chat::ChatToolResult> {
        let call_id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Progressive tool disclosure: an `invoke_tool` call is a bridge for a
        // deferred tool. Unwrap it to the inner tool *before* the PreToolCall
        // reactor event, permission gate, and display events so every
        // downstream consumer sees the real tool name and arguments.
        let was_unwrapped = tool_call.name == "invoke_tool";
        let unwrapped_call;
        let tool_call = if was_unwrapped {
            match Self::unwrap_invoke_tool(&stream_ctx.session_mode, tool_call, &call_id) {
                Ok(inner) => {
                    unwrapped_call = inner;
                    &unwrapped_call
                }
                Err(result) => return Some(result),
            }
        } else {
            tool_call
        };

        let mut args = tool_call
            .arguments
            .clone()
            .unwrap_or(serde_json::Value::Null);

        // Plan mode refuses plugin tools unless the operator named one — their
        // side effects are unknown, so the write-name blocklist cannot classify
        // them, and a plugin's own claim is not evidence. See
        // `tool_modes::plugin_tool_barred` for the two-key rule.
        //
        // Enforced here (not only in the advertised tool set) because the
        // dispatcher always contains them and the mode can change mid-run:
        // before this guard, a session switched to plan kept every plugin
        // tool dispatchable. Before the hook loop, so a plugin cannot
        // "handle" its own tool around the ban.
        if crate::tools::tool_modes::plugin_tool_barred(
            &stream_ctx.session_mode,
            &tool_call.name,
            &stream_ctx.agent_stream_config.plugin_tool_names,
            Some(&stream_ctx.agent_stream_config.modes),
        ) {
            return deny_tool_call(
                stream_ctx,
                &call_id,
                &tool_call.name,
                format!(
                    "Tool '{}' is a plugin tool and not available in plan mode. \
                     To allow it, redeclare the mode naming it exactly: \
                     cru.modes.plan = {{ tools = {{ \"read_*\", \"{}\" }} }}",
                    tool_call.name, tool_call.name
                ),
            );
        }

        // A plugin narrowed this session's tools with `cru.tools.set_active`.
        // Enforced here as well as in the advertised set, for the reason the
        // card policy just below is: an advertisement-only filter is a
        // suggestion, and a model that names an excluded tool anyway would
        // still run it. Above the hook loop with the other hard refusals, so
        // a plugin cannot "handle" its way around another plugin's narrowing.
        if let Some(reason) = stream_ctx
            .agent_stream_config
            .active_tools
            .as_ref()
            .and_then(|sets| sets.dispatch_refusal(&stream_ctx.session_id, &tool_call.name))
        {
            return deny_tool_call(stream_ctx, &call_id, &tool_call.name, reason);
        }

        // Agent-card tool policy: Deny refuses outright (defense in depth —
        // denied tools are also excluded from the advertised definitions),
        // Ask forces the permission gate even for safe tools, Allow skips it.
        //
        // The Deny half is checked HERE, above the hook loop, for the same
        // reason `plugin_tool_barred` is: a `pre_tool_call` handler returning
        // `Handled` returns before the gate is ever reached, so a plugin could
        // see the arguments of, rewrite, and fabricate a result for a tool the
        // session policy refuses outright. No legitimate plugin needs to
        // intercept a denied tool.
        //
        // Only the hard Deny moves. The permission gate itself — prompting,
        // Lua hooks, the mode stance — stays below interception, because
        // reordering it would change every plugin's contract. So does the
        // isolation gate, deliberately: there, a handler taking the call over
        // *is* the sandbox.
        use crucible_core::agent::ToolPolicy;
        let card_policy = stream_ctx
            .agent_stream_config
            .tool_policy
            .as_ref()
            .and_then(|m| m.get(&tool_call.name))
            .copied();
        if card_policy == Some(ToolPolicy::Deny) {
            return deny_tool_call(
                stream_ctx,
                &call_id,
                &tool_call.name,
                format!(
                    "Tool '{}' is denied by this agent's card tool policy",
                    tool_call.name
                ),
            );
        }

        // Review gate. Waits — it does not refuse — while a file this call
        // targets still has unreviewed hunks from the agent's own earlier
        // work. See `review_gate` for the rule and for why waiting beats
        // denying.
        //
        // Above the hook loop, with the hard refusals, and for the same
        // reason: a `pre_tool_call` handler returning `Handled` returns at the
        // interception branch below and never reaches the isolation gate, the
        // permission gate, or dispatch — yet it can still write, since oci's
        // handler runs bash inside a container over the same bind-mounted
        // workspace. A gate placed under the loop would be one a plugin can
        // opt out of. It also means the wait happens before the session-state
        // lock is taken, so a held call does not starve the session.
        //
        // The cost of being up here is that a Transform handler's rewritten
        // path is not what the gate checked. That is the right trade: the
        // rewrite is the plugin's, the unreviewed hunk is the user's.
        super::review_gate::hold_for_review(stream_ctx, &tool_call.name, &args, &diffs).await;

        // The capture bracket opens HERE, below the gate, and not at the call
        // site above this function. `hold_for_review` is an unbounded wait on
        // a human: everything they do to the worktree while it waits — most
        // sharply the revert that rejecting a hunk performs, which is what
        // releases the gate — would otherwise be measured inside this call's
        // interval and attributed to the agent.
        //
        // It cannot open any lower either. The `pre_tool_call` handlers just
        // below can write (oci's handler runs bash in a container over the
        // same bind-mounted workspace), and a bracket opened after them would
        // report their edits as `external`. The permission prompt further down
        // is the other unbounded wait, and it is handled by re-baselining
        // rather than by moving this point.
        //
        // Side effect of being below the `invoke_tool` unwrap: the bracket now
        // sees the real tool name, so `invoke_tool`→`read_file` stops being
        // bracketed and `invoke_tool`→`delegate_session` is correctly excluded.
        *bracket = stream_ctx.open_review_bracket(&tool_call.name).await;

        let mut intercepted = {
            let state = stream_ctx.session_state.lock().await;
            // Session-scoped handlers, then plugin-registered ones. Plugins
            // live in the loader's VM with their own registry; a RegistryKey
            // is only valid against the state that made it, so the two can't
            // be merged into one registry.
            run_pre_tool_call_handlers(
                stream_ctx,
                &state.registry,
                &state.lua,
                &tool_call.name,
                &mut args,
                &call_id,
            )
            .await
        };

        // Plugin handlers run OUTSIDE the session-state lock: a handler like
        // oci's exec-into-container can legitimately run for minutes, and
        // holding the session's whole state across that starves every other
        // operation on the session (and deadlocks a handler that calls back
        // into an API needing the same lock).
        if intercepted.is_none() {
            if let Some((plugin_registry, plugin_lua)) =
                stream_ctx.agent_stream_config.plugin_handlers.as_ref()
            {
                intercepted = run_pre_tool_call_handlers(
                    stream_ctx,
                    plugin_registry,
                    plugin_lua,
                    &tool_call.name,
                    &mut args,
                    &call_id,
                )
                .await;
            }
        }
        if let Some(mut result) = intercepted {
            // A denial already emitted its events inside deny_tool_call. A
            // Handled result runs the `tool_result` seam first, then emits —
            // the TUI and the model must both see the patched value.
            if result.error.is_none() {
                let (patched, patched_error) = super::tool_hooks::apply_tool_result_handlers(
                    stream_ctx,
                    &tool_call.name,
                    &args,
                    result.result,
                    None,
                )
                .await;
                result.result = patched;
                result.error = patched_error;
                emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_call(
                        &stream_ctx.session_id,
                        &call_id,
                        &tool_call.name,
                        args.clone(),
                    ),
                );
                let payload = if let Some(ref err) = result.error {
                    serde_json::json!({ "error": err })
                } else {
                    serde_json::json!({ "result": &result.result })
                };
                emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_result_with_terminate(
                        &stream_ctx.session_id,
                        &call_id,
                        &tool_call.name,
                        payload,
                        result.terminate,
                    ),
                );
            }
            return Some(result);
        }

        // Default-deny for a session a plugin claimed isolation over.
        //
        // Reaching here means no handler took the call over, so it would run
        // wherever the daemon runs. The question asked is "what does this tool
        // reach", answered by the executor that would run it — not "is this
        // name on a list", which refused every kiln tool along with the shell.
        // See `crucible_core::traits::tools::ToolSurface`.
        //
        // Guarded on the registry being present so the ordinary, unsandboxed
        // session never pays for the surface lookup (which can hydrate every
        // provider's tool list on first use).
        if stream_ctx.agent_stream_config.isolation.is_some() {
            let surface = stream_ctx
                .tool_dispatcher
                .tool_surface(&tool_call.name)
                .await;
            if let Some(refusal) = super::isolation_gate::isolation_refusal(
                stream_ctx.agent_stream_config.isolation.as_ref(),
                &stream_ctx.session_id,
                &tool_call.name,
                surface,
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_call.name,
                    ?surface,
                    "refusing tool: session is isolated and no handler took the call"
                );
                return deny_tool_call(stream_ctx, &call_id, &tool_call.name, refusal);
            }
        }

        // `card_policy` was resolved above the hook loop, where the Deny half is
        // enforced. Ask and Allow only matter once a call actually reaches the
        // gate, so they are read here.
        let requires_gate =
            super::gate_decision::requires_permission_gate(card_policy, &tool_call.name);

        // A card's `allow` skips the PROMPT, never the operator's config:
        // the global `[permissions]` deny rules are absolute even for
        // card-allowed tools. Without this, an untrusted kiln could ship a
        // card granting `bash: allow` and sidestep a configured deny.
        let config_deny = if requires_gate {
            None // the full gate below evaluates the config itself
        } else {
            Self::config_deny_reason(stream_ctx, &tool_call.name, &args)
        };
        if let Some(reason) = config_deny {
            let error_msg = format!(
                "Tool '{}' denied by permissions config: {reason}",
                tool_call.name
            );
            emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::tool_result(
                    &stream_ctx.session_id,
                    &call_id,
                    &tool_call.name,
                    serde_json::json!({ "error": &error_msg }),
                ),
            );
            return Some(crucible_core::traits::chat::ChatToolResult {
                name: tool_call.name.clone(),
                result: String::new(),
                error: Some(error_msg),
                call_id: Some(call_id.clone()),
                terminate: false,
            });
        }

        // `Some(reason)` = approved without asking. Captured here, before the
        // `tool_call` event is emitted below, so the marker ships with the card
        // instead of arriving as a follow-up and popping in.
        let auto_approved = if requires_gate {
            match Self::handle_permission_request(stream_ctx, tool_call, &call_id, &args).await {
                Ok(reason) => reason,
                Err(deny_reason) => {
                    // Feed the SPECIFIC denial reason back to the model so it
                    // can adapt (config rule vs shell policy vs non-interactive).
                    return Some(crucible_core::traits::chat::ChatToolResult {
                        name: tool_call.name.clone(),
                        result: String::new(),
                        error: Some(deny_reason),
                        call_id: Some(call_id.clone()),
                        terminate: false,
                    });
                }
            }
        } else {
            // An agent card's `allow` is also a grant the user never saw, and
            // deserves the same marker. A genuinely safe (read-only) tool is
            // not — nothing was granted, because nothing was needed.
            match card_policy {
                Some(ToolPolicy::Allow) => Some("agent card policy".to_string()),
                _ => None,
            }
        };

        // The second unbounded wait, and the only other one. Re-baseline only
        // when the gate genuinely put the question to a person: `auto_approved`
        // is `Some` exactly when config, the mode, or the card answered without
        // asking, and `requires_gate` is false when nothing was asked at all.
        // Rebaselining unconditionally would discard whatever a plugin handler
        // legitimately wrote above.
        if requires_gate && auto_approved.is_none() {
            stream_ctx.rebase_review_bracket(bracket).await;
        }

        let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());
        let (mut description, mut source) = stream_ctx
            .tool_dispatcher
            .get_tool_ref(&tool_call.name)
            .and_then(|tool_ref| match &tool_ref.source {
                ToolSource::Core | ToolSource::Crucible => Some((
                    tool_ref.definition.description.map(|d| d.to_string()),
                    Some(Self::format_tool_source(&tool_ref.source)),
                )),
                // `Acp` is unreachable here — a delegated agent's tools are
                // never in our registry — but it is not a description source
                // either way.
                ToolSource::Mcp { .. } | ToolSource::Plugin { .. } | ToolSource::Acp { .. } => None,
            })
            .unwrap_or((None, None));

        let mut lua_primary_arg: Option<String> = None;

        let hook_event = ToolDisplayStartEvent {
            name: tool_call.name.clone(),
            args: args_str.clone(),
        };
        if let Some(hints) =
            super::tool_hooks::resolve_display_start_hints(stream_ctx, &hook_event).await
        {
            if let Some(label) = hints.label {
                description = Some(label);
            }
            if let Some(detail) = hints.detail {
                source = Some(detail);
            }
            if let Some(pa) = hints.primary_arg {
                lua_primary_arg = Some(pa);
            }
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::tool_call_with_metadata(
                &stream_ctx.session_id,
                &call_id,
                &tool_call.name,
                args.clone(),
                description,
                source,
                lua_primary_arg,
                diffs,
                auto_approved.clone(),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %tool_call.name,
                "No subscribers for tool_call event"
            );
        }

        let before_event = ToolBeforeExecuteEvent {
            name: tool_call.name.clone(),
            args: args.clone(),
        };
        let hook_env_vars =
            super::tool_hooks::resolve_before_execute_env(stream_ctx, &before_event).await;

        // ACP agents execute their own tools internally; Crucible only sees the
        // tool_call / tool_result as notifications. If the tool isn't in our
        // dispatcher, skip dispatch — the tool_call event was already emitted
        // above (for TUI display), and the ACP ToolEnd chunk will emit the
        // matching tool_result separately. Dispatching would produce a bogus
        // "Unknown tool" error that the TUI would render as a failed call.
        if !stream_ctx.tool_dispatcher.has_tool(&tool_call.name) {
            // A tool reached via invoke_tool has no external agent to answer it
            // — returning None would leave the model waiting for a result that
            // never comes and stall the turn until the dispatch timeout. Return
            // an error so the model can recover (e.g. re-run discover_tools). A
            // genuine ACP tool (not unwrapped) still falls through to None so
            // the external agent supplies the result.
            match Self::missing_tool_result(was_unwrapped, &tool_call.name, &call_id) {
                Some(result) => {
                    emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            &call_id,
                            &tool_call.name,
                            serde_json::json!({ "error": result.error }),
                        ),
                    );
                    return Some(result);
                }
                None => {
                    debug!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        "Tool not in local dispatcher; leaving result to external agent"
                    );
                    return None;
                }
            }
        }

        // Most tools get the standard 30 s dispatch timeout. A blocking
        // `delegate_session` legitimately runs a whole child session inside
        // this dispatch, so it gets the delegation timeout plus margin — the
        // delegation layer cancels the child on its own timeout first, this
        // outer bound is only the backstop.
        let dispatch_timeout_secs = if tool_call.name == "delegate_session" {
            stream_ctx
                .agent_stream_config
                .delegation_timeout_secs
                .unwrap_or(300)
                .saturating_add(30)
        } else {
            30
        };
        let tool_result = tokio::time::timeout(
            std::time::Duration::from_secs(dispatch_timeout_secs),
            stream_ctx
                .tool_dispatcher
                .dispatch_tool(&tool_call.name, args.clone(), hook_env_vars),
        )
        .await;
        let (mut result_str, mut error_str) = match tool_result {
            Ok(Ok(val)) => (val.to_string(), None),
            Ok(Err(e)) => (String::new(), Some(e)),
            Err(_elapsed) => (
                String::new(),
                Some(
                    anyhow::anyhow!(
                        "Tool '{}' timed out after {} seconds",
                        tool_call.name,
                        dispatch_timeout_secs
                    )
                    .to_string(),
                ),
            ),
        };

        // `tool_result` seam: chained post-execution patches over what the
        // MODEL receives (`tool:display_complete` only changes what the user
        // sees). Runs BEFORE spill, so a redacted secret never reaches the
        // spill file either.
        (result_str, error_str) = super::tool_hooks::apply_tool_result_handlers(
            stream_ctx,
            &tool_call.name,
            &args,
            result_str,
            error_str,
        )
        .await;

        // Spill large tool outputs to disk and replace with a token-efficient reference.
        // Skip tools whose output is trivially reproducible from existing data on disk.
        const SPILL_THRESHOLD: usize = 10 * 1024; // 10KB
        let should_spill = error_str.is_none()
            && result_str.len() >= SPILL_THRESHOLD
            && !is_reproducible_tool(&tool_call.name);
        let spill_path = if should_spill {
            let counter = {
                let state = stream_ctx.session_state.lock().await;
                state
                    .spill_counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            };
            match Self::spill_tool_output(
                &stream_ctx.session_dir,
                &tool_call.name,
                &result_str,
                counter,
            )
            .await
            {
                Ok((path, filename)) => {
                    // Count lines in the actual content, not the JSON-serialized string
                    let line_count = serde_json::from_str::<serde_json::Value>(&result_str)
                        .ok()
                        .and_then(|v| {
                            v.as_str().map(|s| s.lines().count()).or_else(|| {
                                v.get("result")
                                    .and_then(|r| r.as_str())
                                    .map(|s| s.lines().count())
                            })
                        })
                        .unwrap_or_else(|| result_str.lines().count());
                    let byte_kb = result_str.len() / 1024;
                    result_str = format!(
                        "[{line_count} lines, {byte_kb}KB — full output in $CRU_SESSION_DIR/tools/{filename}]"
                    );
                    Some(path)
                }
                Err(e) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_call.name,
                        error = %e,
                        "Failed to spill tool output, sending full result"
                    );
                    None
                }
            }
        } else {
            None
        };

        let mut event_result = if let Some(error) = &error_str {
            serde_json::json!({ "error": error })
        } else {
            serde_json::json!({ "result": result_str })
        };

        if let Some(ref path) = spill_path {
            event_result["spill_path"] = serde_json::json!(path);
        }

        let complete_event = ToolDisplayCompleteEvent {
            name: tool_call.name.clone(),
            args: args_str,
            result: error_str.clone().unwrap_or_else(|| result_str.clone()),
        };
        if let Some(hints) =
            super::tool_hooks::resolve_display_complete_hints(stream_ctx, &complete_event).await
        {
            if let Some(summary) = hints.summary {
                event_result["summary"] = serde_json::json!(summary);
            }
        }

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
            terminate: false,
        })
    }

    /// Unwrap an `invoke_tool` bridge call into the inner `ChatToolCall`,
    /// reusing the original call id so the result matches the model's request.
    /// Returns an error `ChatToolResult` (never a panic) for a missing/blank
    /// `name`, a recursive `invoke_tool`, or an inner tool disallowed by the
    /// current plan mode.
    fn unwrap_invoke_tool(
        mode: &str,
        tool_call: &crucible_core::traits::chat::ChatToolCall,
        call_id: &str,
    ) -> Result<
        crucible_core::traits::chat::ChatToolCall,
        crucible_core::traits::chat::ChatToolResult,
    > {
        let args = tool_call
            .arguments
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let invoke_err = |msg: String| crucible_core::traits::chat::ChatToolResult {
            name: "invoke_tool".to_string(),
            result: String::new(),
            error: Some(msg),
            call_id: Some(call_id.to_string()),
            terminate: false,
        };

        let inner_name = match args.get("name").and_then(|v| v.as_str()) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => {
                return Err(invoke_err(
                    "invoke_tool requires a non-empty string `name` field naming the tool to \
                     call, plus an optional `args` object"
                        .to_string(),
                ))
            }
        };
        if inner_name == "invoke_tool" {
            return Err(invoke_err("invoke_tool cannot invoke itself".to_string()));
        }

        // Plan mode fails closed: only the read-only plan tool set may be
        // invoked. Gateway/upstream tools are never in that set, so the bridge
        // cannot reach them in plan mode — mirroring visible_tools(), which also
        // excludes upstream tools categorically because we can't tell which
        // ones write.
        if mode == "plan"
            && !crate::tools::tool_modes::PLAN_TOOL_NAMES.contains(&inner_name.as_str())
        {
            return Err(invoke_err(format!(
                "Tool '{inner_name}' is not available in plan mode"
            )));
        }

        let inner_args = args
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        Ok(crucible_core::traits::chat::ChatToolCall {
            name: inner_name,
            arguments: Some(inner_args),
            id: Some(call_id.to_string()),
        })
    }

    /// Decide the result for a tool the local dispatcher doesn't know. When the
    /// call was unwrapped from `invoke_tool` (the model named a tool that isn't
    /// available), return an error `ChatToolResult` so the turn completes rather
    /// than hanging. Otherwise return `None` so an external ACP agent supplies
    /// the result.
    fn missing_tool_result(
        was_unwrapped: bool,
        name: &str,
        call_id: &str,
    ) -> Option<crucible_core::traits::chat::ChatToolResult> {
        if !was_unwrapped {
            return None;
        }
        Some(crucible_core::traits::chat::ChatToolResult {
            name: name.to_string(),
            result: String::new(),
            error: Some(format!(
                "Tool not found: {name}. Use discover_tools to list available tools."
            )),
            call_id: Some(call_id.to_string()),
            terminate: false,
        })
    }

    /// Spill large tool output to disk. Returns (absolute_path, filename).
    async fn spill_tool_output(
        session_dir: &std::path::Path,
        tool_name: &str,
        output: &str,
        counter: u32,
    ) -> anyhow::Result<(PathBuf, String)> {
        let tools_dir = session_dir.join("tools");
        tokio::fs::create_dir_all(&tools_dir).await?;

        let name_slug: String = tool_name
            .chars()
            .take(20)
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let filename = format!("{}-{}.txt", name_slug, counter);
        let path = tools_dir.join(&filename);

        tokio::fs::write(&path, output).await?;
        Ok((path, filename))
    }
}

/// Tools whose output is trivially reproducible from existing data on disk.
/// These should not be spilled — the content already exists and can be re-read.
fn is_reproducible_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "mcp_read"
            | "edit_file"
            | "mcp_edit"
            | "write_file"
            | "mcp_write"
            | "glob"
            | "mcp_glob"
            | "grep"
            | "mcp_grep"
            | "list_notes"
            | "read_note"
            | "read_metadata"
            | "get_kiln_info"
    )
}

#[cfg(test)]
#[path = "tool_call/tests.rs"]
mod invoke_tool_tests;
