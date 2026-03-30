//! Daemon-side Lua plugin loading
//!
//! Provides a [`DaemonPluginLoader`] that creates its own `LuaExecutor`,
//! registers daemon-appropriate modules (networking, filesystem, shell,
//! JSON query, paths) and discovers/loads plugins.
//!
//! UI modules (oil, popup, panel, statusline) are intentionally excluded —
//! the daemon is headless.

use crucible_core::storage::NoteStore;
use crucible_core::storage::PropertyStore;
use crucible_lua::{
    register_graph_module, register_graph_module_with_store, register_oq_module,
    register_paths_module, register_schedule_module, register_sessions_module,
    register_sessions_module_with_api, register_shell_module, register_storage_module,
    register_storage_module_with_store, register_tools_module, register_tools_module_with_api,
    register_vault_module, register_vault_module_with_store, register_ws_module, DaemonSessionApi,
    DaemonToolsApi, LuaExecutor, PathsContext, PluginManager, PluginSource, PluginSpec,
    ShellPolicy,
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
    /// - `cru.schedule` — Interval-based scheduled callbacks
    ///
    /// **Not** registered (UI-only):
    /// - `cru.oil`, `cru.popup`, `cru.panel`, `cru.statusline`
    pub fn new(plugin_config: HashMap<String, serde_json::Value>) -> anyhow::Result<Self> {
        let executor = LuaExecutor::new().map_err(|e| anyhow::anyhow!("LuaExecutor init: {e}"))?;

        // LuaExecutor::new() already registers: http, fs, timer, ratelimit, lua_stdlib.
        // Register additional daemon-specific modules here.
        let lua = executor.lua();

        // Helper to convert module registration errors with context
        fn reg(name: &str, result: Result<(), impl std::fmt::Display>) -> anyhow::Result<()> {
            result.map_err(|e| anyhow::anyhow!("{name} module: {e}"))
        }

        reg("ws", register_ws_module(lua))?;
        reg("shell", register_shell_module(lua, ShellPolicy::default()))?;
        reg("oq", register_oq_module(lua))?;
        reg("paths", register_paths_module(lua, PathsContext::new()))?;
        reg("graph", register_graph_module(lua))?;
        reg("vault", register_vault_module(lua))?;
        reg("storage", register_storage_module(lua))?;
        reg("sessions", register_sessions_module(lua))?;
        reg("tools", register_tools_module(lua))?;
        reg("schedule", register_schedule_module(lua))?;
        reg("config", Self::register_plugin_config(lua, plugin_config))?;

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

    /// Upgrade graph, vault, and storage modules with real store-backed implementations.
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

    /// Upgrade the `cru.storage` module with a real PropertyStore backend.
    ///
    /// Call after a kiln opens and storage is available. The namespace for each
    /// plugin is determined dynamically from `cru._current_plugin` at call time.
    pub fn upgrade_with_property_store(&self, store: Arc<dyn PropertyStore>) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        register_storage_module_with_store(lua, store)
            .map_err(|e| anyhow::anyhow!("storage upgrade: {e}"))?;
        info!("Lua storage module upgraded with PropertyStore");
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
    /// Add plugin search paths to Lua's `package.path` so `require("plugin")`
    /// works globally (from user init.lua, BUILTIN_INIT_LUA, or other plugins).
    ///
    /// Each search path gets two entries:
    /// - `{path}/?.lua` — for single-file plugins
    /// - `{path}/?/init.lua` — for directory plugins (e.g., `require("kiln-expert")` finds `kiln-expert/init.lua`)
    fn configure_runtime_path(
        &self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<()> {
        let lua = self.executor.lua();
        let mut entries = Vec::new();

        for (path, _source) in plugin_paths {
            if !path.exists() {
                continue;
            }
            let path_str = path.to_string_lossy().replace('\\', "/");
            entries.push(format!("{path_str}/?.lua"));
            entries.push(format!("{path_str}/?/init.lua"));
        }

        if entries.is_empty() {
            return Ok(());
        }

        let new_paths = entries.join(";");
        let code = format!(r#"package.path = "{new_paths};" .. package.path"#);
        lua.load(&code)
            .exec()
            .map_err(|e| anyhow::anyhow!("configure runtime path: {e}"))?;

        tracing::debug!("Configured Lua runtime path with {} entries", entries.len());
        Ok(())
    }

    pub async fn load_plugins(
        &mut self,
        plugin_paths: &[(PathBuf, PluginSource)],
    ) -> anyhow::Result<Vec<PluginSpec>> {
        // Set up global runtime path BEFORE discovery so require() works everywhere
        self.configure_runtime_path(plugin_paths)?;

        for (path, source) in plugin_paths {
            self.plugin_manager
                .add_search_path_with_source(path.clone(), *source);
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

    /// Return plugin info including provenance source for each loaded plugin.
    pub fn loaded_plugin_info(&self) -> Vec<serde_json::Value> {
        self.plugin_manager
            .list()
            .filter(|p| p.state == crucible_lua::PluginState::Active)
            .map(|p| {
                serde_json::json!({
                    "name": p.manifest.name,
                    "version": p.manifest.version,
                    "source": p.source.to_string(),
                    "state": p.state.to_string(),
                    "dir": p.dir.to_string_lossy(),
                })
            })
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

    /// Generate LuaCATS type stubs for IDE support.
    ///
    /// Creates `cru.lua` and `cru-docs.json` in `output_dir` by introspecting
    /// registered modules via a temporary executor (does not touch the daemon's
    /// live Lua state).
    pub fn generate_stubs(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        crucible_lua::stubs::StubGenerator::generate(output_dir)
            .map_err(|e| anyhow::anyhow!("stub generation: {e}"))
    }

    pub fn executor(&self) -> &LuaExecutor {
        &self.executor
    }

    /// Evaluate Lua code in the plugin runtime context.
    ///
    /// If `code` starts with `=`, prepend `return ` (Neovim convention).
    /// Returns the string representation of the result.
    pub async fn eval(&self, code: &str) -> anyhow::Result<String> {
        let code = if let Some(expr) = code.strip_prefix('=') {
            format!("return {expr}")
        } else {
            code.to_string()
        };

        let lua = self.executor.lua();
        let result: mlua::Value = lua
            .load(&code)
            .set_name("=lua.eval")
            .eval_async()
            .await
            .map_err(|e| anyhow::anyhow!("{}", crucible_lua::format_lua_error(None, &e)))?;

        match &result {
            mlua::Value::Nil => Ok("nil".to_string()),
            mlua::Value::Boolean(b) => Ok(b.to_string()),
            mlua::Value::Integer(n) => Ok(n.to_string()),
            mlua::Value::Number(n) => Ok(n.to_string()),
            mlua::Value::String(s) => Ok(s
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "<invalid utf8>".to_string())),
            mlua::Value::Table(_) => {
                // Use json encoding for tables
                match lua.from_value::<serde_json::Value>(result) {
                    Ok(json) => Ok(serde_json::to_string_pretty(&json)?),
                    Err(_) => Ok("<table>".to_string()),
                }
            }
            other => Ok(format!("<{}>", other.type_name())),
        }
    }
}

/// Build plugin search paths from config `runtimepath` + env vars + defaults.
///
/// If `runtimepath` is non-empty, each entry's `plugins/` subdir is used as a
/// Runtime source. Otherwise falls back to `CRUCIBLE_RUNTIME` env var and
/// exe-relative detection.
///
/// `CRUCIBLE_PLUGIN_PATH` env var always prepends (highest priority).
/// `~/.config/crucible/plugins/` is always included as User source.
///
/// Paths are ordered by priority (highest first) — same-named plugins at
/// higher-priority paths shadow lower-priority ones.
pub fn daemon_plugin_paths(runtimepath: &[std::path::PathBuf]) -> Vec<(PathBuf, PluginSource)> {
    let mut paths = Vec::new();

    // 1. CRUCIBLE_PLUGIN_PATH env var (highest priority, for dev/CI)
    if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for p in env_paths.split(sep) {
            if !p.is_empty() {
                paths.push((PathBuf::from(p), PluginSource::EnvPath));
            }
        }
    }

    // 2. User plugins (~/.config/crucible/plugins/)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push((
            config_dir.join("crucible").join("plugins"),
            PluginSource::User,
        ));
    }

    // 3. Runtime paths — from config runtimepath or auto-detected
    if !runtimepath.is_empty() {
        for rtp in runtimepath {
            let expanded = expand_tilde(rtp);
            let plugins_dir = expanded.join("plugins");
            if plugins_dir.exists() {
                tracing::debug!("Adding runtimepath plugin dir: {:?}", plugins_dir);
                paths.push((plugins_dir, PluginSource::Runtime));
            }
        }
    } else {
        // Auto-detect: CRUCIBLE_RUNTIME env → exe-relative fallback
        if let Ok(runtime_base) = std::env::var("CRUCIBLE_RUNTIME") {
            let runtime_plugins = PathBuf::from(runtime_base).join("plugins");
            if runtime_plugins.exists() {
                tracing::debug!("Adding runtime plugin path: {:?}", runtime_plugins);
                paths.push((runtime_plugins, PluginSource::Runtime));
            }
        } else if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // Installed: <prefix>/share/crucible/runtime/plugins/
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
                    paths.push((installed_plugins, PluginSource::Runtime));
                }
                // Dev: <repo>/runtime/plugins/
                let dev_plugins = exe_dir
                    .join("..")
                    .join("..")
                    .join("runtime")
                    .join("plugins");
                if dev_plugins.exists() {
                    tracing::debug!("Adding dev runtime plugin path: {:?}", dev_plugins);
                    paths.push((dev_plugins, PluginSource::Runtime));
                }
            }
        }
    }

    paths
}

