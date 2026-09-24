//! Render, before-execute, and tool-result hook resolution for tool calls.
//!
//! Each resolver runs the handler VM's handlers (under the session
//! state lock), then the plugin VM's with the lock released — plugin Lua may
//! run for seconds and must not hold the session's whole state hostage.
//! Env maps merge with session
//! entries winning a key collision (the more specific scope overrides).
//! `apply_tool_result_handlers` is the odd one out: chained partial patches
//! that shape the finished result as the model receives it, so every handler
//! sees the previous ones' edits rather than the first answer winning.

use crucible_lua::StageId;
use crucible_lua::{execute_tool_before_execute_hooks, ToolBeforeExecuteEvent};
use std::ops::ControlFlow;
use tracing::warn;

use crate::agent_manager::vm_pass::run_handlers;

use super::StreamContext;

/// The Lua render of `call`, or `None` when no render answers or a render
/// fails. `outcome` is the result text and the error of a finished call.
pub(crate) async fn lua_render(
    handlers: Option<&crate::agent_manager::vm_pass::PluginHandlers>,
    session_id: &str,
    call: &crucible_core::types::CanonicalToolCall,
    args: &serde_json::Value,
    origin: &crucible_core::turn::TurnOrigin,
    outcome: Option<(&str, Option<&str>)>,
) -> Option<crucible_core::types::ToolRender> {
    let (registry, lua) = handlers?;
    crucible_lua::execute_tool_render(lua, registry, Some(session_id), call, args, origin, outcome)
        .await
        .unwrap_or_else(|error| {
            warn!(session_id, kind = %call.kind, %error, "tool:render failed");
            None
        })
}

/// Set the render of `call`: the Lua render of its kind, else the fallback.
///
/// The daemon renders a call once for each event that carries it and once
/// for its prompt, so every client reads the same table. A render that fails
/// gives the fallback, which shows every field of the call.
pub(crate) async fn render_call(
    handlers: Option<&crate::agent_manager::vm_pass::PluginHandlers>,
    session_id: &str,
    call: &mut crucible_core::types::CanonicalToolCall,
    args: &serde_json::Value,
    origin: &crucible_core::turn::TurnOrigin,
) {
    let render = lua_render(handlers, session_id, call, args, origin, None).await;
    call.render =
        Some(render.unwrap_or_else(|| crucible_core::types::ToolRender::fallback(call, args)));
}

impl StreamContext {
    /// The canonical call of a Crucible tool, with its render.
    pub(super) async fn rendered_call(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> crucible_core::types::CanonicalToolCall {
        let mut call = crucible_core::types::CanonicalToolCall::crucible_tool(name, args);
        let handlers = self.agent_stream_config.plugin_handlers.as_ref();
        render_call(handlers, &self.session_id, &mut call, args, &self.origin).await;
        call
    }
}

/// The `tool_result` seam: chained partial patches over a finished tool
/// call's outcome, as the MODEL will receive it.
///
/// Handlers get `{ tool, args, result, error }` and return a `Transform` of
/// `{ result = <string> }` and/or `{ error = <string> }` — partial: an
/// omitted key keeps the current value, and each handler sees the previous
/// handlers' patches. Use cases:
/// redacting secrets from bash output, summarising a large read. Execution
/// already happened, so Cancel/Handled have nothing to act on and are
/// ignored; handler errors fail open like every non-gate hook — a redactor
/// that must be able to veto belongs in `pre_tool_call`, before execution.
pub(super) async fn apply_tool_result_handlers(
    stream_ctx: &StreamContext,
    tool_name: &str,
    args: &serde_json::Value,
    result: String,
    error: Option<String>,
) -> (String, Option<String>) {
    async fn run_pass(
        stream_ctx: &StreamContext,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        tool_name: &str,
        args: &serde_json::Value,
        result: &mut String,
        error: &mut Option<String>,
    ) {
        for handler in registry.runtime_handlers_for(
            StageId::ToolResult.as_str(),
            Some(tool_name),
            crucible_lua::Firing::InSession(&stream_ctx.session_id),
        ) {
            let event = crucible_core::events::SessionEvent::Custom {
                name: "tool_result".to_string(),
                payload: serde_json::json!({
                    "tool": tool_name,
                    "args": args,
                    "result": &*result,
                    "error": &*error,
                }),
            };
            match registry
                .execute_runtime_handler(lua, handler.id, &event, Some(&stream_ctx.session_id))
                .await
            {
                Ok(crucible_lua::ScriptHandlerResult::Transform(val)) => {
                    if let Some(new_result) = val.get("result").and_then(|v| v.as_str()) {
                        *result = new_result.to_string();
                    }
                    if let Some(new_error) = val.get("error") {
                        *error = match new_error {
                            serde_json::Value::String(s) => Some(s.clone()),
                            serde_json::Value::Null => None,
                            _ => error.take(),
                        };
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        tool = %tool_name,
                        handler = handler.id,
                        error = %err,
                        "tool_result handler error (fail-open)"
                    );
                }
            }
        }
    }

    run_handlers(
        stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
        (result, error),
        |registry, lua, (mut result, mut error)| {
            Box::pin(async move {
                run_pass(
                    stream_ctx,
                    &registry,
                    &lua,
                    tool_name,
                    args,
                    &mut result,
                    &mut error,
                )
                .await;
                ControlFlow::Continue((result, error))
            })
        },
    )
    .await
}

pub(super) async fn resolve_before_execute_env(
    stream_ctx: &StreamContext,
    event: &ToolBeforeExecuteEvent,
) -> std::collections::HashMap<String, String> {
    // Each VM's env is folded under the one before it, so a session value
    // wins over a plugin value for the same key.
    run_handlers(
        stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
        std::collections::HashMap::new(),
        |registry, lua, acc| {
            Box::pin(async move {
                let mut env = match execute_tool_before_execute_hooks(
                    &lua,
                    &registry,
                    Some(&stream_ctx.session_id),
                    event,
                )
                .await
                {
                    Ok(Some(result)) => result.env,
                    Ok(None) => std::collections::HashMap::new(),
                    Err(error) => {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %event.name,
                            error = %error,
                            "tool:before_execute hook error, proceeding without env vars"
                        );
                        std::collections::HashMap::new()
                    }
                };
                env.extend(acc);
                ControlFlow::Continue(env)
            })
        },
    )
    .await
}
