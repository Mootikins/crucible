use super::PluginManager;
use std::path::{Path, PathBuf};

mod dependencies;
mod discovery;
mod error_log;
mod hooks;
mod loading;
mod registration;
mod spec;

pub(super) fn create_test_plugin(dir: &Path, name: &str, version: &str) -> PathBuf {
    let lua = format!(
        r#"
local M = {{}}
function M.test_tool()
    return "ok"
end
return {{
    name = "{name}",
    version = "{version}",
    tools = {{
        test_tool = {{
            desc = "A test tool",
            fn = M.test_tool,
        }},
    }},
}}
"#
    );
    create_plugin_with_lua(dir, name, version, &lua)
}

pub(super) fn create_plugin_with_lua(
    dir: &Path,
    name: &str,
    version: &str,
    lua_source: &str,
) -> PathBuf {
    let plugin_dir = dir.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = format!("name: {name}\nversion: \"{version}\"\nmain: init.lua\n");
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), lua_source).unwrap();

    plugin_dir
}

pub(super) fn create_test_plugin_with_source(
    dir: &Path,
    name: &str,
    version: &str,
    lua_source: &str,
) {
    create_plugin_with_lua(dir, name, version, lua_source);
}

pub(super) fn create_plugin_with_deps(dir: &Path, name: &str, deps: &[&str]) -> PathBuf {
    let plugin_dir = dir.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let deps_yaml: String = deps
        .iter()
        .map(|d| format!("  - name: {d}"))
        .collect::<Vec<_>>()
        .join("\n");

    let manifest = format!(
        r#"
name: {name}
version: "1.0.0"
main: init.lua
dependencies:
{deps_yaml}
"#
    );
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "-- empty").unwrap();

    plugin_dir
}

pub(super) fn create_spec_plugin(dir: &Path, name: &str) -> PathBuf {
    let plugin_dir = dir.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let lua = format!(
        r#"
local M = {{}}
function M.search(args) return {{ result = "ok" }} end
function M.search_command(args, ctx) end
function M.on_note(ctx, event) return event end
function M.graph_view(ctx) end
function M.graph_handler(key, ctx) end

return {{
    name = "{}",
    version = "1.0.0",
    description = "Test spec plugin",
    capabilities = {{ "kiln" }},

    tools = {{
        search = {{
            desc = "Search notes",
            params = {{
                {{ name = "query", type = "string", desc = "Search query" }},
                {{ name = "limit", type = "number", desc = "Max results", optional = true }},
            }},
            fn = M.search,
        }},
    }},

    commands = {{
        search = {{ desc = "Search command", hint = "[query]", fn = M.search_command }},
    }},

    handlers = {{
        {{ event = "note:created", priority = 50, name = "on_note", fn = M.on_note }},
    }},

    views = {{
        graph = {{ desc = "Graph view", fn = M.graph_view, handler = M.graph_handler }},
    }},

    setup = function(config) end,
}}
"#,
        name
    );

    std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();
    plugin_dir
}

/// Set up a PluginManager with the full Lua stdlib loaded (needed for emitter tests).
pub(super) fn setup_emitter_manager() -> PluginManager {
    setup_emitter_manager_with_paths(vec![])
}

pub(super) fn setup_emitter_manager_with_paths(paths: Vec<PathBuf>) -> PluginManager {
    let manager = PluginManager::new().with_search_paths(paths);
    manager
        .lua
        .load(
            r#"
        cru = {}
        cru.log = function(level, msg) end
        cru.timer = { sleep = function(secs) end }
    "#,
        )
        .exec()
        .unwrap();
    crate::lua_stdlib::register_lua_stdlib(&manager.lua).unwrap();
    manager
}
