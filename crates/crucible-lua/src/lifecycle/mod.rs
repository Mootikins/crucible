//! The registry of discovered plugins and the state of each.
//!
//! `PluginManager` holds no VM. The daemon VM is the only VM that runs
//! plugin code: discovery reads each fragment there (`discovery.rs`), and
//! the daemon's `activate` runs each entry file there. This type records
//! what discovery found and what the daemon did with it, so `plugin.list`
//! can answer.

mod discovery;
mod error;
mod error_log;
pub mod fragment;
mod spec;

#[cfg(test)]
mod tests;

use crate::manifest::{LoadedPlugin, PluginSource, PluginState};
use mlua::{Function, Lua, RegistryKey};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

pub use discovery::PluginDiscoveryError;
pub use error::{LifecycleError, LifecycleResult};
pub use error_log::{record_plugin_error, PluginErrorEntry, PluginErrorLog};
pub use fragment::{read_fragment, Fragment, FRAGMENT_FILE};
pub use spec::{spec_from_table, PluginSpec};

pub struct PluginManager {
    plugins: HashMap<String, LoadedPlugin>,
    search_paths: Vec<PathBuf>,
    /// Maps search paths to their provenance category.
    path_sources: HashMap<PathBuf, PluginSource>,
    /// The `on_load` and `on_unload` functions each active module carries.
    /// The keys belong to the daemon VM, so every call takes that VM.
    on_unload_hooks: HashMap<String, RegistryKey>,
    on_load_hooks: HashMap<String, RegistryKey>,
    /// Directories that failed to become plugins during `discover()`.
    ///
    /// A plugin whose fragment does not read never enters `plugins`, so it
    /// has no `PluginState` to mark `Error`. Before this the only trace was
    /// a `warn!` in the daemon log, which is how `reflection` stayed
    /// invisible.
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
            .field("on_unload_hooks_count", &self.on_unload_hooks.len())
            .field("on_load_hooks_count", &self.on_load_hooks.len())
            .finish()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            search_paths: Vec::new(),
            path_sources: HashMap::new(),
            on_unload_hooks: HashMap::new(),
            on_load_hooks: HashMap::new(),
            discovery_errors: Vec::new(),
        }
    }

    /// Discover on the standard paths without executing anything.
    ///
    /// What an enumeration wants. `lua.discover_plugins` used to answer with
    /// `initialize`, so listing the plugins ran all of them: a read-shaped
    /// RPC, reachable from the web UI, with arbitrary Lua as a side effect.
    /// The name comes from the directory. The version comes from the
    /// fragment, which `lua` evaluates in an environment that can act on
    /// nothing; a plugin without a fragment reports none.
    ///
    /// The standard paths are the env override, then the user's plugin
    /// directory. Both come from `crucible_core::paths` (`env_plugin_paths`,
    /// `user_plugins_dir`), which the daemon's `daemon_plugin_paths` reads
    /// too, so the two lists cannot diverge.
    ///
    /// **A kiln's `plugins/` is deliberately not here.** A kiln entry would
    /// make `git clone` put arbitrary directories on the path an enumeration
    /// walks. A kiln's plugins load by putting the kiln on `runtimepath`,
    /// which is the one path list and is the user's own config saying so.
    pub fn discover_only(lua: &Lua) -> LifecycleResult<Self> {
        let mut paths = crucible_core::paths::env_plugin_paths();
        paths.extend(crucible_core::paths::user_plugins_dir());
        let mut manager = Self::new();
        manager.search_paths = paths;
        manager.discover(lua)?;
        Ok(manager)
    }

    /// Replace the search paths. A test builder: production adds each path
    /// with its source (`add_search_path_with_source`), or reads the
    /// standard paths in `discover_only`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_search_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.search_paths = paths;
        self
    }

    /// Add a search path with provenance tracking.
    pub fn add_search_path_with_source(&mut self, path: PathBuf, source: PluginSource) {
        self.path_sources.insert(path.clone(), source);
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

    /// Record that the daemon activated `name`: state `Active`, no error.
    ///
    /// Activation is the daemon's act, in the daemon VM. This manager is the
    /// registry of what was discovered and what state each plugin is in, so
    /// the daemon tells it the outcome.
    pub fn mark_active(&mut self, name: &str) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.state = PluginState::Active;
            plugin.last_error = None;
            match plugin.version() {
                Some(version) => info!("Activated plugin: {name} v{version}"),
                None => info!("Activated plugin: {name} (no version declared)"),
            }
        }
    }

    /// Record that activation failed: state `Error`, with the reason.
    ///
    /// Without this the daemon could only `warn!`, and `plugin.list` kept
    /// reporting `Active` for a plugin whose `setup` raised.
    pub fn mark_error(&mut self, name: &str, error: impl Into<String>) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.state = PluginState::Error;
            plugin.last_error = Some(error.into());
        }
    }

    /// Record that the daemon deactivated `name`: state `Discovered`.
    ///
    /// The entry stays in the map so `plugin.list` keeps showing it. The
    /// `on_unload` hook is the daemon's to call, with the daemon VM, before
    /// it clears the plugin's registrations; see [`Self::call_on_unload_hook`].
    pub fn unload(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        if plugin.state != PluginState::Active {
            return Ok(());
        }

        plugin.state = PluginState::Discovered;
        self.on_load_hooks.remove(name);
        info!("Unloaded plugin: {}", name);

        Ok(())
    }

    /// Drop a plugin from the manager entirely: its map entry and its
    /// lifecycle hooks.
    ///
    /// `unload` deliberately leaves the entry in the map (state `Discovered`)
    /// so `plugin.list` keeps showing it. Removal wants it gone, and
    /// `discover()` skips names it already knows, so without this a removed
    /// plugin stayed listed forever and a reinstall registered nothing while
    /// it reported success. Callers deactivate first; `forget` does not.
    pub fn forget(&mut self, name: &str) {
        self.plugins.remove(name);
        self.on_load_hooks.remove(name);
        self.on_unload_hooks.remove(name);
    }

    /// Clear the operator's `disable`. The plugin is `Discovered` again, and
    /// the next activation pass may run it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn enable(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        if plugin.state == PluginState::Disabled {
            plugin.state = PluginState::Discovered;
        }

        Ok(())
    }

    /// The operator's kill switch: deactivate, then `Disabled`. The state
    /// holds until [`Self::enable`], and the daemon's `activate` refuses a
    /// disabled plugin.
    pub fn disable(&mut self, name: &str) -> LifecycleResult<()> {
        self.unload(name)?;

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.state = PluginState::Disabled;

        Ok(())
    }

    /// Remember the hooks a module table carries. A `None` drops the hook
    /// the previous generation left, so a reload that removed `on_unload`
    /// from its table does not fire the old one.
    pub fn set_lifecycle_hooks(
        &mut self,
        plugin_name: &str,
        on_load: Option<RegistryKey>,
        on_unload: Option<RegistryKey>,
    ) {
        match on_load {
            Some(key) => {
                self.on_load_hooks.insert(plugin_name.to_string(), key);
            }
            None => {
                self.on_load_hooks.remove(plugin_name);
            }
        }
        match on_unload {
            Some(key) => {
                self.on_unload_hooks.insert(plugin_name.to_string(), key);
            }
            None => {
                self.on_unload_hooks.remove(plugin_name);
            }
        }
    }

    /// Call the plugin's `on_load`, if it has one. The key stays, so a
    /// later reload that reuses the table fires it again.
    ///
    /// A hook that raises is logged and recorded in the VM's error log, and
    /// the activation goes on: `docs/Help/Extending/Creating Plugins.md`
    /// states that contract.
    pub fn call_on_load_hook(&self, lua: &Lua, plugin_name: &str) {
        let Some(hook_key) = self.on_load_hooks.get(plugin_name) else {
            return;
        };
        call_hook(lua, hook_key, plugin_name, "on_load");
    }

    /// Call the plugin's `on_unload`, if it has one, and forget it. One
    /// generation, one call: a second unload of the same generation fires
    /// nothing.
    pub fn call_on_unload_hook(&mut self, lua: &Lua, plugin_name: &str) {
        let Some(hook_key) = self.on_unload_hooks.remove(plugin_name) else {
            return;
        };
        call_hook(lua, &hook_key, plugin_name, "on_unload");
    }
}

/// Resolve `hook_key` on `lua`, call it, and record a raise. The load or
/// unload goes on either way.
fn call_hook(lua: &Lua, hook_key: &RegistryKey, plugin_name: &str, hook: &str) {
    match lua.registry_value::<Function>(hook_key) {
        Ok(callback) => {
            if let Err(error) = callback.call::<()>(()) {
                warn!("{hook} hook failed for {plugin_name}: {error}");
                record_plugin_error(
                    lua,
                    plugin_name,
                    &error,
                    format!("handler:{hook}:{plugin_name}"),
                );
            }
        }
        Err(error) => {
            warn!("Failed to retrieve {hook} hook for {plugin_name}: {error}");
        }
    }
}
