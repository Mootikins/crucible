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
    register_shell_module, register_tools_module, register_tools_module_with_api,
    register_vault_module, register_vault_module_with_store, register_ws_module, DaemonSessionApi,
    DaemonToolsApi, LuaExecutor, PathsContext, PluginManager, PluginSpec, ShellPolicy,
};
use mlua::LuaSerdeExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Daemon-side plugin loader with its own Lua runtime.
///
/// The daemon gets a separate `LuaExecutor` — it does **not** share
/// Lua state with any CLI instance.
pub struct DaemonPluginLoader {
    executor: LuaExecutor,
    plugin_manager: PluginManager,
    loaded_specs: Vec<PluginSpec>,
    /// Service functions extracted from plugins during loading.
    /// Each entry is `(service_name, mlua::Function)`.
    service_fns: Vec<(String, mlua::Function)>,
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
    pub fn new(plugin_config: HashMap<String, serde_json::Value>) -> anyhow::Result<Self> {
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
        register_tools_module(lua).map_err(|e| anyhow::anyhow!("tools module: {e}"))?;
        Self::register_plugin_config(lua, plugin_config)
            .map_err(|e| anyhow::anyhow!("config module: {e}"))?;

        let plugin_manager = PluginManager::new();

        Ok(Self {
            executor,
            plugin_manager,
            loaded_specs: Vec::new(),
            service_fns: Vec::new(),
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

        info!(
            "Lua graph/vault modules upgraded with storage (kiln: {})",
            kiln_path.display()
        );
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

    /// Upgrade tools module with real daemon-backed implementations.
    ///
    /// Call after workspace tools are available. Replaces stub `cru.tools.*`
    /// functions with implementations that delegate to the provided API.
    pub fn upgrade_with_tools(&self, api: Arc<dyn DaemonToolsApi>) -> anyhow::Result<()> {
        register_tools_module_with_api(self.executor.lua(), api)
            .map_err(|e| anyhow::anyhow!("tools upgrade: {e}"))?;
        info!("Lua tools module upgraded with daemon API");
        Ok(())
    }

    /// Discover and load plugins from the given search paths.
    ///
    /// Returns the list of [`PluginSpec`]s extracted from successfully loaded
    /// plugins. Service functions are stored internally and can be retrieved
    /// via [`take_service_fns`].
    pub async fn load_plugins(
        &mut self,
        plugin_paths: &[PathBuf],
    ) -> anyhow::Result<Vec<PluginSpec>> {
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
            match self.load_plugin_spec(name).await {
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

    /// Drain and return all extracted service functions.
    ///
    /// Each entry is `(service_name, mlua::Function)`. The functions hold
    /// internal refs to the Lua VM and can be spawned as independent async
    /// tasks via `func.call_async::<()>(())`.
    pub fn take_service_fns(&mut self) -> Vec<(String, mlua::Function)> {
        std::mem::take(&mut self.service_fns)
    }

    async fn load_plugin_spec(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        let plugin = self
            .plugin_manager
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not found after load", name))?;

        let main_path = plugin.main_path();

        // Extract spec from sandbox (for metadata)
        let spec = crucible_lua::load_plugin_spec(&main_path)
            .map_err(|e| anyhow::anyhow!("spec load for '{}': {e}", name))?
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' returned no spec", name))?;

        // Execute the plugin in the daemon's real Lua runtime using eval_async
        // so that async Lua functions (gateway.connect, etc.) can yield.
        // Also extract service Function refs from the returned spec table.
        match self.execute_plugin(&main_path).await {
            Ok(services) => {
                for (svc_name, func) in services {
                    debug!(
                        "Extracted service function '{}' from plugin '{}'",
                        svc_name, name
                    );
                    self.service_fns.push((svc_name, func));
                }
            }
            Err(e) => {
                warn!(
                    "Failed to execute plugin '{}' in daemon runtime: {}",
                    name, e
                );
            }
        }

        Ok(spec)
    }

    /// Execute a plugin's init.lua in the daemon's Lua executor (async).
    ///
    /// Sets up `package.path` so that `require("gateway")` etc. resolves
    /// to files in the plugin's `lua/` directory, then evaluates the init file
    /// using `eval_async` to enable async Lua function yielding.
    ///
    /// Returns extracted service `(name, Function)` pairs from the returned
    /// spec table's `services` field.
    async fn execute_plugin(
        &self,
        init_path: &std::path::Path,
    ) -> anyhow::Result<Vec<(String, mlua::Function)>> {
        let lua = self.executor.lua();
        let plugin_dir = init_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("init path has no parent"))?;
        let lua_dir = plugin_dir.join("lua");

        // Add plugin's lua/ dir to package.path so require() works
        let lua_dir_str = lua_dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let setup_code = format!(r#"package.path = "{}/?.lua;" .. package.path"#, lua_dir_str);
        lua.load(&setup_code)
            .exec()
            .map_err(|e| anyhow::anyhow!("package.path setup: {e}"))?;

        // Execute init.lua with eval_async — captures return value AND enables async Lua
        let source = std::fs::read_to_string(init_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", init_path.display()))?;
        let return_val: mlua::Value = lua
            .load(&source)
            .set_name(init_path.to_string_lossy().as_ref())
            .eval_async()
            .await
            .map_err(|e| anyhow::anyhow!("exec {}: {e}", init_path.display()))?;

        // Extract service functions from the returned spec table
        let mut services = Vec::new();
        if let mlua::Value::Table(spec) = return_val {
            if let Ok(svc_table) = spec.get::<mlua::Table>("services") {
                for (name, entry) in svc_table.pairs::<String, mlua::Table>().flatten() {
                    if let Ok(func) = entry.get::<mlua::Function>("fn") {
                        services.push((name, func));
                    }
                }
            }
        }

        info!("Executed plugin in daemon runtime: {}", init_path.display());
        Ok(services)
    }
    /// Reload a plugin: clear its Lua module cache, unload registrations,
    /// re-execute `init.lua`, and re-extract service functions.
    pub async fn reload_plugin(&mut self, name: &str) -> anyhow::Result<PluginSpec> {
        let plugin = self
            .plugin_manager
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not found", name))?;

        let main_path = plugin.main_path();
        let plugin_dir = main_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("plugin main path has no parent"))?;
        let lua_dir = plugin_dir.join("lua");

        self.clear_plugin_lua_cache(&lua_dir)?;

        self.plugin_manager
            .unload(name)
            .map_err(|e| anyhow::anyhow!("unload plugin '{}': {e}", name))?;

        self.plugin_manager
            .load(name)
            .map_err(|e| anyhow::anyhow!("reload plugin '{}': {e}", name))?;

        let spec = self.load_plugin_spec(name).await?;

        let spec_name = spec.name.clone();
        if let Some(existing) = self.loaded_specs.iter_mut().find(|s| s.name == spec_name) {
            *existing = spec.clone();
        } else {
            self.loaded_specs.push(spec.clone());
        }

        info!("Reloaded plugin '{}' successfully", name);
        Ok(spec)
    }

    /// Clear `package.loaded` entries for modules whose `.lua` file lives under `lua_dir`.
    fn clear_plugin_lua_cache(&self, lua_dir: &std::path::Path) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let lua_dir_str = lua_dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");

        lua.load(format!(
            r#"
            local dir = "{lua_dir_str}"
            for mod_name, _ in pairs(package.loaded) do
                local path = dir .. "/" .. mod_name:gsub("%.", "/") .. ".lua"
                local f = io.open(path, "r")
                if f then
                    f:close()
                    package.loaded[mod_name] = nil
                end
            end
            "#,
        ))
        .exec()
        .map_err(|e| anyhow::anyhow!("clear lua cache: {e}"))
    }

    pub fn loaded_plugin_names(&self) -> Vec<String> {
        self.loaded_specs
            .iter()
            .filter_map(|s| s.name.clone())
            .collect()
    }

    /// Return `(plugin_name, plugin_dir)` pairs for all loaded plugins.
    ///
    /// Used by the plugin file watcher to know which directories to monitor
    /// and which plugin name to reload when a file changes.
    pub fn loaded_plugin_dirs(&self) -> Vec<(String, PathBuf)> {
        self.plugin_manager
            .list()
            .filter(|p| p.state == crucible_lua::PluginState::Active)
            .filter_map(|p| {
                let name = p.manifest.name.clone();
                let dir = p.dir.clone();
                if dir.exists() {
                    Some((name, dir))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn executor(&self) -> &LuaExecutor {
        &self.executor
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

    if let Ok(runtime_base) = std::env::var("CRUCIBLE_RUNTIME") {
        let runtime_plugins = PathBuf::from(runtime_base).join("plugins");
        if runtime_plugins.exists() {
            tracing::debug!("Adding runtime plugin path: {:?}", runtime_plugins);
            paths.push(runtime_plugins);
        }
    } else if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let installed_plugins = exe_dir
                .join("..")
                .join("share")
                .join("crucible")
                .join("runtime")
                .join("plugins");
            if installed_plugins.exists() {
                tracing::debug!(
                    "Adding installed runtime plugin path: {:?}",
                    installed_plugins
                );
                paths.push(installed_plugins);
            }
            let dev_plugins = exe_dir
                .join("..")
                .join("..")
                .join("runtime")
                .join("plugins");
            if dev_plugins.exists() {
                tracing::debug!("Adding dev runtime plugin path: {:?}", dev_plugins);
                paths.push(dev_plugins);
            }
        }
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
        assert!(has_plugins, "Expected plugins path in {:?}", paths);
    }

    #[test]
    fn test_default_paths_includes_runtime_when_set() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let runtime_dir = tmp.path();
        std::fs::create_dir(runtime_dir.join("plugins")).unwrap();

        std::env::set_var(
            "CRUCIBLE_RUNTIME",
            runtime_dir.to_string_lossy().to_string(),
        );

        let paths = default_daemon_plugin_paths();

        let has_runtime = paths
            .iter()
            .any(|p| p.ends_with("plugins") && p.starts_with(runtime_dir));

        std::env::remove_var("CRUCIBLE_RUNTIME");

        assert!(has_runtime, "Expected runtime plugin path in {:?}", paths);
    }

    #[test]
    fn test_runtime_path_resolved_from_exe() {
        // Ensure CRUCIBLE_RUNTIME is not set
        std::env::remove_var("CRUCIBLE_RUNTIME");

        let paths = default_daemon_plugin_paths();

        // Should have at least one path (config dir or exe-relative)
        assert!(!paths.is_empty(), "Expected at least one path");

        // At least one path should contain "plugins"
        let has_plugins = paths
            .iter()
            .any(|p| p.to_string_lossy().contains("plugins"));
        assert!(has_plugins, "Expected plugins path in {:?}", paths);
    }
}
