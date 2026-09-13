use super::{LifecycleError, LifecycleResult};
use crate::command_effect::CommandEffect;
use crate::discovered::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredService, DiscoveredTool,
};
use mlua::{Table, Value};
use std::path::Path;

/// The declarations in the table a plugin's `init.luau` returns.
///
/// Tools, commands, handlers, services and `setup`. A field the table does
/// not hold is empty. The plugin's metadata (name, version, description,
/// author, license, the intercept grant) is not here: the fragment carries
/// it, see [`super::Fragment`].
#[derive(Debug, Clone, Default)]
pub struct PluginSpec {
    pub tools: Vec<DiscoveredTool>,
    pub commands: Vec<DiscoveredCommand>,
    pub handlers: Vec<DiscoveredHandler>,
    pub services: Vec<DiscoveredService>,
    pub has_setup: bool,
    /// Where the plugin was discovered from (user, runtime, kiln, etc.)
    pub source: Option<String>,
}

/// Refuse a parameter whose declared type the host cannot read.
///
/// The type text is not decoration: it becomes the JSON Schema an agent sees,
/// the Luau declaration a plugin is checked against, and the control a
/// generated argument dialog draws. An unreadable declaration used to become
/// `"type": "string"` in the schema and `any` in the stub — two different
/// wrong answers, neither of which the author was told about. Now the plugin
/// does not load, and the message names the declaration, the parameter and the
/// text.
///
/// `kind` is `tool` or `command`. Both reach the same schema, so both are
/// checked; only the tool half used to be.
fn validate_declared_types(
    kind: &str,
    name: &str,
    params: &[DiscoveredParam],
) -> LifecycleResult<()> {
    for param in params {
        if let Err(error) = crate::signature::LuaType::parse(&param.param_type) {
            return Err(LifecycleError::InvalidDeclaration(format!(
                "{kind} '{name}', parameter '{}': {error}. Declare one of: \
                 string, number, boolean, any, a name, `T?`, `T[]`, \
                 `array<T>`, `table<K, V>`, `T|U`, or `{{ field: T }}`",
                param.name
            )));
        }
    }
    Ok(())
}

/// Read a command's declared effect, or refuse the load.
///
/// Three cases, and only one of them is a fallback:
///
/// - declared and readable — that effect;
/// - declared and unreadable (`effect = "raed"`) — **refused**, exactly as an
///   unreadable parameter type is. Falling back would answer `Write` to an
///   author who was trying to say `read`, and say nothing;
/// - absent — [`CommandEffect::Write`]. Every command written before this
///   field existed lands here, and the conservative answer costs a question
///   while the permissive one costs a file. `CommandEffect` derives no
///   `Default`, so this is the single place that choice is made.
fn command_effect(command: &str, def: &mlua::Table) -> LifecycleResult<CommandEffect> {
    match def.get::<Value>("effect") {
        Ok(Value::String(text)) => {
            let text = text.to_string_lossy();
            CommandEffect::parse(&text).ok_or_else(|| {
                LifecycleError::InvalidDeclaration(format!(
                    "command '{command}': cannot read the effect '{text}'. Declare one of: {}",
                    CommandEffect::declarable()
                ))
            })
        }
        Ok(Value::Nil) | Err(_) => Ok(CommandEffect::Write),
        Ok(other) => Err(LifecycleError::InvalidDeclaration(format!(
            "command '{command}': `effect` must be a string, got {}. Declare one of: {}",
            other.type_name(),
            CommandEffect::declarable()
        ))),
    }
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

/// Read the declarations out of the table `init.luau` returned.
///
/// The table is the one the daemon VM holds after the plugin ran, so the
/// `fn` values in it are live. This function reads only the declarations
/// and refuses one the host cannot read (`validate_declared_types`,
/// `command_effect`). `source_path` names the file in each declaration.
///
/// Every table is read. A table with none of the declaration keys is an
/// empty spec: a module that declares nothing is a plugin all the same.
pub fn spec_from_table(table: &Table, source_path: &Path) -> LifecycleResult<PluginSpec> {
    let source_path_str = source_path.to_string_lossy().to_string();
    let mut spec = PluginSpec::default();

    // Extract tools
    if let Ok(Value::Table(tools_table)) = table.get::<Value>("tools") {
        for pair in tools_table.pairs::<String, Value>() {
            if let Ok((tool_name, Value::Table(tool_def))) = pair {
                let desc = tool_def.get::<String>("desc").unwrap_or_default();

                let params = extract_params_from_table(&tool_def);
                validate_declared_types("tool", &tool_name, &params)?;

                spec.tools.push(DiscoveredTool {
                    name: tool_name,
                    description: desc,
                    params,
                    return_type: None,
                    source_path: source_path_str.clone(),
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

                // Extract params if present. A command's `params` and its
                // `hint` are not alternatives: the hint is one line of free
                // text for a person to read, the params are the typed
                // declaration a client generates a dialog from.
                let params = extract_params_from_table(&cmd_def);
                validate_declared_types("command", &cmd_name, &params)?;
                let effect = command_effect(&cmd_name, &cmd_def)?;

                spec.commands.push(DiscoveredCommand {
                    name: cmd_name.clone(),
                    description: desc,
                    params,
                    input_hint: hint,
                    effect,
                    source_path: source_path_str.clone(),
                    handler_fn: cmd_name,
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
                    });
                }
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

    Ok(spec)
}
