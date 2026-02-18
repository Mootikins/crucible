use crate::annotations::DiscoveredParam;
use crate::error::LuaError;
use crate::schema::LuauType;
use crate::{
    register_graph_module, register_mcp_module_stub, register_oq_module, register_paths_module,
    register_popup_module, register_sessions_module, register_statusline_module,
    register_tools_module, register_ui_module, register_vault_module, LuaExecutor, PathsContext,
};
use mlua::{Lua, Table, Value};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const UNIVERSAL_MODULES: &[&str] = &[
    "kiln",
    "graph",
    "http",
    "fs",
    "session",
    "sessions",
    "tools",
    "oq",
    "paths",
    "timer",
    "ratelimit",
    "mcp",
    "hooks",
    "notify",
    "ask",
];

const UI_ONLY_MODULES: &[&str] = &["oil", "popup", "panel", "interaction", "statusline"];

const UI_NOTE: &str = "UI-only: requires TUI context, not available in daemon plugins";

#[derive(Debug, Clone)]
struct ReturnInfo {
    ty: &'static str,
    name: &'static str,
    description: &'static str,
}

#[derive(Debug, Clone)]
struct StubSignature {
    documentation: &'static str,
    params: Vec<DiscoveredParam>,
    returns: Vec<ReturnInfo>,
}

#[derive(Debug, Clone)]
struct FunctionStub {
    path: String,
    ui_only: bool,
}

#[derive(Debug, Serialize)]
struct DocEntry {
    documentation: String,
}

pub struct StubGenerator;

impl StubGenerator {
    pub fn generate(output_dir: &Path) -> Result<(), LuaError> {
        fs::create_dir_all(output_dir)?;

        let executor = LuaExecutor::new()?;
        let lua = executor.lua();

        register_oq_module(lua)?;
        register_paths_module(lua, PathsContext::new())?;
        register_graph_module(lua)?;
        register_vault_module(lua)?;
        register_sessions_module(lua)?;
        register_tools_module(lua)?;
        register_mcp_module_stub(lua)?;
        register_popup_module(lua)?;
        register_ui_module(lua)?;
        register_statusline_module(lua)?;

        mirror_modules_into_cru(lua)?;

        let (emmylua, docs) = render_stubs(lua)?;

        fs::write(output_dir.join("cru.lua"), emmylua)?;
        let docs_json = serde_json::to_string_pretty(&docs)
            .map_err(|e| LuaError::Serialization(e.to_string()))?;
        fs::write(output_dir.join("cru-docs.json"), docs_json)?;

        Ok(())
    }

    pub fn verify(committed_path: &Path) -> Result<bool, LuaError> {
        let tmp_dir = temp_output_dir();
        fs::create_dir_all(&tmp_dir)?;

        let result = Self::generate(&tmp_dir).and_then(|_| {
            let generated_lua = fs::read_to_string(tmp_dir.join("cru.lua"))?;
            let generated_docs = fs::read_to_string(tmp_dir.join("cru-docs.json"))?;

            let committed_docs = committed_path.with_file_name("cru-docs.json");
            if !committed_path.exists() || !committed_docs.exists() {
                return Ok(false);
            }

            let existing_lua = fs::read_to_string(committed_path)?;
            let existing_docs = fs::read_to_string(committed_docs)?;

            Ok(generated_lua == existing_lua && generated_docs == existing_docs)
        });

        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }
}

