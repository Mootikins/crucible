use crate::error::LuaError;
use crate::{
    register_graph_module, register_mcp_module_stub, register_oq_module, register_paths_module,
    register_popup_module, register_sessions_module, register_statusline_module,
    register_tools_module, register_ui_module, register_vault_module, LuaExecutor, PathsContext,
};
use mlua::{Lua, Table, Value};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

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
        let tmp_dir = std::env::temp_dir().join(format!("crucible-stubs-{}", std::process::id()));
        fs::create_dir_all(&tmp_dir)?;

        let result = (|| {
            Self::generate(&tmp_dir)?;

            let generated_lua = fs::read_to_string(tmp_dir.join("cru.lua"))?;
            let generated_docs = fs::read_to_string(tmp_dir.join("cru-docs.json"))?;

            let committed_docs = committed_path.with_file_name("cru-docs.json");
            if !committed_path.exists() || !committed_docs.exists() {
                return Ok(false);
            }

            let existing_lua = fs::read_to_string(committed_path)?;
            let existing_docs = fs::read_to_string(committed_docs)?;

            Ok(generated_lua == existing_lua && generated_docs == existing_docs)
        })();

        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }
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

    let mut class_paths = BTreeSet::new();
    class_paths.insert("cru".to_string());

    let mut functions = Vec::new();
    for module in UNIVERSAL_MODULES {
        if let Ok(Value::Table(table)) = cru.get::<Value>(*module) {
            collect_function_stubs(
                &table,
                &format!("cru.{}", module),
                false,
                &mut functions,
                &mut class_paths,
            )?;
        }
    }
    for module in UI_ONLY_MODULES {
        if let Ok(Value::Table(table)) = cru.get::<Value>(*module) {
            collect_function_stubs(
                &table,
                &format!("cru.{}", module),
                true,
                &mut functions,
                &mut class_paths,
            )?;
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
    for function in &functions {
        let documentation = format!("Lua API function {}", function.path);

        out.push_str("--- ");
        out.push_str(&documentation);
        out.push('\n');

        if function.ui_only {
            out.push_str("---@note ");
            out.push_str(UI_NOTE);
            out.push('\n');
        }

        out.push_str("---@param ... any\n");
        out.push_str("---@return any\n");
        out.push_str(&format!("function {}(...) end\n\n", function.path));

        docs.insert(function.path.clone(), DocEntry { documentation });
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
