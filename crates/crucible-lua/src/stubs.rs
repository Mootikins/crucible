use crate::error::LuaError;
use crate::{
    register_context_module_stub, register_graph_module, register_mcp_module_stub,
    register_oq_module, register_paths_module, register_sessions_module, register_tools_module,
    register_vault_module, LuaExecutor, PathsContext,
};

use mlua::{Lua, Table, Value};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Modules that exist only in the UI process, so daemon-side stubs would be
/// misleading. `popup`, `panel` and `statusline` used to be listed here and
/// stubbed — but they were registered nowhere in production, so autocomplete
/// advertised an API that did not exist. A stub for a nonexistent function is
/// worse than a stale doc: it looks authoritative.
const UI_ONLY_MODULES: &[&str] = &["oil", "interaction"];

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
    /// Write stubs describing `lua`.
    ///
    /// Callers that own the VM plugins run on should pass **that** VM — the
    /// point of the stubs is to describe what a plugin author can call, and a
    /// stand-in cannot know what the daemon registered. See
    /// `crucible-daemon/tests/plugin_stubs_contract.rs`, which is what holds
    /// the two together.
    pub fn generate_from(lua: &Lua, output_dir: &Path) -> Result<(), LuaError> {
        fs::create_dir_all(output_dir)?;

        let (emmylua, docs) = render_stubs(lua)?;

        fs::write(output_dir.join("cru.lua"), emmylua)?;
        let docs_json = serde_json::to_string_pretty(&docs)
            .map_err(|e| LuaError::Serialization(e.to_string()))?;
        fs::write(output_dir.join("cru-docs.json"), docs_json)?;

        Ok(())
    }

    /// Stubs for the modules this crate can register on its own.
    ///
    /// Necessarily a subset — the daemon registers a dozen more — so this is
    /// for this crate's own tests and for `verify`. Production goes through
    /// [`Self::generate_from`] with the plugin VM.
    pub fn generate(output_dir: &Path) -> Result<(), LuaError> {
        let executor = LuaExecutor::new()?;
        let lua = executor.lua();

        register_oq_module(lua)?;
        register_paths_module(lua, PathsContext::new())?;
        register_graph_module(lua)?;
        register_vault_module(lua)?;
        register_sessions_module(lua)?;
        register_context_module_stub(lua)?;
        register_tools_module(lua)?;
        register_mcp_module_stub(lua)?;

        Self::generate_from(lua, output_dir)
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

fn render_stubs(lua: &Lua) -> Result<(String, BTreeMap<String, DocEntry>), LuaError> {
    let cru: Table = lua.globals().get("cru")?;

    let mut class_paths = BTreeSet::new();
    class_paths.insert("cru".to_string());

    // Whatever is on `cru`, rather than a hardcoded list. The list was the
    // problem: it named six modules the VM does not have and missed twelve it
    // does, and every module registered after it was written was invisible.
    let mut modules: Vec<(String, Table)> = Vec::new();
    for pair in cru.pairs::<String, Value>() {
        let (name, value) = pair?;
        if let Value::Table(table) = value {
            modules.push((name, table));
        }
    }
    modules.sort_by(|a, b| a.0.cmp(&b.0));

    let mut functions = Vec::new();
    for (name, table) in modules {
        let ui_only = UI_ONLY_MODULES.contains(&name.as_str());
        collect_function_stubs(
            &table,
            &format!("cru.{name}"),
            ui_only,
            &mut functions,
            &mut class_paths,
        )?;
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