fn temp_output_dir() -> PathBuf {
    let unique = format!(
        "crucible-stubs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    std::env::temp_dir().join(unique)
}

fn mirror_modules_into_cru(lua: &Lua) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let crucible: Table = globals.get("crucible")?;

    copy_global_table(&globals, &cru, "ask", "ask")?;
    copy_global_table(&globals, &cru, "graph", "graph")?;
    copy_global_table(&globals, &cru, "oq", "oq")?;
    copy_global_table(&globals, &cru, "paths", "paths")?;
    copy_global_table(&globals, &cru, "mcp", "mcp")?;
    copy_global_table(&globals, &cru, "popup", "popup")?;
    copy_global_table(&globals, &cru, "statusline", "statusline")?;
    copy_global_table(&globals, &cru, "ui", "panel")?;

    if let Ok(get_session) = cru.get::<Value>("get_session") {
        if matches!(get_session, Value::Function(_)) {
            let session = lua.create_table()?;
            session.set("get", get_session)?;
            cru.set("session", session)?;
        }
    }

    let hooks = lua.create_table()?;
    if let Ok(f) = crucible.get::<Value>("on_session_start") {
        if matches!(f, Value::Function(_)) {
            hooks.set("on_session_start", f)?;
        }
    }
    if let Ok(f) = crucible.get::<Value>("on_tools_registered") {
        if matches!(f, Value::Function(_)) {
            hooks.set("on_tools_registered", f)?;
        }
    }
    if hooks.pairs::<Value, Value>().next().is_some() {
        cru.set("hooks", hooks)?;
    }

    let notify = lua.create_table()?;
    if let Ok(f) = crucible.get::<Value>("notify") {
        if matches!(f, Value::Function(_)) {
            notify.set("notify", f)?;
        }
    }
    if let Ok(f) = crucible.get::<Value>("notify_once") {
        if matches!(f, Value::Function(_)) {
            notify.set("notify_once", f)?;
        }
    }
    if let Ok(messages) = crucible.get::<Value>("messages") {
        if matches!(messages, Value::Table(_)) {
            notify.set("messages", messages)?;
        }
    }
    if notify.pairs::<Value, Value>().next().is_some() {
        cru.set("notify", notify)?;
    }

    Ok(())
}

fn copy_global_table(
    globals: &Table,
    cru: &Table,
    source: &str,
    target: &str,
) -> Result<(), LuaError> {
    if let Ok(value) = globals.get::<Value>(source) {
        if matches!(value, Value::Table(_)) {
            cru.set(target, value)?;
        }
    }
    Ok(())
}

fn render_stubs(lua: &Lua) -> Result<(String, BTreeMap<String, DocEntry>), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let signatures = known_signatures();

    let mut class_paths = BTreeSet::new();
    class_paths.insert("cru".to_string());

    let mut functions = Vec::new();
    for module in UNIVERSAL_MODULES {
        if let Ok(value) = cru.get::<Value>(*module) {
            if let Value::Table(table) = value {
                collect_function_stubs(
                    &table,
                    &format!("cru.{}", module),
                    UI_ONLY_MODULES.contains(module),
                    &mut functions,
                    &mut class_paths,
                )?;
            }
        }
    }
    for module in UI_ONLY_MODULES {
        if let Ok(value) = cru.get::<Value>(*module) {
            if let Value::Table(table) = value {
                collect_function_stubs(
                    &table,
                    &format!("cru.{}", module),
                    true,
                    &mut functions,
                    &mut class_paths,
                )?;
            }
        }
    }

    functions.sort_by(|a, b| a.path.cmp(&b.path));

    let mut out = String::new();
    out.push_str("error('Cannot require a meta file')\n\n");
    out.push_str("---@class cru\n");
    out.push_str("cru = {}\n\n");

    for class_path in class_paths.iter().filter(|p| p.as_str() != "cru") {
        if is_ui_only_path(class_path) {
            out.push_str("---@note ");
            out.push_str(UI_NOTE);
            out.push('\n');
        }
        out.push_str("---@class ");
        out.push_str(class_path);
        out.push('\n');
        out.push_str(class_path);
        out.push_str(" = {}\n\n");
    }

    let mut docs = BTreeMap::new();
    for function in functions {
        let signature = signatures.get(function.path.as_str());
        let documentation = signature
            .map(|s| s.documentation.to_string())
            .unwrap_or_else(|| format!("Lua API function {}", function.path));

        out.push_str("--- ");
        out.push_str(&documentation);
        out.push('\n');

        if function.ui_only {
            out.push_str("---@note ");
            out.push_str(UI_NOTE);
            out.push('\n');
        }

        if let Some(sig) = signature {
            for param in &sig.params {
                let ty = to_emmylua_type(&param.param_type);
                let optional = if param.optional { "?" } else { "" };
                out.push_str(&format!(
                    "---@param {}{} {} {}\n",
                    param.name, optional, ty, param.description
                ));
            }

            if sig.returns.is_empty() {
                out.push_str("---@return any\n");
            } else {
                for ret in &sig.returns {
                    let ty = to_emmylua_type(ret.ty);
                    if ret.name.is_empty() {
                        out.push_str(&format!("---@return {} {}\n", ty, ret.description));
                    } else {
                        out.push_str(&format!(
                            "---@return {} {} {}\n",
                            ty, ret.name, ret.description
                        ));
                    }
                }
            }

            let args = sig
                .params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("function {}({}) end\n\n", function.path, args));
        } else {
            out.push_str("---@param ... any\n");
            out.push_str("---@return any\n");
            out.push_str(&format!("function {}(...) end\n\n", function.path));
        }

        docs.insert(function.path, DocEntry { documentation });
    }

    Ok((out, docs))
}

