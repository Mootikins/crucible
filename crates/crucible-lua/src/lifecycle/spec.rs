use super::{LifecycleError, LifecycleResult};
use crate::annotations::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredService, DiscoveredTool,
    DiscoveredView,
};
use crate::error::format_lua_error;
use crate::manifest::Capability;
use mlua::{Lua, Value};
use std::path::Path;

/// Spec extracted from a plugin's returned Lua table.
///
/// When a plugin's `init.lua` returns a table, this struct captures the
/// declared metadata and exports. Fields that aren't present in the table
/// are left as `None`/empty.
#[derive(Debug, Clone, Default)]
pub struct PluginSpec {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    pub tools: Vec<DiscoveredTool>,
    pub commands: Vec<DiscoveredCommand>,
    pub handlers: Vec<DiscoveredHandler>,
    pub views: Vec<DiscoveredView>,
    pub services: Vec<DiscoveredService>,
    pub has_setup: bool,
    /// Where the plugin was discovered from (user, runtime, kiln, etc.)
    pub source: Option<String>,
}

/// Parse a capability string (from Lua spec) to a Capability enum.
pub(super) fn parse_capability(s: &str) -> Option<Capability> {
    match s.to_lowercase().as_str() {
        "filesystem" => Some(Capability::Filesystem),
        "network" => Some(Capability::Network),
        "shell" => Some(Capability::Shell),
        "kiln" => Some(Capability::Kiln),
        "agent" => Some(Capability::Agent),
        "ui" => Some(Capability::Ui),
        "config" => Some(Capability::Config),
        "system" => Some(Capability::System),
        "websocket" => Some(Capability::WebSocket),
        _ => None,
    }
}

/// Set up a permissive sandbox for spec extraction.
///
/// Stubs `require()`, `crucible`, `cru`, and `io` so that plugin init files
/// can be evaluated for their return table without crashing on missing runtime
/// dependencies. The stubs are no-ops — we only care about the spec table structure.
pub(super) fn setup_spec_sandbox(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
-- Stub require: return an empty table that tolerates any method call
local stub_mt = {}
stub_mt.__index = function() return function() return setmetatable({}, stub_mt) end end
stub_mt.__call = function() return setmetatable({}, stub_mt) end

local _real_require = require
require = function(name)
    local ok, mod = pcall(_real_require, name)
    if ok then return mod end
    return setmetatable({}, stub_mt)
end

-- Stub crucible namespace
crucible = setmetatable({}, stub_mt)

-- Stub cru namespace
cru = setmetatable({}, stub_mt)

-- Stub io (some plugins use io.open at load time)
if not io then io = setmetatable({}, stub_mt) end
"#,
    )
    .exec()?;
    Ok(())
}

/// Execute a plugin's init.lua and extract a PluginSpec from the returned table.
///
/// Returns `Ok(Some(spec))` if the script returns a table with recognized fields,
/// `Ok(None)` if it returns nil or a non-table value,
/// or `Err` if there's a Lua execution error.
pub fn load_plugin_spec(init_path: &Path) -> LifecycleResult<Option<PluginSpec>> {
    let source = std::fs::read_to_string(init_path).map_err(LifecycleError::Io)?;

    // Compile Fennel to Lua if needed
    let is_fennel = init_path.extension().is_some_and(|ext| ext == "fnl");

    if is_fennel {
        #[cfg(feature = "fennel")]
        {
            let lua_source = crate::fennel::compile_fennel(&source).map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Fennel compilation failed for {}: {}",
                    init_path.display(),
                    e
                ))
            })?;
            return load_plugin_spec_from_source(&lua_source, init_path);
        }
        #[cfg(not(feature = "fennel"))]
        {
            return Err(LifecycleError::LoadError(format!(
                "Fennel file {} requires the 'fennel' feature",
                init_path.display()
            )));
        }
    }

    load_plugin_spec_from_source(&source, init_path)
}

/// Extract `DiscoveredParam` entries from a Lua params table.
fn extract_params_from_table(def: &mlua::Table) -> Vec<DiscoveredParam> {
    let mut params = Vec::new();
    if let Ok(Value::Table(params_table)) = def.get::<Value>("params") {
        for i in 1..=params_table.raw_len() {
            if let Ok(Value::Table(param_def)) = params_table.get::<Value>(i) {
                params.push(DiscoveredParam {
                    name: param_def.get::<String>("name").unwrap_or_default(),
                    param_type: param_def
                        .get::<String>("type")
                        .unwrap_or_else(|_| "string".to_string()),
                    description: param_def.get::<String>("desc").unwrap_or_default(),
                    optional: param_def.get::<bool>("optional").unwrap_or(false),
                });
            }
        }
    }
    params
}

