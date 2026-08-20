//! Display, before-execute, and tool-result hook resolution for tool calls.
//!
//! Each resolver runs the session VM's handlers first (under the session
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
use tracing::warn;

use super::StreamContext;

pub(super) async fn resolve_display_start_hints(
    stream_ctx: &StreamContext,
    event: &ToolDisplayStartEvent,
) -> Option<ToolDisplayStartHints> {
    let session_hints = {
        let state = stream_ctx.session_state.lock().await;
        match execute_tool_display_start_hooks(
            &state.lua,
            &state.registry,
            Some(&stream_ctx.session_id),
            event,
        )
        .await
        {
            Ok(hints) => hints,
            Err(error) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    tool = %event.name,
                    error = %error,
                    "Lua tool:display_start hook error, falling back to default metadata"
                );
                None
            }
        }
    };
    if session_hints.is_some() {
        return session_hints;
    }
    let (plugin_registry, plugin_lua) = stream_ctx.agent_stream_config.plugin_handlers.as_ref()?;
    match execute_tool_display_start_hooks(
        plugin_lua,
        plugin_registry,
        Some(&stream_ctx.session_id),
        event,
    )
    .await
    {
        Ok(hints) => hints,
        Err(error) => {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %event.name,
                error = %error,
                "plugin tool:display_start hook error, falling back to default metadata"
            );
            None
        }
    }
}

pub(super) async fn resolve_display_complete_hints(
    stream_ctx: &StreamContext,
    event: &ToolDisplayCompleteEvent,
) -> Option<ToolDisplayCompleteHints> {
    let session_hints = {
        let state = stream_ctx.session_state.lock().await;
        match execute_tool_display_complete_hooks(
            &state.lua,
            &state.registry,
            Some(&stream_ctx.session_id),
            event,
        )
        .await
        {
            Ok(hints) => hints,
            Err(error) => {
                warn!(
                    session_id = %stream_ctx.session_id,
                    tool = %event.name,
                    error = %error,
                    "Lua tool:display_complete hook error, falling back to default metadata"
                );
                None
            }
        }
    };
    if session_hints.is_some() {
        return session_hints;
    }
    let (plugin_registry, plugin_lua) = stream_ctx.agent_stream_config.plugin_handlers.as_ref()?;
    match execute_tool_display_complete_hooks(
        plugin_lua,
        plugin_registry,
        Some(&stream_ctx.session_id),
        event,
    )
    .await
    {
        Ok(hints) => hints,
        Err(error) => {
            warn!(
                session_id = %stream_ctx.session_id,
                tool = %event.name,
                error = %error,
                "plugin tool:display_complete hook error, falling back to default metadata"
            );
            None
        }
    }
}

/// The `tool_result` seam: chained partial patches over a finished tool
/// call's outcome, as the MODEL will receive it.
///
/// Handlers get `{ tool, args, result, error }` and return a `Transform` of
/// `{ result = <string> }` and/or `{ error = <string> }` — partial: an
/// omitted key keeps the current value, and each handler sees the previous
/// handlers' patches (session VM first, then plugin VM). Use cases:
/// redacting secrets from bash output, summarising a large read. Execution
/// already happened, so Cancel/Handled have nothing to act on and are
/// ignored; handler errors fail open like every non-gate hook — a redactor
/// that must be able to veto belongs in `pre_tool_call`, before execution.
pub(super) async fn apply_tool_result_handlers(
    stream_ctx: &StreamContext,
    tool_name: &str,
    args: &serde_json::Value,
    mut result: String,
    mut error: Option<String>,
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

    {
        let state = stream_ctx.session_state.lock().await;
        run_pass(
            stream_ctx,
            &state.registry,
            &state.lua,
            tool_name,
            args,
            &mut result,
            &mut error,
        )
        .await;
    }
    if let Some((plugin_registry, plugin_lua)) =
        stream_ctx.agent_stream_config.plugin_handlers.as_ref()
    {
        run_pass(
            stream_ctx,
            plugin_registry,
            plugin_lua,
            tool_name,
            args,
            &mut result,
            &mut error,
        )
        .await;
    }
    (result, error)
}

pub(super) async fn resolve_before_execute_env(
    stream_ctx: &StreamContext,
    event: &ToolBeforeExecuteEvent,
) -> std::collections::HashMap<String, String> {
    let session_env = {
        let state = stream_ctx.session_state.lock().await;
        match execute_tool_before_execute_hooks(
            &state.lua,
            &state.registry,
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
                    "Lua tool:before_execute hook error, proceeding without env vars"
                );
                std::collections::HashMap::new()
            }
        }
    };
    let mut env = match stream_ctx.agent_stream_config.plugin_handlers.as_ref() {
        Some((plugin_registry, plugin_lua)) => {
            match execute_tool_before_execute_hooks(
                plugin_lua,
                plugin_registry,
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
                        "plugin tool:before_execute hook error, proceeding without env vars"
                    );
                    std::collections::HashMap::new()
                }
            }
        }
        None => std::collections::HashMap::new(),
    };
    env.extend(session_env);
    env
}
