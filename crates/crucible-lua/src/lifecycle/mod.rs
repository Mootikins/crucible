//! Plugin lifecycle management

mod dependency;
mod discovery;
mod error;
mod error_log;
mod hooks;
mod loading;
mod lua_integration;
mod queries;
mod registration;
mod spec;

#[cfg(test)]
mod tests;

use crate::discovered::{DiscoveredCommand, DiscoveredTool};
use crate::manifest::{LoadedPlugin, PluginSource};
use mlua::{Lua, RegistryKey};
use registration::RegisteredItem;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::warn;

pub use discovery::PluginDiscoveryError;
pub use error::{LifecycleError, LifecycleResult};
pub use error_log::{PluginErrorEntry, PluginErrorLog};
pub use spec::{load_plugin_spec, load_plugin_spec_from_source, PluginSpec};

pub struct PluginManager {
    plugins: HashMap<String, LoadedPlugin>,
    search_paths: Vec<PathBuf>,
    /// Maps search paths to their provenance category.
    path_sources: HashMap<PathBuf, PluginSource>,
    tools: Vec<RegisteredItem<DiscoveredTool>>,
    commands: Vec<RegisteredItem<DiscoveredCommand>>,
    lua: Lua,
    on_unload_hooks: HashMap<String, RegistryKey>,
    on_load_hooks: HashMap<String, RegistryKey>,
    error_log: Arc<Mutex<PluginErrorLog>>,
    /// Directories that failed to become plugins during `discover()`.
    ///
    /// A plugin whose manifest doesn't parse never enters `plugins`, so it has
    /// no `PluginState` to mark `Error` — before this the only trace was a
    /// `warn!` in the daemon log, which is how `reflection` stayed invisible.
    discovery_errors: Vec<PluginDiscoveryError>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &self.plugins)
            .field("search_paths", &self.search_paths)
            .field("tools_count", &self.tools.len())
            .field("commands_count", &self.commands.len())
            .field("on_unload_hooks_count", &self.on_unload_hooks.len())
            .field("on_load_hooks_count", &self.on_load_hooks.len())
            .field(
                "error_log_len",
                &self.error_log.lock().map(|guard| guard.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        let lua = Lua::new();
        let error_log = Arc::new(Mutex::new(PluginErrorLog::new(100)));
        lua.set_app_data(Arc::clone(&error_log));
        if let Err(error) = spec::setup_spec_sandbox(&lua) {
            warn!("Failed to set up plugin runtime sandbox: {}", error);
        }

        Self {
            plugins: HashMap::new(),
            search_paths: Vec::new(),
            path_sources: HashMap::new(),
            tools: Vec::new(),
            commands: Vec::new(),
            lua,
            on_unload_hooks: HashMap::new(),
            on_load_hooks: HashMap::new(),
            error_log,
            discovery_errors: Vec::new(),
        }
    }

    /// Search paths for a standalone `PluginManager`: env override, then the
    /// user's plugin directory.
    ///
    /// **A kiln's `plugins/` is deliberately not here.** It used to be, which
    /// made this a second, divergent copy of the daemon's path list
    /// (`daemon_plugin_paths`: env → user → runtime, versus env → user → kiln
    /// here) — and because `initialize` loads what it discovers, every
    /// `session.create` executed the `init.lua` of every plugin in the kiln
    /// it was opening. That is `git clone` → arbitrary code execution in the
    /// daemon, the exact thing `docs/Help/Extending/Creating Plugins.md` says
    /// does not happen. It also bought nothing: `discover_plugins_for_kiln`
    /// drops this manager immediately, so the tools and handlers those plugins
    /// registered went into a VM nobody kept.
    ///
    /// A kiln's plugins load by putting the kiln on `runtimepath`, which is
    /// the one path list and is the user's own config saying so.
    pub fn with_standard_paths() -> Self {
        let mut paths = Vec::new();

        if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
            let separator = if cfg!(windows) { ';' } else { ':' };
            for p in env_paths.split(separator) {
                let path = PathBuf::from(p);
                if !p.is_empty() && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }

        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("crucible").join("plugins"));
        }

        Self::new().with_search_paths(paths)
    }

    /// Discover **and execute** every plugin on the standard paths.
    ///
    /// The execution is the point for callers that want a live VM, and a trap
    /// for callers that only want a listing — see [`Self::discover_only`].
    pub fn initialize() -> LifecycleResult<Self> {
        let mut manager = Self::with_standard_paths();
        manager.discover()?;
        manager.load_all()?;
        Ok(manager)
    }

    /// Discover without executing anything.
    ///
    /// What an enumeration wants. `lua.discover_plugins` used to answer with
    /// `initialize`, so listing the plugins ran all of them — a read-shaped
    /// RPC, reachable from the web UI, with arbitrary Lua as a side effect.
    /// Manifest metadata (name, version) comes from `plugin.yaml`, which needs
    /// no VM.
    pub fn discover_only() -> LifecycleResult<Self> {
        let mut manager = Self::with_standard_paths();
        manager.discover()?;
        Ok(manager)
    }

    pub fn with_search_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.search_paths = paths;
        self
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Add a search path with provenance tracking.
    pub fn add_search_path_with_source(&mut self, path: PathBuf, source: PluginSource) {
        self.path_sources.insert(path.clone(), source);
        self.add_search_path(path);
    }
}
