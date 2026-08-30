use crate::error::LuaError;
use crate::{
    register_context_module_stub, register_mcp_module_stub, register_oq_module,
    register_paths_module, register_sessions_module, register_tools_module, register_ui_module,
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
const UI_ONLY_MODULES: &[&str] = &["oil"];

const UI_NOTE: &str = "UI-only: requires TUI context, not available in daemon plugins";

#[derive(Debug, Clone)]
struct FunctionStub {
    path: String,
    ui_only: bool,
}

/// A non-function member of a `cru` table — `cru.kiln.active` is a string,
/// `cru.log.levels` a table of them.
///
/// These belong in the Luau declarations for the same reason the functions
/// do: `declare cru: { … }` is an EXACT table type, so a member the
/// declarations omit is a type error at every correct use of it.
#[derive(Debug, Clone)]
pub struct ValueMember {
    pub path: String,
    /// The Luau type of what was observed, always optional: a value present
    /// in the VM the stubs were rendered from may be absent in another (no
    /// kiln is active, no session is open).
    pub luau_type: String,
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

        let (emmylua, docs, paths, values) = render_stubs(lua)?;

        fs::write(output_dir.join("cru.lua"), emmylua)?;
        // The Luau declarations, from the host's own signature table. LuaLS
        // reads `cru.lua`; `luau-analyze` reads this.
        fs::write(
            output_dir.join("cru.d.luau"),
            crate::host_api::render_declarations(&paths, &values),
        )?;
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
        register_vault_module(lua)?;
        register_sessions_module(lua)?;
        register_ui_module(lua)?;
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
            let generated_luau = fs::read_to_string(tmp_dir.join("cru.d.luau"))?;
            let generated_docs = fs::read_to_string(tmp_dir.join("cru-docs.json"))?;

            let committed_docs = committed_path.with_file_name("cru-docs.json");
            let committed_luau = committed_path.with_file_name("cru.d.luau");
            if !committed_path.exists() || !committed_docs.exists() || !committed_luau.exists() {
                return Ok(false);
            }

            let existing_lua = fs::read_to_string(committed_path)?;
            let existing_docs = fs::read_to_string(committed_docs)?;
            let existing_luau = fs::read_to_string(committed_luau)?;

            Ok(generated_lua == existing_lua
                && generated_docs == existing_docs
                && generated_luau == existing_luau)
        })();

        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }
}

/// The EmmyLua stubs, their docs, and every function path found on the VM.
type RenderedStubs = (
    String,
    BTreeMap<String, DocEntry>,
    Vec<String>,
    Vec<ValueMember>,
);

/// Every `cru.*` function path the VM has, in the order the declarations use.
///
/// Public so a gate can compare the host's declared signatures against what
/// is really registered, by PATH. A substring check cannot do that job: the
/// needle `on: (` matches `option: (` and `set_output_validation: (`, so a
/// declaration for a function nobody registered passes unnoticed.
pub fn function_paths(lua: &Lua) -> Result<Vec<String>, LuaError> {
    Ok(render_stubs(lua)?.2)
}

/// Every non-function `cru.*` member the VM has, with its observed type.
pub fn value_members(lua: &Lua) -> Result<Vec<ValueMember>, LuaError> {
    Ok(render_stubs(lua)?.3)
}

fn render_stubs(lua: &Lua) -> Result<RenderedStubs, LuaError> {
    let cru: Table = lua.globals().get("cru")?;

    let mut class_paths = BTreeSet::new();
    class_paths.insert("cru".to_string());

    // Whatever is on `cru`, rather than a hardcoded list. The list was the
    // problem: it named six modules the VM does not have and missed twelve it
    // does, and every module registered after it was written was invisible.
    //
    // Top-level FUNCTIONS count too. `cru.on`, `cru.on_session_start` and
    // their siblings live directly on `cru`, and a walk that kept only tables
    // left them out of both the stubs and the Luau declarations — under which
    // a plugin calling `cru.on(...)` is calling something the declarations say
    // does not exist.
    let mut modules: Vec<(String, Table)> = Vec::new();
    let mut top_level: Vec<String> = Vec::new();
    for pair in cru.pairs::<String, Value>() {
        let (name, value) = pair?;
        match value {
            Value::Table(table) => modules.push((name, table)),
            Value::Function(_) => top_level.push(name),
            _ => {}
        }
    }
    modules.sort_by(|a, b| a.0.cmp(&b.0));
    top_level.sort();

    let mut functions = Vec::new();
    let mut values: Vec<ValueMember> = Vec::new();
    for name in top_level {
        functions.push(FunctionStub {
            path: format!("cru.{name}"),
            ui_only: false,
        });
    }
    for (name, table) in modules {
        let ui_only = UI_ONLY_MODULES.contains(&name.as_str());
        // A callable table is both: `cru.log("info", msg)` works and so does
        // `cru.log.notify(...)`. Record the call before descending.
        if is_callable(&table) {
            functions.push(FunctionStub {
                path: format!("cru.{name}"),
                ui_only,
            });
        }
        collect_function_stubs(
            &table,
            &format!("cru.{name}"),
            ui_only,
            &mut functions,
            &mut class_paths,
            &mut values,
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

    let paths = functions.iter().map(|f| f.path.clone()).collect();
    values.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((out, docs, paths, values))
}

/// Whether a table carries a `__call` metamethod — `cru.log` does.
fn is_callable(table: &Table) -> bool {
    table
        .metatable()
        .is_some_and(|meta| matches!(meta.get::<Value>("__call"), Ok(Value::Function(_))))
}

fn collect_function_stubs(
    table: &Table,
    base_path: &str,
    ui_only: bool,
    functions: &mut Vec<FunctionStub>,
    class_paths: &mut BTreeSet<String>,
    values: &mut Vec<ValueMember>,
) -> Result<(), LuaError> {
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
            // A class is declared only for a table that contributes
            // something. `cru.sessions` is a deprecated metatable alias
            // with no functions of its own; declaring `---@class
            // cru.sessions` would advertise the wrong name on the strength
            // of the alias table merely existing.
            Value::Function(_) => {
                class_paths.insert(base_path.to_string());
                functions.push(FunctionStub { path, ui_only });
            }
            Value::Table(sub_table) => {
                class_paths.insert(base_path.to_string());
                if is_callable(&sub_table) {
                    functions.push(FunctionStub {
                        path: path.clone(),
                        ui_only,
                    });
                }
                collect_function_stubs(&sub_table, &path, ui_only, functions, class_paths, values)?;
            }
            // A scalar member. Recorded so the declarations describe it —
            // `cru.kiln.active` is a string, and an exact table type that
            // omits it rejects every correct read of it.
            other => {
                let luau_type = match other {
                    Value::String(_) => "string",
                    Value::Integer(_) | Value::Number(_) => "number",
                    Value::Boolean(_) => "boolean",
                    _ => "any",
                };
                values.push(ValueMember {
                    path,
                    luau_type: luau_type.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn is_ui_only_path(path: &str) -> bool {
    UI_ONLY_MODULES.iter().any(|module| {
        path == format!("cru.{module}") || path.starts_with(&format!("cru.{module}."))
    })
}
