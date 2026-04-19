use super::{LifecycleError, LifecycleResult, PluginManager};
use crate::manifest::PluginState;
use tracing::{info, warn};

impl PluginManager {
    pub fn load(&mut self, name: &str) -> LifecycleResult<()> {
        let (is_enabled, required_deps, main_path, current_state) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

            let deps: Vec<String> = plugin
                .manifest
                .required_dependencies()
                .map(|d| d.name.clone())
                .collect();

            (
                plugin.manifest.is_enabled(),
                deps,
                plugin.main_path(),
                plugin.state,
            )
        };

        if current_state == PluginState::Active {
            return Err(LifecycleError::AlreadyLoaded(name.to_string()));
        }

        if !is_enabled {
            if let Some(plugin) = self.plugins.get_mut(name) {
                plugin.state = PluginState::Disabled;
            }
            info!("Plugin {} is disabled, skipping load", name);
            return Ok(());
        }

        for dep_name in &required_deps {
            if !self.plugins.contains_key(dep_name) {
                return Err(LifecycleError::DependencyNotSatisfied {
                    plugin: name.to_string(),
                    dependency: dep_name.clone(),
                });
            }

            let dep_plugin = &self.plugins[dep_name];
            if dep_plugin.state != PluginState::Active {
                return Err(LifecycleError::DependencyNotSatisfied {
                    plugin: name.to_string(),
                    dependency: dep_name.clone(),
                });
            }
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

    pub fn load_all(&mut self) -> LifecycleResult<Vec<String>> {
        let names: Vec<String> = self.plugins.keys().cloned().collect();
        let order = self.resolve_load_order(&names)?;

        let mut loaded = Vec::new();
        for name in order {
            match self.load(&name) {
                Ok(()) => loaded.push(name),
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

        for (other_name, other_plugin) in &self.plugins {
            if other_name == name {
                continue;
            }
            if other_plugin.state == PluginState::Active {
                for dep in &other_plugin.manifest.dependencies {
                    if dep.name == name && !dep.optional {
                        return Err(LifecycleError::LoadError(format!(
                            "Cannot unload {}: {} depends on it",
                            name, other_name
                        )));
                    }
                }
            }
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
        self.views
            .retain(|v| !v.item.source_path.starts_with(dir_prefix.as_ref()));
        self.handlers
            .retain(|h| !h.item.source_path.starts_with(dir_prefix.as_ref()));

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
        plugin.state = PluginState::Discovered;
        self.on_load_hooks.remove(name);
        info!("Unloaded plugin: {}", name);

        Ok(())
    }

    pub fn reload(&mut self, name: &str) -> LifecycleResult<()> {
        self.reload_plugin(name)
    }

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

    pub fn enable(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.manifest.enabled = Some(true);

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

        plugin.manifest.enabled = Some(false);
        plugin.state = PluginState::Disabled;

        Ok(())
    }
}
