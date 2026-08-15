use mlua::{Function, Lua, LuaSerdeExt, Result as LuaResult, Value};
use serde_json::Value as JsonValue;

use super::registry::LuaScriptHandlerRegistry;
use super::script_handler::{interpret_handler_result, ScriptHandlerResult};

// --- tool:before_execute hook ---

pub const TOOL_BEFORE_EXECUTE_EVENT: &str = "tool:before_execute";

#[derive(Debug, Clone)]
pub struct ToolBeforeExecuteEvent {
    pub name: String,
    pub args: serde_json::Value,
}

/// Environment variables to inject into tool execution (currently only consumed by bash).
#[derive(Debug, Clone, Default)]
pub struct ToolBeforeExecuteResult {
    pub env: std::collections::HashMap<String, String>,
}

/// Fire all registered `tool:before_execute` hooks.
///
/// Each hook receives `{ name, args, env }` where `env` starts empty.
/// Hooks can populate `env` with key-value pairs to inject into bash commands.
/// Multiple hooks accumulate env vars; later hooks override earlier ones for the same key.
pub async fn execute_tool_before_execute_hooks(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    session_id: Option<&str>,
    event: &ToolBeforeExecuteEvent,
) -> LuaResult<Option<ToolBeforeExecuteResult>> {
    let handlers = registry.runtime_handlers_for(TOOL_BEFORE_EXECUTE_EVENT, None);
    if handlers.is_empty() {
        return Ok(None);
    }

    let payload = serde_json::json!({
        "name": event.name,
        "args": event.args,
        "env": {},
    });

    let mut accumulated_env = std::collections::HashMap::new();

    for handler in handlers {
        match execute_runtime_json_handler(
            lua,
            registry,
            &handler.name,
            payload.clone(),
            session_id,
        )
        .await?
        {
            ScriptHandlerResult::Transform(result) => {
                if let Some(env_obj) = result.get("env").and_then(|v| v.as_object()) {
                    for (k, v) in env_obj {
                        if let Some(val) = v.as_str() {
                            accumulated_env.insert(k.clone(), val.to_string());
                        }
                    }
                }
            }
            ScriptHandlerResult::PassThrough
            | ScriptHandlerResult::Cancel { .. }
            | ScriptHandlerResult::Inject { .. }
            | ScriptHandlerResult::Handled { .. } => {}
        }
    }

    if accumulated_env.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ToolBeforeExecuteResult {
            env: accumulated_env,
        }))
    }
}

pub(super) async fn execute_runtime_json_handler(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    name: &str,
    payload: JsonValue,
    session_id: Option<&str>,
) -> LuaResult<ScriptHandlerResult> {
    let handler: Function = {
        let handler_functions = registry
            .handler_functions
            .lock()
            .expect("handler_functions: poisoned while executing Lua display handler");
        let Some(key) = handler_functions.get(name) else {
            // Same contract as `execute_runtime_handler`: a handler
            // unregistered mid-dispatch (plugin reload in flight) has no
            // opinion, and must not surface as a fail-closed error.
            tracing::debug!(handler = %name, "handler unregistered mid-dispatch; passing through");
            return Ok(ScriptHandlerResult::PassThrough);
        };
        lua.registry_value(key)?
    };

    let ctx_table = lua.create_table()?;
    // Same contract as execute_runtime_handler: a handler registered once at
    // plugin load serves every session and keys per-session state by this.
    if let Some(id) = session_id {
        ctx_table.set("session_id", id)?;
    }
    let payload_val = lua.to_value(&payload)?;
    let result: Value = handler.call_async((ctx_table, payload_val)).await?;

    interpret_handler_result(&result)
}
