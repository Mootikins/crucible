//! Plugin lifecycle management

mod builders;
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

use crate::annotations::{DiscoveredCommand, DiscoveredHandler, DiscoveredTool, DiscoveredView};
use crate::manifest::{LoadedPlugin, PluginSource};
use mlua::{Lua, RegistryKey};
use registration::RegisteredItem;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::warn;

pub use builders::{CommandBuilder, HandlerBuilder, ToolBuilder, ViewBuilder};
pub use error::{LifecycleError, LifecycleResult};
pub use error_log::{PluginErrorEntry, PluginErrorLog};
pub use registration::RegistrationHandle;
pub use spec::{load_plugin_spec, load_plugin_spec_from_source, PluginSpec};

pub struct PluginManager {
    plugins: HashMap<String, LoadedPlugin>,
    search_paths: Vec<PathBuf>,
    /// Maps search paths to their provenance category.
    path_sources: HashMap<PathBuf, PluginSource>,
    tools: Vec<RegisteredItem<DiscoveredTool>>,
    commands: Vec<RegisteredItem<DiscoveredCommand>>,
    views: Vec<RegisteredItem<DiscoveredView>>,
    handlers: Vec<RegisteredItem<DiscoveredHandler>>,
    lua: Lua,
    on_unload_hooks: HashMap<String, RegistryKey>,
    on_load_hooks: HashMap<String, RegistryKey>,
    error_log: Arc<Mutex<PluginErrorLog>>,
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
            .field("views_count", &self.views.len())
            .field("handlers_count", &self.handlers.len())
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
            views: Vec::new(),
            handlers: Vec::new(),
            lua,
            on_unload_hooks: HashMap::new(),
            on_load_hooks: HashMap::new(),
            error_log,
        }
    }

    pub fn with_standard_paths(kiln_path: Option<&Path>) -> Self {
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

        if let Some(kiln) = kiln_path {
            paths.push(kiln.join("plugins"));
        }

        Self::new().with_search_paths(paths)
    }

    pub fn initialize(kiln_path: Option<&Path>) -> LifecycleResult<Self> {
        let mut manager = Self::with_standard_paths(kiln_path);
        manager.discover()?;
        manager.load_all()?;
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
