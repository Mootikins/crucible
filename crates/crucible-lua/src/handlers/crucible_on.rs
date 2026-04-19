use mlua::{Lua, RegistryKey, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::registry::RuntimeHandler;

/// Register the crucible.on() API for runtime handler registration
///
/// Supports two calling conventions:
///
/// ```lua
/// -- Simple (backward compatible):
/// crucible.on("pre_tool_call", function(ctx, event) ... end)
///
/// -- With options (pattern + priority):
/// crucible.on("pre_tool_call", { pattern = "bash", priority = 50 }, function(ctx, event) ... end)
/// ```
pub fn register_crucible_on_api(
    lua: &Lua,
    runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
    handler_functions: Arc<Mutex<HashMap<String, RegistryKey>>>,
) -> LuaResult<()> {
    let crucible: Table = match lua.globals().get("crucible") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            lua.globals().set("crucible", t.clone())?;
            t
        }
    };

    let handlers = runtime_handlers.clone();
    let functions = handler_functions.clone();
    let on_fn = lua.create_function(move |lua, args: mlua::MultiValue| {
        let args_vec: Vec<Value> = args.into_vec();
        if args_vec.len() < 2 {
            return Err(mlua::Error::RuntimeError(
                "crucible.on requires at least 2 arguments: (event_type, handler) or (event_type, opts, handler)".into(),
            ));
        }

        let event_type: String = match &args_vec[0] {
            Value::String(s) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "crucible.on: first argument must be a string (event type)".into(),
                ))
            }
        };

        let (pattern, priority, handler) = match &args_vec[1] {
            Value::Function(f) => {
                // crucible.on(event_type, handler) — backward compatible
                (None, 100i64, f.clone())
            }
            Value::Table(opts) => {
                // crucible.on(event_type, opts, handler)
                if args_vec.len() < 3 {
                    return Err(mlua::Error::RuntimeError(
                        "crucible.on: when second argument is a table, third argument must be the handler function".into(),
                    ));
                }
                let handler = match &args_vec[2] {
                    Value::Function(f) => f.clone(),
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "crucible.on: third argument must be a function".into(),
                        ))
                    }
                };
                let pattern: Option<String> = opts.get("pattern").ok();
                let priority: i64 = opts.get("priority").unwrap_or(100);
                (pattern, priority, handler)
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "crucible.on: second argument must be a function or options table".into(),
                ))
            }
        };

        let mut guard = handlers
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock handlers: {}", e)))?;

        let name = format!("runtime_handler_{}", guard.len());
        guard.push(RuntimeHandler {
            event_type: event_type.clone(),
            name: name.clone(),
            priority,
            pattern: pattern.clone(),
        });

        let key = lua.create_registry_value(handler)?;
        let mut func_guard = functions
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock functions: {}", e)))?;
        func_guard.insert(name.clone(), key);

        debug!(
            "Registered runtime handler '{}' for event '{}' (priority={}, pattern={:?})",
            name, event_type, priority, pattern
        );
        Ok(())
    })?;

    crucible.set("on", on_fn)?;
    Ok(())
}
