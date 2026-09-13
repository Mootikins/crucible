use std::path::{Path, PathBuf};

mod discovery;
mod error_log;
mod spec;
mod state;

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
    let plugin_dir = dir.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    // No manifest: a plugin is a directory with an entry file.
    std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();
    plugin_dir
}

/// A VM with the prelude and an error log, as the daemon VM has. The test
/// reads the log through the handle `install` answers.
pub(super) fn vm_with_error_log() -> (
    mlua::Lua,
    std::sync::Arc<std::sync::Mutex<crate::lifecycle::PluginErrorLog>>,
) {
    let lua = mlua::Lua::new();
    let log = crate::lifecycle::PluginErrorLog::install(&lua, 100);
    lua.load(
        r#"
        cru = {}
        cru.log = function(level, msg) end
        cru.timer = { sleep = function(secs) end }
    "#,
    )
    .exec()
    .unwrap();
    crate::prelude::register_prelude(&lua).unwrap();
    (lua, log)
}
