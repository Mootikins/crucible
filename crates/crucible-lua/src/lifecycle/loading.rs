use super::{LifecycleError, LifecycleResult, PluginManager};
use crate::manifest::PluginState;
use tracing::{info, warn};

impl PluginManager {
    pub fn load(&mut self, name: &str) -> LifecycleResult<()> {
        let (main_path, current_state) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

            (plugin.main_path(), plugin.state)
        };

        if current_state == PluginState::Active {
            return Err(LifecycleError::AlreadyLoaded(name.to_string()));
        }

        // A plugin already marked disabled stays that way until something
        // enables it. Being ON the runtimepath is what enables a plugin —
        // there is no `enabled` field a plugin declares about itself — so
        // this is the operator's `disable()` taking effect, nothing else.
        if current_state == PluginState::Disabled {
            info!("Plugin {name} is disabled, skipping load");
            return Ok(());
        }

        if !main_path.exists() {
            return Err(LifecycleError::LoadError(format!(
                "Main file not found: {}",
                main_path.display()
            )));
        }

        self.discover_exports_for_plugin(name)?;
        self.load_plugin_runtime_state(name)?;

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
        plugin.state = PluginState::Active;
        plugin.last_error = None;
        info!("Loaded plugin: {} v{}", name, plugin.version());

        self.call_on_load_hook(name);

        Ok(())
    }

    /// Mark a plugin as failed after `load()` already succeeded.
    ///
    /// `PluginManager::load` only covers its own stages. The daemon executes
    /// each plugin a second time in the *real* Lua VM and calls its `setup()`
    /// — neither of which the spec sandbox does — so a plugin can be `Active`
    /// here while having blown up there. Without this the daemon could only
    /// `warn!`, and `plugin.list` kept reporting `Active`.
    pub fn mark_error(&mut self, name: &str, error: impl Into<String>) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.state = PluginState::Error;
            plugin.last_error = Some(error.into());
        }
    }

    pub fn load_all(&mut self) -> LifecycleResult<Vec<String>> {
        // No dependency ordering. A plugin does not declare that another must
        // load first: in Vim's model a dependency is another entry on the
        // runtimepath, which the user adds, not a runtime contract between
        // plugins. `resolve_load_order` and the manifest `dependencies:` list
        // it read are both gone.
        //
        // Sorted so the order is at least deterministic across runs.
        let mut order: Vec<String> = self.plugins.keys().cloned().collect();
        order.sort();

        let mut loaded = Vec::new();
        for name in order {
            match self.load(&name) {
                // `load` returns Ok for a disabled plugin too — it is not an
                // error to skip one. Report only what actually became Active,
                // or callers iterating this list will execute plugins the
                // operator has switched off.
                Ok(()) => {
                    if self.plugins.get(&name).map(|p| p.state) == Some(PluginState::Active) {
                        loaded.push(name);
                    }
                }
                Err(LifecycleError::AlreadyLoaded(_)) => {}
                Err(e) => {
                    warn!("Failed to load plugin {}: {}", name, e);
                    if let Some(plugin) = self.plugins.get_mut(&name) {
                        plugin.state = PluginState::Error;
                        plugin.last_error = Some(e.to_string());
                    }
                }
            }
        }

        Ok(loaded)
    }

    pub fn unload(&mut self, name: &str) -> LifecycleResult<()> {
        let (current_state, plugin_dir) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
            (plugin.state, plugin.dir.clone())
        };

        if current_state != PluginState::Active {
            return Ok(());
        }

        self.call_on_unload_hook(name);

        // Clean up global emitter listeners registered by this plugin
        if let Err(e) = self.lua.load(format!(
            r#"local _e = cru.emitter.global(); if _e.unregister_owner then _e:unregister_owner({name:?}) end"#
        )).exec() {
            warn!("Failed to clean up global emitter for {}: {}", name, e);
            self.capture_plugin_error(name, &e, "unload:emitter_cleanup");
        }

        let dir_prefix = plugin_dir.to_string_lossy();
        self.tools
            .retain(|t| !t.item.source_path.starts_with(dir_prefix.as_ref()));
        self.commands
            .retain(|c| !c.item.source_path.starts_with(dir_prefix.as_ref()));

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
        plugin.state = PluginState::Discovered;
        self.on_load_hooks.remove(name);
        info!("Unloaded plugin: {}", name);

        Ok(())
    }

    /// Drop a plugin from the manager entirely: its map entry, its
    /// owner-tagged registrations, and its lifecycle hooks.
    ///
    /// `unload` deliberately leaves the entry in the map (state `Discovered`)
    /// so `plugin.list` keeps showing it — but removal wants it gone, and
    /// `discover()` skips names it already knows, so without this a removed
    /// plugin stayed listed forever and reinstalling it loaded nothing while
    /// reporting success. Callers deactivate first (`unload` runs the
    /// dependent check and the Lua-side cleanup); `forget` does neither.
    pub fn forget(&mut self, name: &str) {
        self.plugins.remove(name);
        // `unload` only cleans registrations for Active plugins; a plugin
        // being forgotten from Error/Disabled may still have owner-tagged
        // spec exports lying around.
        self.unregister_by_owner(name);
        self.on_load_hooks.remove(name);
        self.on_unload_hooks.remove(name);
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn reload_plugin(&mut self, name: &str) -> LifecycleResult<()> {
        self.unload(name)?;
        self.clear_plugin_modules(name)?;

        match self.load(name) {
            Ok(()) => Ok(()),
            Err(reload_error) => {
                if let Some(plugin) = self.plugins.get_mut(name) {
                    plugin.state = PluginState::Error;
                    plugin.last_error = Some(reload_error.to_string());
                }

                Err(reload_error)
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn enable(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.state = PluginState::Discovered;

        if plugin.state == PluginState::Disabled {
            self.load(name)?;
        }

        Ok(())
    }

    pub fn disable(&mut self, name: &str) -> LifecycleResult<()> {
        self.unload(name)?;

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.state = PluginState::Disabled;

        Ok(())
    }
}