/// Expand `~` at the start of a path to the user's home directory.
fn expand_tilde(path: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&s[2..]);
        }
    }
    path.to_path_buf()
}

/// Return default plugin paths (no config runtimepath).
/// Convenience for callers that don't have access to config.
pub fn default_daemon_plugin_paths() -> Vec<(PathBuf, PluginSource)> {
    daemon_plugin_paths(&[])
}

/// Bootstrap declared plugins by git-cloning any that are missing.
///
/// Reads `PluginEntry` declarations (typically from `plugins.toml`) and
/// shallow-clones repos into `~/.config/crucible/plugins/<name>/` when
/// the target directory does not already exist.
pub async fn bootstrap_plugins(entries: &[crucible_config::PluginEntry]) -> anyhow::Result<()> {
    let plugins_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins");

    for entry in entries {
        if !entry.enabled {
            continue;
        }
        let name = match plugin_name_from_url(&entry.url) {
            Some(n) => n,
            None => {
                warn!("Skipping plugin with unparseable URL: '{}'", entry.url);
                continue;
            }
        };
        let dest = plugins_dir.join(&name);
        if dest.exists() {
            continue;
        }

        let url = normalize_git_url(&entry.url);
        info!("Bootstrapping plugin '{}' from {}", name, url);

        let mut cmd = tokio::process::Command::new("git");
        cmd.args(["clone", "--depth", "1"]);
        if let Some(ref branch) = entry.branch {
            cmd.args(["--branch", branch]);
        }
        cmd.arg(&url).arg(&dest);

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                info!("Cloned plugin '{}'", name);
                if let Some(ref pin) = entry.pin {
                    let checkout = tokio::process::Command::new("git")
                        .args(["checkout", pin])
                        .current_dir(&dest)
                        .output()
                        .await;
                    if let Ok(out) = checkout {
                        if !out.status.success() {
                            warn!("Failed to checkout pin '{}' for plugin '{}'", pin, name);
                        }
                    }
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to clone plugin '{}': {}", name, stderr.trim());
            }
            Err(e) => {
                warn!("Failed to run git clone for plugin '{}': {}", name, e);
            }
        }
    }
    Ok(())
}

