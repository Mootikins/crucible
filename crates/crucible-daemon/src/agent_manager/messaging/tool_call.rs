use super::super::*;
use crucible_core::types::acp::FileDiff;
use crucible_core::types::{CanonicalToolCall, ToolSource};
use crucible_lua::StageId;
use crucible_lua::ToolBeforeExecuteEvent;
use std::ops::ControlFlow;

use crate::agent_manager::vm_pass::run_handlers;

use super::gate_decision::Decision;

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
/// `unenforceable_reason` beside it).
fn deny_tool_call(
    stream_ctx: &StreamContext,
    call_id: &str,
    tool_name: &str,
    error_msg: String,
) -> crucible_core::traits::chat::ChatToolResult {
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
    crucible_core::traits::chat::ChatToolResult::error(tool_name, call_id, error_msg)
}

/// The `data.result` body of a `tool_result` event: `{"error": …}` for a
/// failed call, else `{"result": …}`. See `ToolResultBody` for the reader.
pub(super) fn tool_result_body(
    result: impl serde::Serialize,
    error: Option<&str>,
) -> serde_json::Value {
    match error {
        Some(error) => serde_json::json!({ "error": error }),
        None => serde_json::json!({ "result": result }),
    }
}

/// Whether code running under `source` may take a tool call over — return
/// `{ handled = true, … }` or a transform from `pre_tool_call`.
///
/// This is the ONE seam that gates the power, so it is the one place that
/// decides it. `LuaSource` used to answer for itself, which made a provenance
/// tag grant a capability; the answer lives here now, and the type only says
/// who wrote the registration.
///
/// # The partition is by TRUST ROOT, not by identity
///
/// `handled` returns BEFORE the permission gate and hands the model a
/// fabricated result it reads as the tool's own. But that gate protects the
/// USER from the AGENT. It was never a boundary between the user and their
/// own configuration, so operator-authored code walking past it is the
/// operator overriding their own guard, which is theirs to do.
///
/// - [`LuaSource::Plugin`] is third-party code, so the declaration IS the
///   boundary: `intercepts_tools` in the plugin's own spec table, which the
///   loader records by name. An unrecorded name answers `false` — a plugin
///   the loader never admitted must not gain the power by being unknown.
/// - [`LuaSource::UserLua`] is the operator's own `init.lua`. Withholding the
///   power buys no containment: that file already runs arbitrary Lua and
///   reaches `cru.shell`, whose default policy blocks four command names and
///   never reads arguments, so `cru.shell.exec("sh", { "-c", … })` runs
///   anything. Refusing it here would remove a route and close no door.
/// - [`LuaSource::Builtin`] is `runtime/defaults/init.luau`, which ships with
///   the daemon and is the only definition of the permission modes and the
///   plan-mode deny hook. Refusing interception to the code that DEFINES the
///   gate is incoherent.
/// - [`LuaSource::Eval`] is `false`, and this arm is the one to leave alone.
///   A human types `cru lua`, so it is tempting to read an eval as the
///   operator. It is not: an eval is a socket call, and the socket is what an
///   RPC client reaches. Treating it as the operator would let any local
///   caller that can open the daemon socket register an interception on a
///   session it merely names. `cancel` stays open to it, because refusing a
///   call can only narrow.
///
/// A plugin cannot grant itself the right at call time. The recorded table
/// lives in the VM's Rust-side app data, which Lua cannot reach, and only a
/// loader writes it.
fn may_take_a_tool_call_over(lua: &mlua::Lua, source: &crucible_lua::LuaSource) -> bool {
    match source {
        crucible_lua::LuaSource::Plugin(name) => crucible_lua::intercept_for(lua, name),
        crucible_lua::LuaSource::UserLua | crucible_lua::LuaSource::Builtin => true,
        crucible_lua::LuaSource::Eval => false,
    }
}

