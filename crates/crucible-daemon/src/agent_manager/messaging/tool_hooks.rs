//! Display/before-execute hook resolution for tool calls.
//!
//! Each resolver runs the session VM's handlers first (under the session
//! state lock), then the plugin VM's with the lock released — plugin Lua may
//! run for seconds and must not hold the session's whole state hostage.
//! Display hints are first-non-empty-wins; env maps merge with session
//! entries winning a key collision (the more specific scope overrides).

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
        match execute_tool_display_start_hooks(&state.lua, &state.registry, event).await {
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
    match execute_tool_display_start_hooks(plugin_lua, plugin_registry, event).await {
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
        match execute_tool_display_complete_hooks(&state.lua, &state.registry, event).await {
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
    match execute_tool_display_complete_hooks(plugin_lua, plugin_registry, event).await {
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

pub(super) async fn resolve_before_execute_env(
    stream_ctx: &StreamContext,
    event: &ToolBeforeExecuteEvent,
) -> std::collections::HashMap<String, String> {
    let session_env = {
        let state = stream_ctx.session_state.lock().await;
        match execute_tool_before_execute_hooks(&state.lua, &state.registry, event).await {
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
            match execute_tool_before_execute_hooks(plugin_lua, plugin_registry, event).await {
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