/// Extract a PluginSpec from Lua source code. Exposed for testing.
pub fn load_plugin_spec_from_source(
    source: &str,
    source_path: &Path,
) -> LifecycleResult<Option<PluginSpec>> {
    let lua = Lua::new();
    let source_path_str = source_path.to_string_lossy().to_string();
    let is_fennel = source_path.extension().is_some_and(|ext| ext == "fnl");

    // Set up a permissive environment so plugins that use require(), crucible.*,
    // cru.*, io.*, etc. don't crash before we can read their spec table.
    setup_spec_sandbox(&lua)
        .map_err(|e| LifecycleError::LoadError(format!("Failed to set up spec sandbox: {}", e)))?;

    // Execute the source and capture the return value
    let result: Value = lua
        .load(source)
        .set_name(source_path_str.as_str())
        .eval()
        .map_err(|e| LifecycleError::LoadError(format_lua_error(None, &e)))?;

    let table = match result {
        Value::Table(t) => t,
        Value::Nil => return Ok(None),
        _ => return Ok(None),
    };

    // Determine if this is a spec table vs a plain module table.
    // A spec table has at least one recognized declarative field.
    let spec_fields = [
        "name", "version", "tools", "commands", "handlers", "views", "setup",
    ];
    let has_spec_field = spec_fields
        .iter()
        .any(|&field| !matches!(table.get::<Value>(field), Ok(Value::Nil) | Err(_)));

    if !has_spec_field {
        // Plain module table (e.g., `local M = {}; return M`) — not a spec
        return Ok(None);
    }

    let mut spec = PluginSpec {
        name: table.get::<String>("name").ok(),
        version: table.get::<String>("version").ok(),
        description: table.get::<String>("description").ok(),
        ..Default::default()
    };

    // Extract capabilities
    if let Ok(Value::Table(caps)) = table.get::<Value>("capabilities") {
        for i in 1..=caps.raw_len() {
            if let Ok(s) = caps.get::<String>(i) {
                spec.capabilities.push(s);
            }
        }
    }

    // Extract tools
    if let Ok(Value::Table(tools_table)) = table.get::<Value>("tools") {
        for pair in tools_table.pairs::<String, Value>() {
            if let Ok((tool_name, Value::Table(tool_def))) = pair {
                let desc = tool_def.get::<String>("desc").unwrap_or_default();

                let params = extract_params_from_table(&tool_def);

                spec.tools.push(DiscoveredTool {
                    name: tool_name,
                    description: desc,
                    params,
                    return_type: None,
                    source_path: source_path_str.clone(),
                    is_fennel,
                });
            }
        }
    }

    // Extract commands
    if let Ok(Value::Table(cmds_table)) = table.get::<Value>("commands") {
        for pair in cmds_table.pairs::<String, Value>() {
            if let Ok((cmd_name, Value::Table(cmd_def))) = pair {
                let desc = cmd_def.get::<String>("desc").unwrap_or_default();
                let hint = cmd_def.get::<String>("hint").ok();

                // Extract params if present
                let params = extract_params_from_table(&cmd_def);

                spec.commands.push(DiscoveredCommand {
                    name: cmd_name.clone(),
                    description: desc,
                    params,
                    input_hint: hint,
                    source_path: source_path_str.clone(),
                    handler_fn: cmd_name,
                    is_fennel,
                });
            }
        }
    }

    // Extract handlers
    if let Ok(Value::Table(handlers_table)) = table.get::<Value>("handlers") {
        for i in 1..=handlers_table.raw_len() {
            if let Ok(Value::Table(handler_def)) = handlers_table.get::<Value>(i) {
                let event = handler_def.get::<String>("event").unwrap_or_default();
                let priority = handler_def.get::<i64>("priority").unwrap_or(100);
                let pattern = handler_def
                    .get::<String>("pattern")
                    .unwrap_or_else(|_| "*".to_string());
                let name = handler_def
                    .get::<String>("name")
                    .unwrap_or_else(|_| format!("handler_{}", i));
                let desc = handler_def.get::<String>("desc").unwrap_or_default();

                if !event.is_empty() {
                    spec.handlers.push(DiscoveredHandler {
                        name: name.clone(),
                        event_type: event,
                        pattern,
                        priority,
                        description: desc,
                        source_path: source_path_str.clone(),
                        handler_fn: name,
                        is_fennel,
                    });
                }
            }
        }
    }

    // Extract views
    if let Ok(Value::Table(views_table)) = table.get::<Value>("views") {
        for pair in views_table.pairs::<String, Value>() {
            if let Ok((view_name, Value::Table(view_def))) = pair {
                let desc = view_def.get::<String>("desc").unwrap_or_default();
                // Check if handler fn is present (it's a Lua function, so we just check for non-nil)
                let has_handler =
                    matches!(view_def.get::<Value>("handler"), Ok(Value::Function(_)));

                spec.views.push(DiscoveredView {
                    name: view_name.clone(),
                    description: desc,
                    source_path: source_path_str.clone(),
                    view_fn: view_name.clone(),
                    handler_fn: if has_handler {
                        Some(format!("{}_handler", view_name))
                    } else {
                        None
                    },
                    is_fennel,
                });
            }
        }
    }

    // Extract services
    if let Ok(Value::Table(services_table)) = table.get::<Value>("services") {
        for pair in services_table.pairs::<String, Value>() {
            if let Ok((service_name, Value::Table(service_def))) = pair {
                let desc = service_def.get::<String>("desc").unwrap_or_default();
                let has_fn = matches!(service_def.get::<Value>("fn"), Ok(Value::Function(_)));
                if has_fn {
                    spec.services.push(DiscoveredService {
                        name: service_name.clone(),
                        description: desc,
                        source_path: source_path_str.clone(),
                        service_fn: service_name,
                    });
                }
            }
        }
    }

    // Check for setup function
    spec.has_setup = matches!(table.get::<Value>("setup"), Ok(Value::Function(_)));

    Ok(Some(spec))
}