fn collect_function_stubs(
    table: &Table,
    base_path: &str,
    ui_only: bool,
    functions: &mut Vec<FunctionStub>,
    class_paths: &mut BTreeSet<String>,
) -> Result<(), LuaError> {
    class_paths.insert(base_path.to_string());

    let mut keys = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key_str) = key else {
            continue;
        };

        let key_text = key_str
            .to_str()
            .map_err(|e| LuaError::Runtime(e.to_string()))?
            .to_string();

        keys.push((key_text, value));
    }

    keys.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, value) in keys {
        if key.starts_with("__") {
            continue;
        }

        let path = format!("{}.{}", base_path, key);
        match value {
            Value::Function(_) => functions.push(FunctionStub { path, ui_only }),
            Value::Table(sub_table) => {
                collect_function_stubs(&sub_table, &path, ui_only, functions, class_paths)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn is_ui_only_path(path: &str) -> bool {
    UI_ONLY_MODULES.iter().any(|module| {
        path == format!("cru.{module}") || path.starts_with(&format!("cru.{module}."))
    })
}

fn to_emmylua_type(raw: &str) -> String {
    if raw.contains('|') {
        return raw
            .split('|')
            .map(|part| to_emmylua_type(part.trim()))
            .collect::<Vec<_>>()
            .join("|");
    }

    let parsed = LuauType::from_ldoc(raw);
    emmylua_type_from_luau(&parsed)
}

fn emmylua_type_from_luau(ty: &LuauType) -> String {
    match ty {
        LuauType::Primitive { name } => name.clone(),
        LuauType::Optional { inner } => format!("{}|nil", emmylua_type_from_luau(inner)),
        LuauType::Array { element } => format!("{}[]", emmylua_type_from_luau(element)),
        LuauType::Table { .. } => "table".to_string(),
        LuauType::Union { types } => types
            .iter()
            .map(emmylua_type_from_luau)
            .collect::<Vec<_>>()
            .join("|"),
        LuauType::Named { name } => {
            if name == "object" {
                "table".to_string()
            } else {
                name.clone()
            }
        }
        LuauType::Function { .. } => "fun(...)".to_string(),
        LuauType::Any => "any".to_string(),
    }
}

fn known_signatures() -> HashMap<&'static str, StubSignature> {
    let mut signatures = HashMap::new();

    signatures.insert(
        "cru.kiln.search",
        StubSignature {
            documentation: "Search the knowledge base for notes",
            params: vec![
                param("query", "string", "The search query", false),
                param("opts", "table", "Optional parameters", true),
            ],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "results",
                description: "List of matching notes",
            }],
        },
    );

    signatures.insert(
        "cru.kiln.list",
        StubSignature {
            documentation: "List notes in the knowledge base",
            params: vec![param("limit", "number", "Maximum notes to return", true)],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "notes",
                description: "List of note records",
            }],
        },
    );

    signatures.insert(
        "cru.kiln.get",
        StubSignature {
            documentation: "Get a note by path",
            params: vec![param("path", "string", "Path to the note", false)],
            returns: vec![ReturnInfo {
                ty: "table|nil",
                name: "note",
                description: "Note record if found",
            }],
        },
    );

    signatures.insert(
        "cru.graph.find",
        StubSignature {
            documentation: "Find a note by title in an in-memory graph",
            params: vec![
                param("graph", "table", "Graph object", false),
                param("title", "string", "Note title", false),
            ],
            returns: vec![ReturnInfo {
                ty: "table|nil",
                name: "note",
                description: "Matched note table",
            }],
        },
    );

    signatures.insert(
        "cru.http.get",
        StubSignature {
            documentation: "Perform an HTTP GET request",
            params: vec![
                param("url", "string", "Request URL", false),
                param("opts", "table", "Optional request options", true),
            ],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "response",
                description: "HTTP response table",
            }],
        },
    );

    signatures.insert(
        "cru.fs.read",
        StubSignature {
            documentation: "Read a file into a string",
            params: vec![param("path", "string", "File path", false)],
            returns: vec![ReturnInfo {
                ty: "string",
                name: "content",
                description: "File contents",
            }],
        },
    );

    signatures.insert(
        "cru.session.get",
        StubSignature {
            documentation: "Get the active session object",
            params: vec![],
            returns: vec![ReturnInfo {
                ty: "any",
                name: "session",
                description: "Current session",
            }],
        },
    );

    signatures.insert(
        "cru.sessions.create",
        StubSignature {
            documentation: "Create a daemon session",
            params: vec![param("opts", "table", "Session options", false)],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "session",
                description: "Created session metadata",
            }],
        },
    );

    signatures.insert(
        "cru.tools.call",
        StubSignature {
            documentation: "Call a workspace tool directly",
            params: vec![
                param("name", "string", "Tool name", false),
                param("args", "table", "Tool arguments", false),
            ],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "result",
                description: "Tool response",
            }],
        },
    );

    signatures.insert(
        "cru.oq.parse",
        StubSignature {
            documentation: "Parse JSON/YAML/TOML/TOON input",
            params: vec![param("input", "string", "Source text to parse", false)],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "value",
                description: "Parsed object",
            }],
        },
    );

    signatures.insert(
        "cru.timer.sleep",
        StubSignature {
            documentation: "Sleep asynchronously for the given number of seconds",
            params: vec![param("seconds", "number", "Duration in seconds", false)],
            returns: vec![ReturnInfo {
                ty: "nil",
                name: "",
                description: "",
            }],
        },
    );

    signatures.insert(
        "cru.mcp.call",
        StubSignature {
            documentation: "Call a tool on an MCP server",
            params: vec![
                param("server", "string", "Server name", false),
                param("tool", "string", "Tool name", false),
                param("args", "table", "Tool arguments", false),
            ],
            returns: vec![ReturnInfo {
                ty: "table",
                name: "result",
                description: "MCP tool result",
            }],
        },
    );

    signatures.insert(
        "cru.hooks.on_session_start",
        StubSignature {
            documentation: "Register a callback fired on session start",
            params: vec![param("callback", "function", "Hook callback", false)],
            returns: vec![ReturnInfo {
                ty: "nil",
                name: "",
                description: "",
            }],
        },
    );

    signatures.insert(
        "cru.notify.notify",
        StubSignature {
            documentation: "Queue a user notification",
            params: vec![
                param("message", "string", "Notification text", false),
                param("level", "number", "Optional severity", true),
                param("opts", "table", "Optional metadata", true),
            ],
            returns: vec![ReturnInfo {
                ty: "nil",
                name: "",
                description: "",
            }],
        },
    );

    signatures.insert(
        "cru.ask.question",
        StubSignature {
            documentation: "Create an interactive question",
            params: vec![
                param("header", "string", "Question header", false),
                param("text", "string", "Question body", false),
            ],
            returns: vec![ReturnInfo {
                ty: "any",
                name: "question",
                description: "Question builder object",
            }],
        },
    );

    signatures
}

fn param(name: &str, param_type: &str, description: &str, optional: bool) -> DiscoveredParam {
    DiscoveredParam {
        name: name.to_string(),
        param_type: param_type.to_string(),
        description: description.to_string(),
        optional,
    }
}
