use super::{LifecycleError, LifecycleResult, PluginManager};
use std::path::Path;

impl PluginManager {
    pub(super) fn configure_plugin_package_path(&self, plugin_dir: &Path) -> LifecycleResult<()> {
        let plugin_dir = plugin_dir.to_string_lossy();
        self.lua
            .load(format!(
                r#"
local plugin_dir = {plugin_dir:?}
local path_entries = {{
    plugin_dir .. "/?.lua",
    plugin_dir .. "/?/init.lua",
}}

for _, entry in ipairs(path_entries) do
    if not package.path:find(entry, 1, true) then
        package.path = entry .. ";" .. package.path
    end
end
"#
            ))
            .exec()
            .map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Failed to configure package.path for {}: {}",
                    plugin_dir, e
                ))
            })
    }

    pub(super) fn clear_plugin_modules(&self, plugin_name: &str) -> LifecycleResult<()> {
        self.lua
            .load(format!(
                r#"
local name = {plugin_name:?}
for k, _ in pairs(package.loaded) do
    if type(k) == "string" and (k == name or k:sub(1, #name + 1) == name .. ".") then
        package.loaded[k] = nil
    end
end
"#
            ))
            .exec()
            .map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Failed to clear package.loaded entries for {}: {}",
                    plugin_name, e
                ))
            })
    }

    pub fn eval_runtime<T>(&self, source: &str) -> LifecycleResult<T>
    where
        T: mlua::FromLua,
    {
        self.lua.load(source).eval().map_err(|e| {
            LifecycleError::LoadError(format!("Failed to evaluate plugin runtime Lua: {}", e))
        })
    }
}
