//! Daemon-side Lua plugin loading
//!
//! Provides a [`DaemonPluginLoader`] that creates its own `LuaExecutor`,
//! registers daemon-appropriate modules (networking, filesystem, shell,
//! JSON query, paths) and discovers/loads plugins.
//!
//! UI modules (oil, popup, panel, statusline) are intentionally excluded —
//! the daemon is headless.

use crucible_lua::{
    register_fs_module, register_http_module, register_oq_module, register_paths_module,
    register_shell_module, register_ws_module, LuaExecutor, PathsContext, PluginManager,
    PluginSpec, ShellPolicy,
};
use std::path::PathBuf;
use tracing::{info, warn};

/// Daemon-side plugin loader with its own Lua runtime.
///
/// The daemon gets a separate `LuaExecutor` — it does **not** share
/// Lua state with any CLI instance.
pub struct DaemonPluginLoader {
    executor: LuaExecutor,
    plugin_manager: PluginManager,
    loaded_specs: Vec<PluginSpec>,
}

impl DaemonPluginLoader {
    /// Create a new loader, registering daemon-appropriate Lua modules.
    ///
    /// Registered modules:
    /// - `cru.http` — HTTP client
    /// - `cru.ws` — WebSocket client
    /// - `cru.fs` — Filesystem operations
    /// - `cru.shell` — Shell execution (with default policy)
    /// - `cru.json_query` (`oq`) — JSON/YAML/TOML query
    /// - `cru.paths` — Standard path helpers
    ///
    /// **Not** registered (UI-only):
    /// - `cru.oil`, `cru.popup`, `cru.panel`, `cru.statusline`
    pub fn new() -> anyhow::Result<Self> {
        let executor = LuaExecutor::new().map_err(|e| anyhow::anyhow!("LuaExecutor init: {e}"))?;

        let lua = executor.lua();

        register_http_module(lua).map_err(|e| anyhow::anyhow!("http module: {e}"))?;
        register_ws_module(lua).map_err(|e| anyhow::anyhow!("ws module: {e}"))?;
        register_fs_module(lua).map_err(|e| anyhow::anyhow!("fs module: {e}"))?;
        register_shell_module(lua, ShellPolicy::default())
            .map_err(|e| anyhow::anyhow!("shell module: {e}"))?;
        register_oq_module(lua).map_err(|e| anyhow::anyhow!("oq module: {e}"))?;
        register_paths_module(lua, PathsContext::new())
            .map_err(|e| anyhow::anyhow!("paths module: {e}"))?;

        let plugin_manager = PluginManager::new();

        Ok(Self {
            executor,
            plugin_manager,
            loaded_specs: Vec::new(),
        })
    }

    /// Discover and load plugins from the given search paths.
    ///
    /// Returns the list of [`PluginSpec`]s extracted from successfully loaded
    /// plugins.
    pub fn load_plugins(&mut self, plugin_paths: &[PathBuf]) -> anyhow::Result<Vec<PluginSpec>> {
        for path in plugin_paths {
            self.plugin_manager.add_search_path(path.clone());
        }

        let discovered = self
            .plugin_manager
            .discover()
            .map_err(|e| anyhow::anyhow!("plugin discover: {e}"))?;

        if discovered.is_empty() {
            info!("No daemon plugins discovered");
            return Ok(Vec::new());
        }

        info!("Discovered {} daemon plugin(s)", discovered.len());

        let loaded = self
            .plugin_manager
            .load_all()
            .map_err(|e| anyhow::anyhow!("plugin load_all: {e}"))?;

        info!("Loaded {} daemon plugin(s)", loaded.len());

        let mut specs = Vec::new();
        for name in &loaded {
            match self.load_plugin_spec(name) {
                Ok(spec) => {
                    info!(
                        "Plugin '{}' spec extracted (tools={}, commands={}, handlers={}, services={})",
                        name,
                        spec.tools.len(),
                        spec.commands.len(),
                        spec.handlers.len(),
                        spec.services.len(),
                    );
                    for svc in &spec.services {
                        info!(
                            "  service '{}' (fn={}) — {}",
                            svc.name, svc.service_fn, svc.description
                        );
                    }
                    specs.push(spec);
                }
                Err(e) => {
                    warn!("Failed to extract spec for plugin '{}': {}", name, e);
                }
            }
        }

        self.loaded_specs = specs.clone();
        Ok(specs)
    }

    fn load_plugin_spec(&self, name: &str) -> anyhow::Result<PluginSpec> {
        let plugin = self
            .plugin_manager
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not found after load", name))?;

        let main_path = plugin.main_path();
        let spec = crucible_lua::load_plugin_spec(&main_path)
            .map_err(|e| anyhow::anyhow!("spec load for '{}': {e}", name))?
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' returned no spec", name))?;
        Ok(spec)
    }

    #[allow(dead_code)]
    pub fn loaded_specs(&self) -> &[PluginSpec] {
        &self.loaded_specs
    }

    /// Borrow the underlying [`LuaExecutor`].
    #[allow(dead_code)]
    pub fn executor(&self) -> &LuaExecutor {
        &self.executor
    }

    /// Borrow the underlying [`PluginManager`].
    #[allow(dead_code)]
    pub fn plugin_manager(&self) -> &PluginManager {
        &self.plugin_manager
    }
}

/// Return the default daemon plugin search paths.
///
/// Currently: `~/.config/crucible/daemon-plugins/`
pub fn default_daemon_plugin_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(env_paths) = std::env::var("CRUCIBLE_DAEMON_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for p in env_paths.split(sep) {
            if !p.is_empty() {
                paths.push(PathBuf::from(p));
            }
        }
    }

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("crucible").join("daemon-plugins"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_plugin_loader_creates_successfully() {
        let loader = DaemonPluginLoader::new();
        assert!(
            loader.is_ok(),
            "DaemonPluginLoader::new() failed: {:?}",
            loader.err()
        );
    }

    #[test]
    fn default_paths_includes_config_dir() {
        let paths = default_daemon_plugin_paths();
        let has_daemon_plugins = paths
            .iter()
            .any(|p| p.to_string_lossy().contains("daemon-plugins"));
        assert!(
            has_daemon_plugins,
            "Expected daemon-plugins path in {:?}",
            paths
        );
    }
}
