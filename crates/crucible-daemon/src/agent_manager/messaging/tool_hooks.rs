//! Display, before-execute, and tool-result hook resolution for tool calls.
//!
//! Each resolver runs the handler VM's handlers (under the session
//! state lock), then the plugin VM's with the lock released — plugin Lua may
//! run for seconds and must not hold the session's whole state hostage.
//! Display hints are first-non-empty-wins; env maps merge with session
//! entries winning a key collision (the more specific scope overrides).
//! `apply_tool_result_handlers` is the odd one out: chained partial patches
//! that shape the finished result as the model receives it, so every handler
//! sees the previous ones' edits rather than the first answer winning.

use crucible_lua::StageId;
use crucible_lua::{
    execute_tool_before_execute_hooks, execute_tool_display_complete_hooks,
    execute_tool_display_start_hooks, ToolBeforeExecuteEvent, ToolDisplayCompleteEvent,
    ToolDisplayCompleteHints, ToolDisplayStartEvent, ToolDisplayStartHints,
};
use std::ops::ControlFlow;
use tracing::warn;

use crate::agent_manager::vm_pass::run_handlers;

use super::StreamContext;

/// A display stage a tool call passes through, with the Lua hook that
/// resolves its hints. The two stages differ only in their event, their
/// hints, and the hook they call.
pub(super) trait DisplayStage: Sync {
    type Hints: Send;
    const STAGE: &'static str;

    fn tool_name(&self) -> &str;

    fn run(
        &self,
        lua: &mlua::Lua,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        session_id: &str,
    ) -> impl std::future::Future<Output = mlua::Result<Option<Self::Hints>>> + Send;
}

impl DisplayStage for ToolDisplayStartEvent {
    type Hints = ToolDisplayStartHints;
    const STAGE: &'static str = "tool:display_start";

    fn tool_name(&self) -> &str {
        &self.name
    }

    async fn run(
        &self,
        lua: &mlua::Lua,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        session_id: &str,
    ) -> mlua::Result<Option<Self::Hints>> {
        execute_tool_display_start_hooks(lua, registry, Some(session_id), self).await
    }
}

impl DisplayStage for ToolDisplayCompleteEvent {
    type Hints = ToolDisplayCompleteHints;
    const STAGE: &'static str = "tool:display_complete";

    fn tool_name(&self) -> &str {
        &self.name
    }

    async fn run(
        &self,
        lua: &mlua::Lua,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        session_id: &str,
    ) -> mlua::Result<Option<Self::Hints>> {
        execute_tool_display_complete_hooks(lua, registry, Some(session_id), self).await
    }
}

/// Resolve the display hints for one stage from the handler VM's handlers,
/// then the plugin VM's. A hook error falls back to the default metadata.
pub(super) async fn resolve_hints<E: DisplayStage>(
    stream_ctx: &StreamContext,
    event: &E,
) -> Option<E::Hints> {
    run_handlers(
        stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
        None,
        |registry, lua, _| {
            Box::pin(async move {
                match event.run(&lua, &registry, &stream_ctx.session_id).await {
                    Ok(Some(hints)) => ControlFlow::Break(Some(hints)),
                    Ok(None) => ControlFlow::Continue(None),
                    Err(error) => {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %event.tool_name(),
                            error = %error,
                            "{} hook error, falling back to default metadata",
                            E::STAGE
                        );
                        ControlFlow::Continue(None)
                    }
                }
            })
        },
    )
    .await
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
        for handler in registry.runtime_handlers_for(StageId::ToolResult.as_str(), Some(tool_name))
        {
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
                .execute_runtime_handler(lua, &handler.name, &event, Some(&stream_ctx.session_id))
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
                        handler = %handler.name,
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