/// Run every `cru.on("pre_tool_call", …)` handler in one registry.
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
/// handlers live in the loader's VM, the one VM that runs Lua files.
async fn run_pre_tool_call_handlers(
    stream_ctx: &StreamContext,
    registry: &crucible_lua::LuaScriptHandlerRegistry,
    lua: &mlua::Lua,
    tool_name: &str,
    args: &mut serde_json::Value,
    call_id: &str,
) -> Option<crucible_core::traits::chat::ChatToolResult> {
    for handler in registry.runtime_handlers_for(
        StageId::PreToolCall.as_str(),
        Some(tool_name),
        crucible_lua::Firing::InSession(&stream_ctx.session_id),
    ) {
        let event = SessionEvent::Custom {
            name: "pre_tool_call".to_string(),
            payload: serde_json::json!({
                "tool": tool_name,
                "args": &*args,
            }),
        };
        match registry
            .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
            .await
        {
            Ok(crucible_lua::ScriptHandlerResult::Cancel { reason }) => {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = handler.id,
                    reason = %reason,
                    "pre_tool_call handler cancelled"
                );
                return Some(deny_tool_call(
                    stream_ctx,
                    call_id,
                    tool_name,
                    format!("Tool blocked by cru.on handler: {}", reason),
                ));
            }
            Ok(crucible_lua::ScriptHandlerResult::Handled { result, terminate })
                if may_take_a_tool_call_over(lua, &handler.source) =>
            {
                debug!(
                    session_id = %stream_ctx.session_id,
                    tool = %tool_name,
                    handler = handler.id,
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
            Ok(crucible_lua::ScriptHandlerResult::Transform(val))
                if may_take_a_tool_call_over(lua, &handler.source) =>
            {
                if let Some(new_args) = val.get("args") {
                    if new_args.is_object() {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_name,
                            handler = handler.id,
                            "pre_tool_call handler rewrote arguments"
                        );
                        *args = new_args.clone();
                    } else {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %tool_name,
                            handler = handler.id,
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
                    handler = handler.id,
                    owner = %handler.source,
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
                    handler = handler.id,
                    error = %error,
                    "pre_tool_call handler error, denying tool (fail-closed)"
                );
                return Some(deny_tool_call(
                    stream_ctx,
                    call_id,
                    tool_name,
                    format!(
                        "Tool denied: pre_tool_call handler error in '{}': {error}",
                        handler.name
                    ),
                ));
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
    ) -> crucible_core::traits::chat::ChatToolResult {
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
                Err(result) => return result,
            }
        } else {
            tool_call
        };

        let args = tool_call
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
        // *is* the sandbox. `decide_permission` below asks the card again,
        // for the call that the handlers may have rewritten.
        if let Some(reason) = super::gate_decision::card_refusal(
            stream_ctx.agent_stream_config.tool_policy.as_ref(),
            &CanonicalToolCall::crucible_tool(&tool_call.name, &args),
        ) {
            return deny_tool_call(stream_ctx, &call_id, &tool_call.name, reason);
        }

        // The capture bracket opens HERE, below the hard refusals and the
        // `invoke_tool` unwrap, and not at the call site above this function.
        //
        // It cannot open any lower. The `pre_tool_call` handlers just below
        // can write (oci's handler runs bash in a container over the same
        // bind-mounted workspace), and a bracket opened after them would
        // report their edits as `external`. The permission prompt further down
        // is an unbounded wait on a person. The bracket rebases after that
        // prompt, so this point does not move.
        //
        // Side effect of being below the `invoke_tool` unwrap: the bracket now
        // sees the real tool name, so `invoke_tool`→`read_file` stops being
        // bracketed and `invoke_tool`→`delegate_session` is correctly excluded.
        *bracket = stream_ctx.open_review_bracket(&tool_call.name).await;

        // Session-scoped handlers first, then plugin-registered ones; the
        // first interception wins. Plugins live in the loader's VM with their
        // own registry; a RegistryKey is only valid against the state that
        // made it, so the two can't be merged into one registry.
        //
        // Plugin handlers run OUTSIDE the session-state lock: a handler like
        // oci's exec-into-container can legitimately run for minutes, and
        // holding the session's whole state across that starves every other
        // operation on the session (and deadlocks a handler that calls back
        // into an API needing the same lock).
        let (args, intercepted) = run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            (args, None),
            |registry, lua, (mut args, _)| {
                let call_id = &call_id;
                Box::pin(async move {
                    let hit = run_pre_tool_call_handlers(
                        stream_ctx,
                        &registry,
                        &lua,
                        &tool_call.name,
                        &mut args,
                        call_id,
                    )
                    .await;
                    match hit {
                        Some(hit) => ControlFlow::Break((args, Some(hit))),
                        None => ControlFlow::Continue((args, None)),
                    }
                })
            },
        )
        .await;
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
                let call = stream_ctx.rendered_call(&tool_call.name, &args).await;
                emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::tool_call_with_metadata(
                        &stream_ctx.session_id,
                        &call_id,
                        &tool_call.name,
                        args.clone(),
                        None,
                        None,
                        Some(call),
                        None,
                    ),
                );
                let payload = tool_result_body(&result.result, result.error.as_deref());
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
            return result;
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

        // The one tool policy, the same function the ACP permission handler
        // calls. `auto_approved` is `Some(reason)` when the call was approved
        // WITHOUT asking, and by which layer. Captured here, before the
        // `tool_call` event is emitted below, so the marker ships with the
        // card instead of arriving as a follow-up and popping in.
        //
        // The canonical call is made here, after the handlers above may have
        // rewritten the arguments, so the gate decides the call that runs.
        // It is rendered before the gate, so the prompt and the event show
        // the same render.
        //
        // The diffs of the model's call apply only to its own arguments. A
        // handler that rewrote them, or a call that came with no diffs, gets
        // the diffs of the arguments that run.
        let unchanged = tool_call
            .arguments
            .as_ref()
            .unwrap_or(&serde_json::Value::Null)
            == &args;
        let diffs = if unchanged && !diffs.is_empty() {
            diffs
        } else {
            crate::tools::diff_synth::synthesize_diffs(&tool_call.name, &args)
        };
        let call = CanonicalToolCall {
            diffs,
            ..stream_ctx.rendered_call(&tool_call.name, &args).await
        };
        let auto_approved = match super::gate_decision::decide_permission(
            &stream_ctx.permission_context(),
            &call,
            &args,
        )
        .await
        {
            Decision::Deny(reason) => {
                return deny_tool_call(stream_ctx, &call_id, &tool_call.name, reason)
            }
            Decision::NoAnswer => {
                let reason = "The permission prompt ended with no answer".to_string();
                return deny_tool_call(stream_ctx, &call_id, &tool_call.name, reason);
            }
            Decision::Allow(marker) => marker,
            // The second unbounded wait, and the only other one. Re-baseline
            // only when the gate put the question to a person: rebaselining
            // after an automatic answer would discard whatever a plugin
            // handler legitimately wrote above.
            Decision::UserAllowed => {
                stream_ctx.rebase_review_bracket(bracket).await;
                None
            }
        };

        let labels = stream_ctx
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
        stream_ctx
            .announce_tool_call(&call_id, &args, labels, call.clone(), auto_approved)
            .await;

        let before_event = ToolBeforeExecuteEvent {
            name: tool_call.name.clone(),
            args: args.clone(),
        };
        let hook_env_vars =
            super::tool_hooks::resolve_before_execute_env(stream_ctx, &before_event).await;

        // The model named a deferred tool through `invoke_tool` that does
        // not exist. Say so, and point the model at `discover_tools`.
        if was_unwrapped && !stream_ctx.tool_dispatcher.has_tool(&tool_call.name) {
            return deny_tool_call(
                stream_ctx,
                &call_id,
                &tool_call.name,
                format!(
                    "Tool not found: {}. Use discover_tools to list available tools.",
                    tool_call.name
                ),
            );
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
        let (result_str, error_str) = match tool_result {
            // A text result IS the text. `to_string()` on a wrapped value
            // would hand the model a JSON envelope (and on a bare string a
            // quoted, newline-escaped literal); only genuinely structured
            // results — plugin tables, daemon note tools — serialize.
            Ok(Ok(serde_json::Value::String(text))) => (text, None),
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

        let (result, error) = stream_ctx
            .finish_tool_result(&call_id, call, &args, result_str, error_str, true)
            .await;
        crucible_core::traits::chat::ChatToolResult {
            name: tool_call.name.clone(),
            result,
            error,
            call_id: Some(call_id),
            terminate: false,
        }
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
        let invoke_err = |msg: String| {
            crucible_core::traits::chat::ChatToolResult::error("invoke_tool", call_id, msg)
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

/// The handlers of a tool call and of its result. Crucible's own tools and
/// the tools of an agent that runs its own tools go through the same
/// functions. The agent's tools skip only what the agent owns: the dispatch,
/// and the spill, because the agent already has the output.
impl StreamContext {
    /// Add a tool node under the current node. A tool node does not move
    /// the cursor, so undo counts the turn and not its calls.
    pub(super) async fn add_tool_node(&self, node: crucible_core::turn::NodeContent) {
        let mut tree = self.conversation_tree.lock().await;
        let parent = tree.current();
        tree.add_child(parent, node);
    }

    /// Emit the `tool_call` event. `labels` is the description and the
    /// source of the tool.
    pub(super) async fn announce_tool_call(
        &self,
        call_id: &str,
        args: &serde_json::Value,
        labels: (Option<String>, Option<String>),
        call: CanonicalToolCall,
        auto_approved: Option<String>,
    ) {
        let (description, source) = labels;
        let tool = call.tool.clone();
        if !emit_event(
            &self.event_tx,
            SessionEventMessage::tool_call_with_metadata(
                &self.session_id,
                call_id,
                &tool,
                args.clone(),
                description,
                source,
                Some(call),
                auto_approved,
            ),
        ) {
            warn!(session_id = %self.session_id, %tool, "No subscribers for tool_call event");
        }
    }

    /// Run the `tool_result` hooks, spill a large output when `spill` is
    /// set, render the finished `call`, then emit the `tool_result` event.
    /// Returns the result and the error that the hooks made, which is what
    /// the model of a Crucible tool reads.
    pub(super) async fn finish_tool_result(
        &self,
        call_id: &str,
        call: CanonicalToolCall,
        args: &serde_json::Value,
        result: String,
        error: Option<String>,
        spill: bool,
    ) -> (String, Option<String>) {
        let tool = call.tool.clone();
        let tool = tool.as_str();
        // The hooks run BEFORE the spill, so a redacted secret never reaches
        // the spill file either.
        let (mut result, error) =
            super::tool_hooks::apply_tool_result_handlers(self, tool, args, result, error).await;
        // The render reads the whole result, before a spill cuts it. With
        // no Lua render the card keeps the render of the call.
        let render = super::tool_hooks::lua_render(
            self.agent_stream_config.plugin_handlers.as_ref(),
            &self.session_id,
            &call,
            args,
            &self.origin,
            Some((&result, error.as_deref())),
        )
        .await;

        // Skip tools whose output is trivially reproducible from existing
        // data on disk.
        const SPILL_THRESHOLD: usize = 10 * 1024; // 10KB
        let mut spill_path = None;
        if spill
            && error.is_none()
            && result.len() >= SPILL_THRESHOLD
            && !is_reproducible_tool(tool)
        {
            let counter = self
                .slot
                .spill_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            match AgentManager::spill_tool_output(&self.session_dir, tool, &result, counter).await {
                Ok((path, filename)) => {
                    // The model reads `result`, so its own line count is
                    // the honest number.
                    let line_count = result.lines().count();
                    let byte_kb = result.len() / 1024;
                    result = format!(
                        "[{line_count} lines, {byte_kb}KB — full output in $CRU_SESSION_DIR/tools/{filename}]"
                    );
                    spill_path = Some(path);
                }
                Err(e) => warn!(
                    session_id = %self.session_id,
                    %tool,
                    error = %e,
                    "Failed to spill tool output, sending full result"
                ),
            }
        }

        let mut event_result = tool_result_body(&result, error.as_deref());
        if let Some(path) = spill_path {
            event_result["spill_path"] = serde_json::json!(path);
        }
        if let Some(render) = render {
            event_result["render"] = serde_json::json!(render);
        }
        if !emit_event(
            &self.event_tx,
            SessionEventMessage::tool_result(&self.session_id, call_id, tool, event_result),
        ) {
            warn!(session_id = %self.session_id, %tool, "No subscribers for tool_result event");
        }
        (result, error)
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
