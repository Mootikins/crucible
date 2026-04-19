use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Result of permission hook execution
///
/// Represents the possible outcomes from a Lua permission hook:
/// - Allow: Skip prompt and allow the tool execution
/// - Deny: Skip prompt and deny the tool execution
/// - Prompt: Show normal permission prompt (hook returned nil or other)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionHookResult {
    /// Hook returned `{allow=true}` - skip prompt and allow
    Allow,
    /// Hook returned `{deny=true}` - skip prompt and deny
    Deny,
    /// Hook returned nil or other - show normal prompt
    Prompt,
}

/// A permission request passed to Lua hooks
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// Tool name (e.g., "write", "bash")
    pub tool_name: String,
    /// Tool arguments as JSON
    pub args: JsonValue,
    /// File path if applicable
    pub file_path: Option<String>,
}

/// Stored permission hook callback
pub struct PermissionHook {
    /// Handler name for debugging
    pub name: String,
}

/// Register the crucible.permissions.on_request() API for permission hooks
///
/// This allows Lua scripts to register callbacks that fire before permission prompts:
///
/// ```lua
/// crucible.permissions.on_request(function(request)
///     -- request.tool_name, request.args, request.file_path
///     if request.tool_name == "bash" and string.match(request.args.command, "^npm ") then
///         return {allow=true}  -- Auto-allow npm commands
///     end
///     return nil  -- Show normal prompt
/// end)
/// ```
pub fn register_permission_hook_api(
    lua: &Lua,
    permission_hooks: Arc<Mutex<Vec<PermissionHook>>>,
    permission_functions: Arc<Mutex<HashMap<String, RegistryKey>>>,
) -> LuaResult<()> {
    let crucible: Table = match lua.globals().get("crucible") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            lua.globals().set("crucible", t.clone())?;
            t
        }
    };

    let permissions: Table = match crucible.get("permissions") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            crucible.set("permissions", t.clone())?;
            t
        }
    };

    let hooks = permission_hooks.clone();
    let functions = permission_functions.clone();
    let on_request_fn = lua.create_function(move |lua, handler: Function| {
        let mut guard = hooks
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock hooks: {}", e)))?;

        let name = format!("permission_hook_{}", guard.len());
        guard.push(PermissionHook { name: name.clone() });

        let key = lua.create_registry_value(handler)?;
        let mut func_guard = functions
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock functions: {}", e)))?;
        func_guard.insert(name.clone(), key);

        debug!("Registered permission hook '{}'", name);
        Ok(())
    })?;

    permissions.set("on_request", on_request_fn)?;
    Ok(())
}

/// Execute permission hooks and return the result
///
/// Executes all registered permission hooks in order. The first hook to return
/// `{allow=true}` or `{deny=true}` wins. If all hooks return nil, returns `Prompt`.
///
/// # Arguments
/// * `lua` - The Lua state
/// * `hooks` - List of registered permission hooks
/// * `functions` - Map of hook names to registry keys
/// * `request` - The permission request to evaluate
///
/// # Returns
/// * `PermissionHookResult::Allow` - Hook returned `{allow=true}`
/// * `PermissionHookResult::Deny` - Hook returned `{deny=true}`
/// * `PermissionHookResult::Prompt` - All hooks returned nil or no hooks registered
///
/// Note: deliberately sync (not async). Permission decisions must be fast and cannot
/// call async APIs. The MutexGuards from the caller are not Send across await points.
pub fn execute_permission_hooks(
    lua: &Lua,
    hooks: &[PermissionHook],
    functions: &HashMap<String, RegistryKey>,
    request: &PermissionRequest,
) -> LuaResult<PermissionHookResult> {
    if hooks.is_empty() {
        return Ok(PermissionHookResult::Prompt);
    }

    let request_table = lua.create_table()?;
    request_table.set("tool_name", request.tool_name.as_str())?;
    request_table.set("args", lua.to_value(&request.args)?)?;
    if let Some(ref path) = request.file_path {
        request_table.set("file_path", path.as_str())?;
    }

    for hook in hooks {
        let key = match functions.get(&hook.name) {
            Some(k) => k,
            None => {
                warn!("Permission hook '{}' not found in registry", hook.name);
                continue;
            }
        };

        let handler: Function = lua.registry_value(key)?;
        let result: Value = handler.call(request_table.clone())?;

        match result {
            Value::Nil => {
                debug!("Permission hook '{}' returned nil, continuing", hook.name);
            }
            Value::Table(t) => {
                if t.get::<bool>("allow").unwrap_or(false) {
                    debug!("Permission hook '{}' returned allow=true", hook.name);
                    return Ok(PermissionHookResult::Allow);
                }
                if t.get::<bool>("deny").unwrap_or(false) {
                    debug!("Permission hook '{}' returned deny=true", hook.name);
                    return Ok(PermissionHookResult::Deny);
                }
                debug!(
                    "Permission hook '{}' returned table without allow/deny",
                    hook.name
                );
            }
            _ => {
                debug!(
                    "Permission hook '{}' returned unexpected type, treating as prompt",
                    hook.name
                );
            }
        }
    }

    Ok(PermissionHookResult::Prompt)
}
