//! Daemon-side Lua plugin loading
//!
//! Provides a [`DaemonPluginLoader`] that creates its own `LuaExecutor`,
//! registers daemon-appropriate modules (networking, filesystem, shell,
//! JSON query, paths) and discovers/loads plugins.
//!
//! UI modules (oil, popup, panel, statusline) are intentionally excluded —
//! the daemon is headless.

use crucible_core::storage::NoteStore;
use crucible_lua::{
    register_graph_module, register_graph_module_with_store, register_oq_module,
    register_paths_module, register_sessions_module, register_sessions_module_with_api,
    register_shell_module, register_vault_module, register_vault_module_with_store,
    register_ws_module, DaemonSessionApi, LuaExecutor, PathsContext, PluginManager, PluginSpec,
    ShellPolicy,
};
use mlua::LuaSerdeExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
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
    /// `LuaExecutor::new()` provides: `cru.http`, `cru.fs`, `cru.timer`,
    /// `cru.ratelimit`, and `cru.retry`/`cru.emitter`/`cru.check` (lua stdlib).
    ///
    /// Additional daemon modules registered here:
    /// - `cru.ws` — WebSocket client
    /// - `cru.shell` — Shell execution (with default policy)
    /// - `oq` — JSON/YAML/TOML query
    /// - `paths` — Standard path helpers
    /// - `cru.kiln` / `cru.graph` — Kiln and graph stubs (upgraded with storage later)
    ///
    /// **Not** registered (UI-only):
    /// - `cru.oil`, `cru.popup`, `cru.panel`, `cru.statusline`
    pub fn new(
        plugin_config: HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<Self> {
        let executor = LuaExecutor::new().map_err(|e| anyhow::anyhow!("LuaExecutor init: {e}"))?;

        // LuaExecutor::new() already registers: http, fs, timer, ratelimit, lua_stdlib.
        // Register additional daemon-specific modules here.
        let lua = executor.lua();

        register_ws_module(lua).map_err(|e| anyhow::anyhow!("ws module: {e}"))?;
        register_shell_module(lua, ShellPolicy::default())
            .map_err(|e| anyhow::anyhow!("shell module: {e}"))?;
        register_oq_module(lua).map_err(|e| anyhow::anyhow!("oq module: {e}"))?;
        register_paths_module(lua, PathsContext::new())
            .map_err(|e| anyhow::anyhow!("paths module: {e}"))?;
        register_graph_module(lua).map_err(|e| anyhow::anyhow!("graph module: {e}"))?;
        register_vault_module(lua).map_err(|e| anyhow::anyhow!("vault module: {e}"))?;
        register_sessions_module(lua).map_err(|e| anyhow::anyhow!("sessions module: {e}"))?;
        Self::register_plugin_config(lua, plugin_config)
            .map_err(|e| anyhow::anyhow!("config module: {e}"))?;

        let plugin_manager = PluginManager::new();

        Ok(Self {
            executor,
            plugin_manager,
            loaded_specs: Vec::new(),
        })
    }

    /// Register plugin config as `crucible.config` in the Lua runtime.
    ///
    /// Provides `crucible.config.get("plugin_name.key")` for dotted-key lookup
    /// from `[plugins.*]` sections in config.toml.
    fn register_plugin_config(
        lua: &mlua::Lua,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<(), mlua::Error> {
        let config_table = lua.create_table()?;

        // Store the raw config data as a Lua table
        let data = lua.to_value(&config)?;
        config_table.set("_data", data)?;

        // crucible.config.get("namespace.key") -> value
        let get_fn = lua.create_function(|lua, key: String| {
            let globals = lua.globals();
            let crucible: mlua::Table = globals.get("crucible")?;
            let config: mlua::Table = crucible.get("config")?;
            let data: mlua::Value = config.get("_data")?;

            let mlua::Value::Table(data_table) = data else {
                return Ok(mlua::Value::Nil);
            };

            // Split on first dot: "discord.bot_token" -> ("discord", "bot_token")
            if let Some(dot_pos) = key.find('.') {
                let namespace = &key[..dot_pos];
                let subkey = &key[dot_pos + 1..];
                let ns_val: mlua::Value = data_table.get(namespace.to_string())?;
                if let mlua::Value::Table(ns_table) = ns_val {
                    return ns_table.get(subkey.to_string());
                }
                Ok(mlua::Value::Nil)
            } else {
                data_table.get(key)
            }
        })?;
        config_table.set("get", get_fn)?;

        // Register on the crucible global
        let globals = lua.globals();
        let crucible: mlua::Table = globals.get("crucible")?;
        crucible.set("config", config_table)?;

        Ok(())
    }

    /// Upgrade graph and vault modules with real NoteStore-backed implementations.
    ///
    /// Call after a kiln opens and storage is available. Replaces stub functions
    /// registered in `new()` with implementations that query the store.
    /// Also sets `cru.kiln.active_path` to the kiln directory path.
    pub fn upgrade_with_storage(
        &self,
        store: Arc<dyn NoteStore>,
        kiln_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let lua = self.executor.lua();

        register_graph_module_with_store(lua, store.clone())
            .map_err(|e| anyhow::anyhow!("graph upgrade: {e}"))?;
        register_vault_module_with_store(lua, store)
            .map_err(|e| anyhow::anyhow!("vault upgrade: {e}"))?;

        // Set cru.kiln.active_path so plugins know which kiln is active
        let globals = lua.globals();
        if let Ok(cru) = globals.get::<mlua::Table>("cru") {
            if let Ok(kiln) = cru.get::<mlua::Table>("kiln") {
                let _ = kiln.set("active_path", kiln_path.to_string_lossy().to_string());
            }
        }

        info!("Lua graph/vault modules upgraded with storage (kiln: {})", kiln_path.display());
        Ok(())
    }

    /// Upgrade sessions module with real daemon-backed implementations.
    ///
    /// Call after session/agent managers are created. Replaces stub `cru.sessions.*`
    /// functions with implementations that delegate to the provided API.
    pub fn upgrade_with_sessions(&self, api: Arc<dyn DaemonSessionApi>) -> anyhow::Result<()> {
        register_sessions_module_with_api(self.executor.lua(), api)
            .map_err(|e| anyhow::anyhow!("sessions upgrade: {e}"))?;
        info!("Lua sessions module upgraded with daemon API");
        Ok(())
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

        // Extract spec from sandbox (for metadata)
        let spec = crucible_lua::load_plugin_spec(&main_path)
            .map_err(|e| anyhow::anyhow!("spec load for '{}': {e}", name))?
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' returned no spec", name))?;

        // Also execute the plugin in the daemon's real Lua runtime so that
        // event handlers, gateway connections, etc. actually register.
        if let Err(e) = self.execute_plugin(&main_path) {
            warn!("Failed to execute plugin '{}' in daemon runtime: {}", name, e);
        }

        Ok(spec)
    }

    /// Execute a plugin's init.lua in the daemon's Lua executor.
    ///
    /// Sets up `package.path` so that `require("gateway")` etc. resolves
    /// to files in the plugin's `lua/` directory, then evaluates the init file.
    fn execute_plugin(&self, init_path: &std::path::Path) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let plugin_dir = init_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("init path has no parent"))?;
        let lua_dir = plugin_dir.join("lua");

        // Add plugin's lua/ dir to package.path so require() works
        let setup_code = format!(
            r#"package.path = "{}/?.lua;" .. package.path"#,
            lua_dir.display()
        );
        lua.load(&setup_code)
            .exec()
            .map_err(|e| anyhow::anyhow!("package.path setup: {e}"))?;

        // Execute init.lua
        let source = std::fs::read_to_string(init_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", init_path.display()))?;
        lua.load(&source)
            .set_name(init_path.to_string_lossy().as_ref())
            .exec()
            .map_err(|e| anyhow::anyhow!("exec {}: {e}", init_path.display()))?;

        info!("Executed plugin in daemon runtime: {}", init_path.display());
        Ok(())
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

/// Return the default plugin search paths.
///
/// Uses the same paths as the CLI: `CRUCIBLE_PLUGIN_PATH` env var
/// and `~/.config/crucible/plugins/`.
pub fn default_daemon_plugin_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for p in env_paths.split(sep) {
            if !p.is_empty() {
                paths.push(PathBuf::from(p));
            }
        }
    }

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("crucible").join("plugins"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_plugin_loader_creates_successfully() {
        let loader = DaemonPluginLoader::new(HashMap::new());
        assert!(
            loader.is_ok(),
            "DaemonPluginLoader::new() failed: {:?}",
            loader.err()
        );
    }

    #[test]
    fn default_paths_includes_config_dir() {
        let paths = default_daemon_plugin_paths();
        let has_plugins = paths
            .iter()
            .any(|p| p.to_string_lossy().contains("plugins"));
        assert!(
            has_plugins,
            "Expected plugins path in {:?}",
            paths
        );
    }
}
