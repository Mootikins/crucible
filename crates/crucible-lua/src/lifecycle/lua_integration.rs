use super::{LifecycleError, LifecycleResult, PluginManager};
use crate::modules::{ModuleRegistry, PrivateRootGuard, RootKind};
use std::path::Path;

impl PluginManager {
    /// Make this plugin's own `lua/` directory resolvable while the returned
    /// guard lives, and its directory's parent resolvable by plugin name.
    ///
    /// The guard is what keeps resolution independent of load order: the
    /// private root pops when the load ends, so the next plugin cannot
    /// resolve the previous plugin's private modules.
    pub(super) fn enter_plugin_modules(
        &self,
        plugin_dir: &Path,
    ) -> LifecycleResult<PrivateRootGuard> {
        let mut roots: Vec<(std::path::PathBuf, RootKind)> = Vec::new();
        for path in &self.search_paths {
            roots.push((path.clone(), RootKind::Plugin));
        }
        if let Some(parent) = plugin_dir.parent() {
            if !roots.iter().any(|(root, _)| root == parent) {
                roots.push((parent.to_path_buf(), RootKind::Plugin));
            }
        }
        self.modules
            .set_roots(roots)
            .map_err(|e| LifecycleError::LoadError(format!("configure module roots: {e}")))?;
        self.modules
            .enter_plugin_root(plugin_dir)
            .map_err(|e| LifecycleError::LoadError(format!("enter plugin module root: {e}")))
    }

    /// The registry this manager's `require` reads.
    pub(super) fn modules(&self) -> &ModuleRegistry {
        &self.modules
    }

    /// Forget everything a plugin's directory contributed, so a reload
    /// re-reads it: the entry module by name, and every module cached from a
    /// file under the plugin's directory.
    pub(super) fn clear_plugin_modules(&self, plugin_name: &str) -> LifecycleResult<()> {
        self.modules
            .invalidate_name(&self.lua, plugin_name)
            .map_err(|e| LifecycleError::LoadError(format!("clear plugin module: {e}")))?;
        if let Some(plugin) = self.plugins.get(plugin_name) {
            self.modules
                .invalidate_under(&self.lua, &plugin.dir)
                .map_err(|e| LifecycleError::LoadError(format!("clear plugin modules: {e}")))?;
        }
        Ok(())
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub fn eval_runtime<T>(&self, source: &str) -> LifecycleResult<T>
    where
        T: mlua::FromLua,
    {
        self.lua.load(source).eval().map_err(|e| {
            LifecycleError::LoadError(format!("Failed to evaluate plugin runtime Lua: {}", e))
        })
    }
}