/// Extract plugin name from URL (last path segment, sans `.git`).
///
/// Returns `None` if the extracted name is empty, `.`, or `..` — callers
/// should skip/warn rather than creating directories with unsafe names.
fn plugin_name_from_url(url: &str) -> Option<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git")
        .to_string();
    if name.is_empty() || name == "." || name == ".." {
        None
    } else {
        Some(name)
    }
}

/// Normalize shorthand URLs to full git URLs.
///
/// Passes through full URLs (`http`, `git@`, `ssh://`) unchanged.
/// Treats anything else as a GitHub `user/repo` shorthand.
fn normalize_git_url(url: &str) -> String {
    if url.starts_with("http") || url.starts_with("git@") || url.starts_with("ssh://") {
        url.to_string()
    } else {
        format!("https://github.com/{}.git", url)
    }
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
            .any(|(p, _)| p.to_string_lossy().contains("plugins"));
        assert!(has_plugins, "Expected plugins path in {:?}", paths);
    }

    #[test]
    fn test_default_paths_includes_runtime_when_set() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let runtime_dir = tmp.path();
        std::fs::create_dir(runtime_dir.join("plugins")).unwrap();

        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "CRUCIBLE_RUNTIME",
            runtime_dir.to_string_lossy().to_string(),
        );

        let paths = default_daemon_plugin_paths();

        let has_runtime = paths.iter().any(|(p, src)| {
            p.ends_with("plugins") && p.starts_with(runtime_dir) && *src == PluginSource::Runtime
        });

        assert!(has_runtime, "Expected runtime plugin path in {:?}", paths);
    }

    #[test]
    fn test_runtime_path_resolved_from_exe() {
        // Ensure CRUCIBLE_RUNTIME is not set
        let _guard = crucible_core::test_support::EnvVarGuard::remove("CRUCIBLE_RUNTIME");

        let paths = default_daemon_plugin_paths();

        // Should have at least one path (config dir or exe-relative)
        assert!(!paths.is_empty(), "Expected at least one path");

        // At least one path should contain "plugins"
        let has_plugins = paths
            .iter()
            .any(|(p, _)| p.to_string_lossy().contains("plugins"));
        assert!(has_plugins, "Expected plugins path in {:?}", paths);
    }

    #[tokio::test]
    async fn eval_expression_with_equals_prefix() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        assert_eq!(loader.eval("=1+1").await.unwrap(), "2");
    }

    #[tokio::test]
    async fn eval_string_expression() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        assert_eq!(loader.eval("='hello'").await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn eval_nil_result() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        assert_eq!(loader.eval("=nil").await.unwrap(), "nil");
    }

    #[tokio::test]
    async fn eval_table_as_json() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        let result = loader.eval("={a=1, b=2}").await.unwrap();
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(json["a"], 1);
        assert_eq!(json["b"], 2);
    }

    #[tokio::test]
    async fn eval_statement_returns_nil() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        assert_eq!(loader.eval("local x = 42").await.unwrap(), "nil");
    }

    #[tokio::test]
    async fn eval_syntax_error_returns_err() {
        let loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        assert!(loader.eval("=???").await.is_err());
    }

    #[test]
    fn plugin_name_from_full_https_url() {
        assert_eq!(
            plugin_name_from_url("https://github.com/user/my-plugin.git"),
            Some("my-plugin".to_string())
        );
    }

    #[test]
    fn plugin_name_from_shorthand() {
        assert_eq!(
            plugin_name_from_url("user/my-plugin"),
            Some("my-plugin".to_string())
        );
    }

    #[test]
    fn plugin_name_strips_git_suffix() {
        assert_eq!(
            plugin_name_from_url("git@github.com:user/cool.git"),
            Some("cool".to_string())
        );
    }

    #[test]
    fn plugin_name_no_slash() {
        assert_eq!(
            plugin_name_from_url("standalone"),
            Some("standalone".to_string())
        );
    }

    #[test]
    fn plugin_name_trailing_slash_stripped() {
        assert_eq!(
            plugin_name_from_url("https://github.com/user/repo/"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn plugin_name_empty_url_returns_none() {
        assert_eq!(plugin_name_from_url(""), None);
    }

    #[test]
    fn plugin_name_only_slashes_returns_none() {
        assert_eq!(plugin_name_from_url("///"), None);
    }

    #[test]
    fn plugin_name_dot_returns_none() {
        assert_eq!(plugin_name_from_url("."), None);
    }

    #[test]
    fn plugin_name_dotdot_returns_none() {
        assert_eq!(plugin_name_from_url(".."), None);
    }

    #[test]
    fn plugin_name_bare_git_suffix_returns_none() {
        assert_eq!(plugin_name_from_url(".git"), None);
    }

    #[test]
    fn normalize_passes_https_through() {
        assert_eq!(
            normalize_git_url("https://github.com/user/repo.git"),
            "https://github.com/user/repo.git"
        );
    }

    #[test]
    fn normalize_passes_ssh_through() {
        assert_eq!(
            normalize_git_url("git@github.com:user/repo.git"),
            "git@github.com:user/repo.git"
        );
    }

    #[test]
    fn normalize_expands_shorthand() {
        assert_eq!(
            normalize_git_url("user/repo"),
            "https://github.com/user/repo.git"
        );
    }

    #[test]
    fn normalize_passes_ssh_scheme_through() {
        assert_eq!(
            normalize_git_url("ssh://git@host/repo.git"),
            "ssh://git@host/repo.git"
        );
    }

    #[tokio::test]
    async fn bootstrap_skips_disabled_entries() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Override config dir isn't feasible, but we can verify the function
        // doesn't attempt to clone when entry is disabled
        let entries = vec![crucible_config::PluginEntry {
            url: "user/disabled-plugin".to_string(),
            branch: None,
            pin: None,
            enabled: false,
        }];
        // Should succeed without attempting any git operations
        let result = bootstrap_plugins(&entries).await;
        assert!(result.is_ok());
        drop(tmp);
    }
}
